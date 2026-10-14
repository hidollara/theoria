# Organon graphical application

The graphical application is part of the [Organon framework](../../docs/organon.md),
alongside its declaration model and compiler. XML is one of its
representations, rather than the whole framework. This application currently
provides editable Element data, interpreted previews, compiled
example drawings, a preview window, and PNG output. A graphical editing UI and
file hot reload are not implemented yet.

The package is named `organon-app` and its executable is `organon`. The application
library's `organon_app::Element` exposes the current core authored tree for editing.
Compiled XML elements and interpreted previews share Rust implementations in
`organon-theoria`. Storage, import loading and checked IR live in `organon-core`.
The app has no direct dependency on the compiler and no drawing-specific schema.
Compiled examples use `organon_theoria::library!(crate::module)` to load the crate-root `organon.xml`,
without a build script. Paths are relative to this application's `Cargo.toml`.

From the repository root:

```powershell
cargo run -p organon-app
cargo run -p organon-app -- --source apps/organon/src/preview/badge.organon.xml --input width=360 --input fill=#49dcb1
cargo run -p organon-app -- --png target/organon-text.png
cargo test -p organon-app
```

`--source` resolves imports through the core package loader and selects the
sole public Element. If the file defines several Elements, use a file-level
`export` to select the preview entry. The default host includes the standard Rust implementations.
A custom host uses `organon_app::preview_library!("organon.xml")` to generate a registry
for its imported libraries. Adapter generation lives in `preview-macro/`, an app-owned
proc-macro crate; it uses normal compiler linking helpers and generated typed APIs.
The framework compiler and proc-macro crate have no preview-specific entry point.
The adapters and IR evaluator belong to the app. Providers
supply normal typed APIs without an app dependency or preview registration. The loaded
Element is an evaluation view, not a replacement for its authored import files.

The window requires Windows. PNG rendering requires a Vulkan device; other
platforms also need an explicit `--font path/to/font.ttf` for the text demo.
See the framework README for XML vocabulary, Let evaluation, and build integration.
