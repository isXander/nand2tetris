# Nand2Tetris for IntelliJ IDEA

A lightweight Kotlin plugin that connects IDEA to `n2t lsp` through
[LSP4IJ](https://github.com/redhat-developer/lsp4ij). It supports IntelliJ IDEA
2025.2 and newer, including Community Edition.

- TextMate highlighting for `.hdl`, `.tst`, `.cmp` and `.out` from the
  [shared grammars](../../grammars/README.md).
- Diagnostics, completion, hover, definition, references, rename, inlay hints
  and semantic highlighting supplied by the language server and LSP4IJ.
- A project setting for the `n2t` executable. File-pattern mappings preserve
  TextMate highlighting without a separate IDEA parser or file type.

## Install

Build the plugin:

```sh
bun run build:idea
```

In IDEA, install **LSP4IJ** from Settings → Plugins → Marketplace and enable the
bundled **TextMate Bundles** plugin. Use **Install Plugin from Disk** to select
`editors/idea/build/distributions/nand2tetris-0.1.0.zip`, then restart if requested.

Open an `.hdl` or `.tst` file to start the language server. Like the Zed extension,
the plugin resolves the executable in this order:

1. The path under **Settings → Languages & Frameworks → Nand2Tetris**.
2. `n2t` on `PATH` (`n2t.exe` on Windows).
3. The latest stable GitHub release of `isXander/nand2tetris`, using
   `n2t-<arch>-<target>.tar.gz` (`.zip` on Windows), with the executable at the
   archive root. Supported architectures are `aarch64` and `x86_64`; targets are
   `apple-darwin`, `unknown-linux-gnu` and `pc-windows-msvc`, matching Zed.

Downloads run during background server startup and are cached under IDEA's system
cache directory in `nand2tetris/servers/<target>/<release-tag>`. The plugin checks
for a new release once per IDE session when no configured or PATH binary exists.
If GitHub is unavailable, it reuses the previous successful download. Failures
appear in LSP4IJ's server logs with instructions for setting a local executable.

Until a GitHub release with matching assets exists, install the server with
`cargo install --path crates/cli` from the repository root or configure a local
binary. The live GitHub download flow has not yet been verified against a release.

Enter the executable only when overriding its path; the plugin adds `lsp` and
uses the project directory as the working directory. Applying a changed executable
setting restarts the server for open matching files.

Use LSP4IJ's **LSP Consoles** tool window for server logs and restart controls,
and **Settings → Languages & Frameworks → Language Servers** for LSP4IJ options.

## Development

Requires JDK 21 or newer on `PATH` and Bun. The checked-in Gradle wrapper is used;
a separate Gradle installation is unnecessary. The first build downloads the
IntelliJ SDK and plugin dependencies.

```sh
bun run build:idea     # package the plugin ZIP
bun run idea:run       # launch an isolated IDEA sandbox with both dependencies
bun run idea:verify    # validate plugin structure
```

The module uses IntelliJ's Platform Gradle plugin and compiles Kotlin to JVM 21.
It packages `grammars/textmate` directly into the plugin's `textmate/nand2tetris`
directory, so installed plugins can load the bundle from disk.
