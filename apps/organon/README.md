# Organon graphical application

The graphical application is part of the [Organon framework](../../docs/organon.md),
alongside its declaration model and compiler. XML is one of its
representations, rather than the whole framework. This application currently
provides editable Element data, interpreted previews, compiled
example drawings, a preview window, and PNG output. A graphical editing UI and
file hot reload are not implemented yet.

The package is named `organon-app` and its executable is `organon`. The application
library's `organon_app::Element` owns editing data. Compiled XML elements and Rust interfaces share evaluation support in `organon` and expose native Theoria outputs. XML storage and compilation live in the sibling
library packages `crates/organon-format` and `crates/organon-compiler`.
Compiled examples use `organon::import!` to import XML directly in Rust modules,
without a build script. Paths are relative to this application's `Cargo.toml`.

From the repository root:

```powershell
cargo run -p organon-app
cargo run -p organon-app -- --source apps/organon/src/preview/badge.organon.xml --input width=360 --input fill=#49dcb1
cargo run -p organon-app -- --png target/organon-text.png
cargo test -p organon-app
```

The window requires Windows. PNG rendering requires a Vulkan device; other
platforms also need an explicit `--font path/to/font.ttf` for the text demo.
See the framework README for XML vocabulary, Let evaluation, and build integration.
