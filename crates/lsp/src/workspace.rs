//! Open documents and per-folder analysis projects.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lsp_types as lsp;
use lsp_types::Url;
use n2t_core::LineIndex;
use n2t_core::builtins::{BuiltinChip, BuiltinLibrary};
use n2t_core::hdl::resolve::{
    FsSourceProvider, OverlaySourceProvider, SourceFile, is_valid_chip_name,
};
use n2t_core::hdl::{ChipUnit, Elaborator, ElaboratorOptions, UnitOrigin};
use n2t_core::source::Span;
use n2t_core::tst::{ParsedScript, ScriptAnalysis};

use crate::convert::{self, Text, canonical_path, path_to_url, url_to_path};

pub type ProjectElaborator = Elaborator<OverlaySourceProvider<FsSourceProvider>>;

/// All chips in one folder share a resolution context (the folder is the
/// "project directory" of the specification).
pub struct Project {
    pub dir: PathBuf,
    pub elaborator: ProjectElaborator,
}

impl Project {
    fn new(dir: PathBuf, builtins: Arc<BuiltinLibrary>) -> Self {
        let provider = OverlaySourceProvider::new(FsSourceProvider::new(&dir));
        Self {
            dir,
            elaborator: Elaborator::with_options(provider, builtins, ElaboratorOptions::editor()),
        }
    }

    /// The source identity the file-system provider uses for a chip.
    pub fn source_id(&self, chip_name: &str) -> String {
        self.elaborator
            .provider()
            .inner()
            .chip_path(chip_name)
            .display()
            .to_string()
    }

    /// Names of chips with a `.hdl` file in the folder.
    pub fn local_chips(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .filter_map(|e| {
                let name = e.file_name().into_string().ok()?;
                let stem = name.strip_suffix(".hdl")?;
                is_valid_chip_name(stem).then(|| stem.to_string())
            })
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Local chips plus built-ins not shadowed by a local file.
    pub fn all_chips(&self) -> Vec<String> {
        let mut names = self.local_chips();
        for chip in self.elaborator.builtins().iter() {
            if !names.contains(&chip.name) {
                names.push(chip.name.clone());
            }
        }
        names.sort();
        names
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocKind {
    Hdl,
    Tst,
    Other,
}

pub struct Document {
    pub uri: Url,
    pub version: i32,
    pub text: Arc<str>,
    pub lines: LineIndex,
    pub kind: DocKind,
    /// Canonical file path, for `file:` documents.
    pub path: Option<PathBuf>,
    /// The project folder.
    pub dir: PathBuf,
    /// The file stem (the chip name for `.hdl` files).
    pub stem: String,
    pub tst: Option<(ParsedScript, ScriptAnalysis)>,
}

impl Document {
    pub fn text(&self) -> Text<'_> {
        Text::new(&self.text, &self.lines)
    }

    /// Whether this HDL document is analysed through the project overlay
    /// (its stem is a valid chip name).
    pub fn is_overlay_chip(&self) -> bool {
        self.kind == DocKind::Hdl && is_valid_chip_name(&self.stem)
    }
}

pub struct Workspace {
    pub docs: HashMap<Url, Document>,
    pub projects: HashMap<PathBuf, Project>,
    pub builtins: Arc<BuiltinLibrary>,
    builtin_dir: PathBuf,
    untitled_dir: PathBuf,
}

impl Workspace {
    pub fn new() -> Self {
        let base = std::env::temp_dir().join(format!("n2t-lsp-{}", env!("CARGO_PKG_VERSION")));
        Self {
            docs: HashMap::new(),
            projects: HashMap::new(),
            builtins: Arc::new(BuiltinLibrary::standard()),
            builtin_dir: base.join("builtins"),
            untitled_dir: base.join("untitled"),
        }
    }

    pub fn project(&mut self, dir: &Path) -> &mut Project {
        let builtins = self.builtins.clone();
        self.projects
            .entry(dir.to_path_buf())
            .or_insert_with(|| Project::new(dir.to_path_buf(), builtins))
    }

    // ---------------------------------------------------------------------
    // Document lifecycle
    // ---------------------------------------------------------------------

    pub fn open(&mut self, uri: Url, text: String, version: i32) {
        let path = url_to_path(&uri).map(|p| canonical_path(&p));
        let file_name = path
            .as_ref()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_string)
            .unwrap_or_else(|| {
                uri.path_segments()
                    .and_then(|mut s| s.next_back())
                    .unwrap_or("")
                    .to_string()
            });
        let (stem, extension) = match file_name.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), e.to_ascii_lowercase()),
            None => (file_name.clone(), String::new()),
        };
        let kind = match extension.as_str() {
            "hdl" => DocKind::Hdl,
            "tst" => DocKind::Tst,
            _ => DocKind::Other,
        };
        let dir = match path.as_ref().and_then(|p| p.parent()) {
            Some(dir) => dir.to_path_buf(),
            None => {
                let _ = std::fs::create_dir_all(&self.untitled_dir);
                canonical_path(&self.untitled_dir)
            }
        };
        let text: Arc<str> = Arc::from(text);
        let doc = Document {
            uri: uri.clone(),
            version,
            lines: LineIndex::new(&text),
            text,
            kind,
            path,
            dir,
            stem,
            tst: None,
        };
        self.docs.insert(uri.clone(), doc);
        self.sync_overlay(&uri);
    }

    pub fn change(&mut self, uri: &Url, text: String, version: i32) {
        if let Some(doc) = self.docs.get_mut(uri) {
            doc.text = Arc::from(text);
            doc.lines = LineIndex::new(&doc.text);
            doc.version = version;
            doc.tst = None;
        }
        self.sync_overlay(uri);
    }

    pub fn close(&mut self, uri: &Url) {
        if let Some(doc) = self.docs.remove(uri)
            && doc.is_overlay_chip()
        {
            let stem = doc.stem.clone();
            self.project(&doc.dir)
                .elaborator
                .provider_mut()
                .remove_overlay(&stem);
        }
    }

    fn sync_overlay(&mut self, uri: &Url) {
        let Some(doc) = self.docs.get(uri) else {
            return;
        };
        if !doc.is_overlay_chip() {
            return;
        }
        let (dir, stem, text) = (doc.dir.clone(), doc.stem.clone(), doc.text.clone());
        let project = self.project(&dir);
        let id = project.source_id(&stem);
        project
            .elaborator
            .provider_mut()
            .set_overlay(stem, SourceFile::new(id, text));
    }

    /// Open documents sharing a project folder.
    pub fn docs_in(&self, dir: &Path) -> Vec<Url> {
        self.docs
            .values()
            .filter(|d| d.dir == dir)
            .map(|d| d.uri.clone())
            .collect()
    }

    // ---------------------------------------------------------------------
    // Analysis
    // ---------------------------------------------------------------------

    /// The compiled unit for an HDL document.
    pub fn hdl_unit(&mut self, uri: &Url) -> Option<Arc<ChipUnit>> {
        let doc = self.docs.get(uri)?;
        if doc.kind != DocKind::Hdl {
            return None;
        }
        let (dir, stem, text) = (doc.dir.clone(), doc.stem.clone(), doc.text.clone());
        let overlay = doc.is_overlay_chip();
        let project = self.project(&dir);
        if overlay {
            Some(project.elaborator.compile(&stem))
        } else {
            let file = SourceFile::new(uri.to_string(), text);
            Some(Arc::new(project.elaborator.compile_file(None, file)))
        }
    }

    /// Ensures the script analysis of a TST document is up to date.
    pub fn tst_state(&mut self, uri: &Url) -> Option<&(ParsedScript, ScriptAnalysis)> {
        let doc = self.docs.get(uri)?;
        if doc.kind != DocKind::Tst {
            return None;
        }
        if doc.tst.is_none() {
            let (dir, text) = (doc.dir.clone(), doc.text.clone());
            let parsed = n2t_core::tst::parse(&text);
            let analysis =
                n2t_core::tst::analyze(&parsed.script, &mut self.project(&dir).elaborator);
            self.docs.get_mut(uri)?.tst = Some((parsed, analysis));
        }
        self.docs.get(uri)?.tst.as_ref()
    }

    /// Discards cached script analyses in a folder (after an HDL edit).
    pub fn invalidate_scripts(&mut self, dir: &Path) {
        for doc in self.docs.values_mut() {
            if doc.dir == dir {
                doc.tst = None;
            }
        }
    }

    pub fn diagnostics(&mut self, uri: &Url) -> Vec<lsp::Diagnostic> {
        let Some(kind) = self.docs.get(uri).map(|d| d.kind) else {
            return Vec::new();
        };
        let core: Vec<n2t_core::Diagnostic> = match kind {
            DocKind::Hdl => self
                .hdl_unit(uri)
                .map(|u| u.diagnostics.clone())
                .unwrap_or_default(),
            DocKind::Tst => self
                .tst_state(uri)
                .map(|(p, a)| {
                    p.diagnostics
                        .iter()
                        .chain(&a.diagnostics)
                        .cloned()
                        .collect()
                })
                .unwrap_or_default(),
            DocKind::Other => Vec::new(),
        };
        let doc = &self.docs[uri];
        let text = doc.text();
        let docs = &self.docs;
        let mut resolve = |id: &str| source_by_id(docs, id);
        core.iter()
            .map(|d| convert::diagnostic(uri, &text, d, &mut resolve))
            .collect()
    }

    // ---------------------------------------------------------------------
    // Locations
    // ---------------------------------------------------------------------

    /// The URL and text of a source identity (an open document or a file).
    pub fn source(&self, id: &str) -> Option<(Url, String)> {
        source_by_id(&self.docs, id)
    }

    /// A location for a span in a chip unit's source (local or built-in).
    pub fn unit_location(&self, unit: &ChipUnit, span: Span) -> Option<lsp::Location> {
        match &unit.origin {
            UnitOrigin::Source(file) => {
                let (url, _) = self.source(&file.id)?;
                Some(lsp::Location::new(url, convert::range_in(&file.text, span)))
            }
            UnitOrigin::Builtin(chip) => self.builtin_location(chip, span),
            _ => None,
        }
    }

    /// Writes a built-in's interface text to a cache file so editors can open it.
    pub fn builtin_location(&self, chip: &BuiltinChip, span: Span) -> Option<lsp::Location> {
        let path = self.builtin_dir.join(format!("{}.hdl", chip.name));
        let current = std::fs::read_to_string(&path).ok();
        if current.as_deref() != Some(&*chip.source) {
            std::fs::create_dir_all(&self.builtin_dir).ok()?;
            std::fs::write(&path, &*chip.source).ok()?;
        }
        let url = path_to_url(&canonical_path(&path))?;
        Some(lsp::Location::new(
            url,
            convert::range_in(&chip.source, span),
        ))
    }
}

fn source_by_id(docs: &HashMap<Url, Document>, id: &str) -> Option<(Url, String)> {
    let path = PathBuf::from(id);
    if let Some(doc) = docs
        .values()
        .find(|d| d.path.as_deref() == Some(path.as_path()) || d.uri.as_str() == id)
    {
        return Some((doc.uri.clone(), doc.text.to_string()));
    }
    let text = std::fs::read_to_string(&path).ok()?;
    Some((path_to_url(&path)?, text))
}
