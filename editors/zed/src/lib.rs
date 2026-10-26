//! Zed extension for Nand to Tetris: wires the `n2t lsp` language server to
//! the HDL and test-script languages, and styles its completions.

use std::fs;

use zed_extension_api::{
    self as zed, CodeLabel, CodeLabelSpan, LanguageServerId, LanguageServerInstallationStatus,
    Range, Result, Worktree,
    lsp::{Completion, CompletionKind},
    settings::LspSettings,
};

const SERVER_NAME: &str = "n2t";
const BINARY_NAME: &str = "n2t";
const GITHUB_REPOSITORY: &str = "isXander/nand2tetris";

struct Nand2TetrisExtension {
    /// A binary downloaded by this extension during this session.
    cached_binary: Option<String>,
}

impl Nand2TetrisExtension {
    fn binary_name() -> String {
        match zed::current_platform().0 {
            zed::Os::Windows => format!("{BINARY_NAME}.exe"),
            _ => BINARY_NAME.to_string(),
        }
    }

    /// Locates the `n2t` binary: an explicit `lsp.n2t.binary.path` setting,
    /// then `n2t` on the worktree's `PATH`, then a release downloaded from
    /// GitHub.
    fn binary_path(&mut self, id: &LanguageServerId, worktree: &Worktree) -> Result<String> {
        if let Some(path) = LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|s| s.binary)
            .and_then(|b| b.path)
        {
            return Ok(path);
        }
        if let Some(path) = worktree.which(BINARY_NAME) {
            return Ok(path);
        }
        if let Some(path) = &self.cached_binary
            && fs::metadata(path).is_ok_and(|m| m.is_file())
        {
            return Ok(path.clone());
        }
        match self.download(id) {
            Ok(path) => {
                self.cached_binary = Some(path.clone());
                Ok(path)
            }
            Err(error) => {
                zed::set_language_server_installation_status(
                    id,
                    &LanguageServerInstallationStatus::Failed(error.clone()),
                );
                Err(format!(
                    "could not find `{BINARY_NAME}` on PATH and could not download it ({error}). \
                     Install it with `cargo install --path crates/cli` from the nand2tetris \
                     repository, or set `lsp.n2t.binary.path` in your settings."
                ))
            }
        }
    }

    /// Downloads the latest release asset named
    /// `n2t-<arch>-<target>.tar.gz` (or `.zip` on Windows).
    fn download(&self, id: &LanguageServerId) -> Result<String> {
        zed::set_language_server_installation_status(
            id,
            &LanguageServerInstallationStatus::CheckingForUpdate,
        );
        let release = zed::latest_github_release(
            GITHUB_REPOSITORY,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )?;

        let (os, arch) = zed::current_platform();
        let arch = match arch {
            zed::Architecture::Aarch64 => "aarch64",
            zed::Architecture::X8664 => "x86_64",
            zed::Architecture::X86 => return Err("32-bit x86 is not supported".into()),
        };
        let (target, extension, file_type) = match os {
            zed::Os::Mac => ("apple-darwin", "tar.gz", zed::DownloadedFileType::GzipTar),
            zed::Os::Linux => (
                "unknown-linux-gnu",
                "tar.gz",
                zed::DownloadedFileType::GzipTar,
            ),
            zed::Os::Windows => ("pc-windows-msvc", "zip", zed::DownloadedFileType::Zip),
        };
        let asset_name = format!("{BINARY_NAME}-{arch}-{target}.{extension}");
        let asset = release
            .assets
            .iter()
            .find(|a| a.name == asset_name)
            .ok_or_else(|| format!("release {} has no asset `{asset_name}`", release.version))?;

        let version_dir = format!("{BINARY_NAME}-{}", release.version);
        let binary = format!("{version_dir}/{}", Self::binary_name());
        if !fs::metadata(&binary).is_ok_and(|m| m.is_file()) {
            zed::set_language_server_installation_status(
                id,
                &LanguageServerInstallationStatus::Downloading,
            );
            zed::download_file(&asset.download_url, &version_dir, file_type)?;
            zed::make_file_executable(&binary)?;

            // Remove older downloads.
            if let Ok(entries) = fs::read_dir(".") {
                for entry in entries.flatten() {
                    let name = entry.file_name().to_string_lossy().into_owned();
                    if name.starts_with(&format!("{BINARY_NAME}-")) && name != version_dir {
                        fs::remove_dir_all(entry.path()).ok();
                    }
                }
            }
        }
        zed::set_language_server_installation_status(id, &LanguageServerInstallationStatus::None);
        Ok(binary)
    }
}

impl zed::Extension for Nand2TetrisExtension {
    fn new() -> Self {
        Self {
            cached_binary: None,
        }
    }

    fn language_server_command(
        &mut self,
        id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<zed::Command> {
        let settings = LspSettings::for_worktree(SERVER_NAME, worktree).unwrap_or_default();
        let binary = settings.binary;
        let command = self.binary_path(id, worktree)?;
        let args = binary
            .as_ref()
            .and_then(|b| b.arguments.clone())
            .unwrap_or_else(|| vec!["lsp".to_string()]);
        let mut env = worktree.shell_env();
        if let Some(extra) = binary.and_then(|b| b.env) {
            env.retain(|(key, _)| !extra.contains_key(key));
            env.extend(extra);
        }
        Ok(zed::Command { command, args, env })
    }

    fn language_server_initialization_options(
        &mut self,
        _id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|s| s.initialization_options))
    }

    fn language_server_workspace_configuration(
        &mut self,
        _id: &LanguageServerId,
        worktree: &Worktree,
    ) -> Result<Option<zed::serde_json::Value>> {
        Ok(LspSettings::for_worktree(SERVER_NAME, worktree)
            .ok()
            .and_then(|s| s.settings))
    }

    fn label_for_completion(
        &self,
        _id: &LanguageServerId,
        completion: Completion,
    ) -> Option<CodeLabel> {
        let highlight = match completion.kind? {
            CompletionKind::Class | CompletionKind::Module => "type",
            CompletionKind::Field => "property",
            CompletionKind::Property => "property",
            CompletionKind::Variable => "variable",
            CompletionKind::Constant => "boolean",
            CompletionKind::Keyword => "keyword",
            CompletionKind::File => "string.special",
            _ => return None,
        };
        let label = completion.label;
        let mut spans = vec![CodeLabelSpan::literal(
            label.clone(),
            Some(highlight.into()),
        )];
        if let Some(detail) = completion.detail.filter(|d| !d.is_empty()) {
            // Chip signatures repeat the name: show only the pin lists.
            let detail = detail
                .strip_prefix(label.as_str())
                .unwrap_or(&detail)
                .to_string();
            spans.push(CodeLabelSpan::literal(" ", None));
            spans.push(CodeLabelSpan::literal(detail, Some("comment".into())));
        }
        Some(CodeLabel {
            code: String::new(),
            spans,
            filter_range: Range {
                start: 0,
                end: label.len() as u32,
            },
        })
    }
}

zed::register_extension!(Nand2TetrisExtension);
