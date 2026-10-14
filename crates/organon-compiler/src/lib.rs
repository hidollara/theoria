//! Compiles Organon definitions into typed Rust builders.
//!
//! This crate does not drive an application or evaluate reactive values.
//! Generated drawing elements depend directly on theoria-canvas, theoria-text,
//! theoria-reactive, and theoria-component. XML elements and Rust interfaces
//! share initial evaluation and value storage through organon.
//!
//! Applications use `organon::import!`, whose implementation calls this compiler during macro
//! expansion without a build script:
//! ```ignore
//! mod panel {
//!     organon::import!("src/panel.organon.xml");
//! }
//! ```
//!
//! Paths are relative to the consuming package's Cargo.toml; the enclosing Rust
//! module chooses the namespace and visibility. Tooling can also call
//! `compile_xml`, `generate_element_rust`, or the file-to-file `compile` API.
//! `<element name="Badge">`
//! generates `Badge::builder()` and consuming input setters. `build(&Reactive)`
//! validates required inputs, evaluates outputs, and returns a persistent element. Each exported Let
//! has `foo() -> reactive::Value<T>` and `foo_fixed() -> Option<&T>` accessors.
//! Fallible primitive expressions and their dependents use Result as T; their
//! evaluation errors do not prevent construction or use of unrelated outputs.
//!
//! Fixedness and fallibility are independent. Private dynamic Lets use Computed,
//! while inline expressions are expanded into their consumers. Public dynamic
//! inline outputs receive a native Computed adapter. Fixed outputs retain both
//! their direct value and an immutable native handle.
//!
//! `<organon version="1">` files can also declare interfaces implemented through
//! `organon::Implementation<I>`. These generate tuple-backed `Input<I>` accessors,
//! named output computations, and Builders accepting a Rust implementation.
//! Their shared evaluation support lives in `organon`. Initial evaluation records
//! actual reads to distinguish fixed outputs from dynamic ones, including branches.
//!
//! `build_in(&mut MountContext)` constructs within an existing Component.
//! `builder.component(callback)` defines a Component that publishes its build
//! result through the callback when mounted, without owning a ComponentInstance.
//!
//! Version 1 supports `Draw` with separate Coverage (`All`/`Fill`) and Path
//! (`Polygon`/`Rect`) expressions, `Translate`, `SourceOver`, `ref`, and horizontal
//! `Text` with style attributes and inline `s`, `span`, and `br` content.
//! Only `s.t` supplies text; span and string styles inherit field by field.
//! `Input` declarations accept `f32`, `Color`, `ColorField`, `Canvas`, `String`,
//! `Fonts` (a host font chain), `Path`, and loaded Element/interface names;
//! attributes refer to them with `{name}`; child expressions use `<ref name="name"/>`.
//! Textual attributes support in-place interpolation; literal braces use `{{`
//! and `}}`. Composition requires a fixed explicit space. Unknown vocabulary is
//! rejected, not ignored.
//! Text syntax is provisional; measurement references are not yet defined.
//! File-relative imports are resolved once per entry point. Named element
//! instances are values that can be passed or exported. `ref` selects one binding;
//! `get` selects one output. Attribute templates support `{instance.child.output}`.
//! Rust implementations are supplied by the import macro's `implementations`
//! argument. The graphical editor and component preview registry are not implemented.

mod api;
mod bind;
mod element;
mod generate;
mod imports;
mod instance;
mod interface;
mod ir;
mod lower;
mod references;
mod selection;

pub use element::generate_element_rust;
pub use imports::{LoadedDefinitions, load_definitions};
pub use interface::{generate_file_rust, generate_file_with_implementations};

use ir::{CanvasExpr, Color, Space, Value};

use std::{error::Error as StdError, fmt, fs, path::Path};

/// A compilation error. File compilation errors include the relevant path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Error(pub(crate) String);

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl StdError for Error {}

/// Compiles XML source without accessing the filesystem.
pub fn compile_xml(source: &str) -> Result<String, Error> {
    let file =
        organon_format::DefinitionFile::from_xml(source).map_err(|e| Error(e.to_string()))?;
    generate_loaded_file(&file)
}

/// Generate a resolved file, preserving the single-element convenience API.
pub fn generate_loaded_file(file: &organon_format::DefinitionFile) -> Result<String, Error> {
    if !file.imports.is_empty() {
        return Err(Error(
            "unresolved imports: use load_definitions with an entry file path".into(),
        ));
    }
    // Keep the original single-Element generated API stable.
    if let [organon_format::Definition::Element(definition)] = file.definitions.as_slice() {
        let definition = selection::normalize(definition)?;
        if !definition
            .inputs
            .iter()
            .any(|i| matches!(i.kind, organon_format::InputType::Element(_)))
            && !definition
                .definitions
                .iter()
                .any(|d| matches!(d.value, organon_format::ValueExpr::Instance(_)))
        {
            return element::generate(&definition, &std::collections::HashMap::new())
                .map(|(code, _)| code);
        }
    }
    generate_file_rust(file)
}

/// Compiles an XML definition into a Rust module at `output` and emits Cargo's
/// input-change directive. Call from `build.rs` with an output inside OUT_DIR.
/// The caller owns the module name and includes the generated file explicitly.
pub fn compile(input: impl AsRef<Path>, output: impl AsRef<Path>) -> Result<(), Error> {
    let input = input.as_ref();
    let output = output.as_ref();
    let loaded = load_definitions(input)?;
    let code = generate_loaded_file(&loaded.file)?;
    for path in &loaded.paths {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    fs::write(output, code).map_err(|error| Error(format!("{}: {error}", output.display())))
}
