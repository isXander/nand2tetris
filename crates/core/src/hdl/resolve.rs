//! Locating project-local chip sources.
//!
//! Chip lookup is source-first: `Xxx.hdl` in the project directory is used if
//! present (even if it is a stub or broken), and only a genuine *not found*
//! falls back to the built-in library. Built-in fallback is handled by the
//! elaborator; providers only report what exists in the project.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceFile {
    /// A stable identity for the source, e.g. a canonical path or editor URI.
    pub id: String,
    pub text: Arc<str>,
}

impl SourceFile {
    pub fn new(id: impl Into<String>, text: impl Into<Arc<str>>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceLookup {
    Found(SourceFile),
    /// No source exists; `searched` describes the locations examined.
    NotFound {
        searched: Vec<String>,
    },
    /// A source exists but could not be read. This is *not* a reason to fall
    /// back to a built-in.
    Error {
        id: String,
        message: String,
    },
}

/// Provides project-local chip sources by chip name.
pub trait SourceProvider {
    fn load_chip(&self, chip_name: &str) -> SourceLookup;
}

impl<T: SourceProvider + ?Sized> SourceProvider for &T {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        (**self).load_chip(chip_name)
    }
}

impl<T: SourceProvider + ?Sized> SourceProvider for Box<T> {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        (**self).load_chip(chip_name)
    }
}

impl<T: SourceProvider + ?Sized> SourceProvider for Arc<T> {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        (**self).load_chip(chip_name)
    }
}

/// Whether `name` can be a chip name (and therefore a safe file stem).
pub fn is_valid_chip_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reads `<project_dir>/<Name>.hdl`, enforcing exact-case file names even on
/// case-insensitive file systems.
#[derive(Debug, Clone)]
pub struct FsSourceProvider {
    project_dir: PathBuf,
}

impl FsSourceProvider {
    pub fn new(project_dir: impl Into<PathBuf>) -> Self {
        let project_dir = project_dir.into();
        let project_dir = project_dir.canonicalize().unwrap_or(project_dir);
        Self { project_dir }
    }

    pub fn project_dir(&self) -> &Path {
        &self.project_dir
    }

    /// The path a chip's source would have.
    pub fn chip_path(&self, chip_name: &str) -> PathBuf {
        self.project_dir.join(format!("{chip_name}.hdl"))
    }
}

impl SourceProvider for FsSourceProvider {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        let path = self.chip_path(chip_name);
        let display = path.display().to_string();
        if !is_valid_chip_name(chip_name) {
            return SourceLookup::NotFound {
                searched: vec![display],
            };
        }
        let file_name = format!("{chip_name}.hdl");
        let exact_entry_exists = match std::fs::read_dir(&self.project_dir) {
            Ok(entries) => entries
                .filter_map(Result::ok)
                .any(|e| e.file_name().to_str() == Some(file_name.as_str())),
            Err(e) => {
                return SourceLookup::Error {
                    id: self.project_dir.display().to_string(),
                    message: format!("cannot read project directory: {e}"),
                };
            }
        };
        if !exact_entry_exists {
            return SourceLookup::NotFound {
                searched: vec![display],
            };
        }
        match std::fs::read_to_string(&path) {
            Ok(text) => SourceLookup::Found(SourceFile::new(display, text)),
            Err(e) => SourceLookup::Error {
                id: display,
                message: e.to_string(),
            },
        }
    }
}

/// An in-memory set of sources keyed by chip name.
#[derive(Debug, Clone, Default)]
pub struct MemorySourceProvider {
    files: HashMap<String, SourceFile>,
}

impl MemorySourceProvider {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a source using `<chip_name>.hdl` as its identity.
    pub fn with(mut self, chip_name: &str, text: &str) -> Self {
        self.insert(chip_name, SourceFile::new(format!("{chip_name}.hdl"), text));
        self
    }

    pub fn insert(&mut self, chip_name: impl Into<String>, file: SourceFile) {
        self.files.insert(chip_name.into(), file);
    }

    pub fn remove(&mut self, chip_name: &str) -> Option<SourceFile> {
        self.files.remove(chip_name)
    }
}

impl SourceProvider for MemorySourceProvider {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        match self.files.get(chip_name) {
            Some(file) => SourceLookup::Found(file.clone()),
            None => SourceLookup::NotFound {
                searched: vec![format!("{chip_name}.hdl (in memory)")],
            },
        }
    }
}

/// Layers unsaved editor buffers over another provider.
#[derive(Debug, Clone)]
pub struct OverlaySourceProvider<P> {
    overlays: HashMap<String, SourceFile>,
    inner: P,
}

impl<P> OverlaySourceProvider<P> {
    pub fn new(inner: P) -> Self {
        Self {
            overlays: HashMap::new(),
            inner,
        }
    }

    pub fn set_overlay(&mut self, chip_name: impl Into<String>, file: SourceFile) {
        self.overlays.insert(chip_name.into(), file);
    }

    pub fn remove_overlay(&mut self, chip_name: &str) -> Option<SourceFile> {
        self.overlays.remove(chip_name)
    }

    pub fn inner(&self) -> &P {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }
}

impl<P: SourceProvider> SourceProvider for OverlaySourceProvider<P> {
    fn load_chip(&self, chip_name: &str) -> SourceLookup {
        match self.overlays.get(chip_name) {
            Some(file) => SourceLookup::Found(file.clone()),
            None => self.inner.load_chip(chip_name),
        }
    }
}
