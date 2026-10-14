# Organon

Organon describes construction graphs independently of their execution backend.
The Theoria integration builds graphics and reactive values; handwritten Rust owns
component mounting, events, state and evaluation timing.

## Crate boundaries

| Package | Responsibility |
| --- | --- |
| `organon-core` | XML storage, declaration contracts, import resolution, semantic validation and construction IR. No Cargo or Theoria dependency. |
| `organon-cargo` | Cargo package discovery, dependency aliases, declaration identities, Rust library connections and source-change tracking. |
| `organon-theoria` | Theoria construction contracts, value sources, attribute interpretation and native Element implementations. |
| `organon-theoria-compiler` | Rust generation from checked declarations; its construction plan is private. |
| `organon-theoria-proc-macro` | Macro input syntax, diagnostics and Theoria implementation binding. |
| `organon-app` | Editing the authored tree, interpreting IR through native adapters, preview presentation and PNG output. |
| `organon-app-preview-macro` | App-owned adapter generation for its IR evaluator; located under `apps/organon`. |

`organon-cargo` implements core's `loader::Sources` contract with `CargoSources`.
It owns Cargo metadata lookup, package-root `organon.xml` discovery, dependency
aliases and within-package declaration identities. Its Rust connection helpers
export and consume the `organon!` provider macro, check the declared module
location and register files read during expansion as rustc inputs. These helpers
do not generate Theoria types or evaluation code.

Compilation and preview hosts use this same package adapter. Core contains no
Cargo feature or package discovery API. The app has no direct compiler dependency;
`library!` uses the compiler during macro expansion.

Core's shared vocabulary lives outside its authored format: `names` defines
identifiers and binding names, `types` defines input/output types and children
contracts, and `Error` is exported at the crate root. Neither `types` nor the
semantic IR depends on the format tree. XML codec helpers are private to `format`.
Its `definition`, `external`, `element`, `contract`, `export` and `import`
modules own the corresponding authored structures and their readers/writers.
File publication uses `Export`; Element and interface output mappings share `Provide`.

## Authored data and evaluation

`organon_core::format::DefinitionFile` is the authored tree. It preserves file
declarations, Element declarations and children in their original order. Attribute
values remain XML-decoded strings; attribute order is canonicalized on serialization.
Formatting comments and source whitespace are not a lossless editing format.

`organon_core::loader` resolves file imports, public names and aliases using a
host-supplied package resolver. Canonical files are read once per load and their
snapshots are retained. The flattened definition graph is an evaluation view;
it must not replace the individual authored files when saving.
Type names are resolved in their original file's scope before flattening. An
unimported private type cannot become visible by matching a name in another file.

`organon_core::semantic` resolves declarations, input/output contracts and
construction dependencies into an IR. Its signatures retain optional inputs and
child type/count constraints, so consumers do not need to recover those contracts
from the authored tree. Contracts come from `declare`, `interface` and `element`;
there is no separate programmatic constructor registry in core.
`DefinitionKind` preserves this distinction in the IR; whether a definition can
be constructed as a tag is derived from that kind, not used to identify interfaces.

Native attributes remain `Argument::Attribute { text, expected }` in this IR;
core does not treat their contents as parsed references or templates.
Native attribute parsers subsequently
interpret attribute text and report typed references. Their code runs during
construction or preview preparation, not inside the proc macro.
The `{name}` string-template grammar belongs to `organon-theoria::attribute`,
alongside its other attribute parsers, rather than to core syntax.

There is one XML schema and one semantic frontend. The compiler has no implicit
Rect/Text/Color syntax and no fallback for files without native value declarations.
Standard elements are imported from `organon-theoria` like any external library.

## Runtime modules and public entry points

| Implementation module | Responsibility | Public entry point |
| --- | --- | --- |
| `source` | Calculation and Fixed/Reactive/Inline storage | `Calculation`, `Source`, `Read`, `ReadContext` |
| `contract` | Rust declaration, output and interface contracts | Root traits: `Type`, `Declaration`, `Output`, `Evaluate`, `Provide` |
| `attribute` | String interpretation, typed dependencies and opaque reference values | `ParseContext`, parser helpers, `Registry` and `Value` |
| `elements` | Individual standard Element implementations | Generated public types, `types`, `attribute_types()` |

The source modules `source`, `contract`, and `elements` are private.
Generated cross-crate contracts remain public but doc-hidden. Application and
library authors do not need to reference `__organon` directly.

`organon-theoria` does not interpret IR or retain output dictionaries. Its
attribute registry contains type contracts used by ordinary generated code;
it has no entry point for constructing an Element by name. The app owns
`evaluation::Registry`, native construction adapters and inspectable `Object`
outputs. Its `Value` wraps an attribute value with optional output information.

## XML construction

Every file has an `organon` root. Elements receive named inputs, construct local
values and publish selected bindings. There is no implicit default output.

```xml
<organon version="1">
  <import from="organon-theoria"/>
  <element name="Panel">
    <input name="width" type="f32"/>
    <input name="fill" type="Color"/>
    <let name="shape">
      <Rect x="0" y="0" width="{width}" height="48"/>
    </let>
    <bind path="outline">
      <ref name="shape"/>
    </bind>
    <bind drawing="drawing">
      <Draw source="{fill}">
        <Fill>
          <ref name="shape"/>
        </Fill>
      </Draw>
    </bind>
    <provide name="outline" ref="outline"/>
    <provide name="drawing" ref="drawing"/>
  </element>
  <export name="Panel"/>
</organon>
```

`input`, `let` and `bind` introduce names in one Element-local environment.
`ref` refers to a whole named value. `bind` extracts named outputs into local
bindings and is an Element-level declaration, not an expression wrapper.
`provide` supplies an output: `name` is the output name and `ref` is the local
binding supplying its value. The same syntax is used inside interface `impl` blocks.
Nested `get` and dotted reference selection are not supported.

Generic element tags are library constructions. Their receiving declarations
define inputs and children. The standard declarations in
[`organon.xml`](../crates/organon-theoria/organon.xml) are the authoritative port
and interface definitions. `Fill` accepts Outline children, `Draw` combines
ColorSource and CoverageSource, and `Text` accepts TextContent children.
`s`, `span`, and `br` implement TextContent through ordinary declarations.

The receiving type decides attribute interpretation. Without a custom parser,
values accept bare reference names. The standard numeric parser accepts a literal
or a whole `{reference}`; String interprets a format string with typed Display
references and doubled braces for literal braces. XML character references remain
available for explicit newlines, tabs and quotes in attribute values.

## Imports and publication

`<import from="./parts.xml"/>` brings in all public names from a relative file.
A bare source name denotes
a Cargo dependency alias, whose entry is `organon.xml` beside its Cargo.toml.
`<use from="./parts.xml" name="Panel" as="Card"/>` selects one public definition;
`from` and `name` are required, and `as` optionally gives it a local name.
Both tags are empty file-level declarations. `import` accepts only `from`, and
neither tag republishes names implicitly. They may be combined: identical names
for the same original definition are merged, while different definitions under
the same name are rejected. The owning
package may also import its own entry point without a self dependency.

File-level `<export name="Panel" as="Card"/>` publishes a local or imported definition.
Only explicitly published names are visible to importing files and generated public
Rust APIs; without `export`, no definitions are public. Publication may precede its
definition or import. Separate files may have identically named private helpers.
Element-level `provide` exposes a binding as an output and does not publish the Element itself.
Importing a library reuses its Rust types and implementation identities instead
of constructing unrelated copies of them.

## Preview and editing

`organon_app::Element` currently exposes the core authored Element structure.
The app does not have a separate enum for Canvas, Path or primitive drawing tags.
Edits preserve the surrounding file imports and declaration order via
`DefinitionFile`; GUI selection and undo state can be layered on it separately.

`organon_cargo::load(path)` supplies the loaded graph and checked IR.
The preview maps declaration identities to a `Registry` supplied by its host,
then calls `Registry::evaluate`. Rust implementations must already be compiled
into that host; XML does not dynamically load arbitrary Rust code.

The preview app supports a native window and PNG output. There is no graphical
property editor, file watcher or automatic reload yet. Re-evaluation uses new
sample inputs and the current checked definition. See the
[app README](../apps/organon/README.md) for commands.

## Validation

```sh
cargo test -p organon-core -p organon-theoria -p organon-theoria-compiler -p organon-theoria-proc-macro -p organon-app -j 1
cargo check -p organon-core --no-default-features
```

Tests cover ordered editing, import visibility and aliases, generated-library
linkage, dynamic dependencies, fixed and inline evaluation, attribute contracts,
and equality of compiled and interpreted geometry. Obsolete graphics-frontend
tests were removed; the runtime and current library integration tests remain.

## Cargo integration

Each crate has one Organon entry point: `organon.xml` beside `Cargo.toml`.
`library!(crate::graphics)` loads that graph into its containing Rust module.
The argument declares the actual placement; it does not redirect generated items.
The macro checks that it matches `module_path!()` and rejects mismatches.
XML imports manage file splitting and dependencies; `export` chooses public names.

```rust,ignore
pub mod graphics {
    organon_theoria::library!(crate::graphics);

    // Ordinary Rust items can share the module.
    pub struct RenderOptions;
}
```

At crate root, use `library!(crate)`. The placement must be publicly accessible
when another crate uses this library. Nested modules and split Rust source files
are supported. No manifest placement setting or root module re-export is needed.

Rust callers use ordinary paths, such as `use widgets::graphics::Panel;`.
`library!` also publishes `types` for declaration types and `attribute_types()`
for attribute interpretation in the same module. `types` follows the root XML `export` declarations,
including aliases, without exposing private declarations. No handwritten
re-export of generated internal modules is needed.
For XML imports, generated code uses a single `organon!` connector exported in
the crate-root macro namespace to access contracts and dependencies without
knowing the provider's Rust module location. This connector has `#[doc(hidden)]`
and only accepts the internal `@link` protocol; it is not a supported API for
handwritten Rust. Cross-crate access requires it to remain exported, so hiding
it from documentation does not prevent explicit calls. A second `library!`
invocation conflicts with the same macro name and is rejected.

The explicit path supplies information not available to stable proc macros:
the containing module path as Rust syntax. It can remain supported if automatic
placement discovery becomes available in the future.

`#[implement] impl Rect` implements the named declaration reachable from the root;
neither a file argument nor a name override is accepted. The declaration may live
in an imported local file, and Rust implementations may live in other modules.

The same entry point supports XML Elements and Rust-backed declarations.
Native value declarations expose their Rust value types; constructible declarations
expose their builders. Imported XML Elements reuse the provider's Rust type, so
Organon imports and Rust `use` do not produce distinct copies of that type.
Private definitions remain available to the graph without being re-exported into
the enclosing Rust module. The hidden connection module is implementation plumbing,
not a Rust privacy boundary.

Generated native-library code uses the runtime facade for reactive/component
support; consumers need not add those dependencies merely to expand the macro.
Host Rust code can depend on Theoria crates for the APIs it uses directly.
XML-only edits trigger recompilation, and invalid definitions produce compiler
errors at the macro invocation. Tests use the same `library!` and `implement`
entry points as consumers; there is no separate file-import macro.

Tooling can generate a package library with
`organon_theoria_compiler::library::compile(entry, "crate::graphics")`.
The compiler consumes the shared core declarations and lowers them to a private
Rust construction plan. It does not own another XML schema or primitive parser.

## Native libraries: standard and external

`organon-theoria/organon.xml` declares the standard library, and
`organon-theoria/src/elements/` implements it, grouped into value, color, geometry,
drawing, and text modules. `elements.rs` groups these individual Element
implementations. External providers use the same
declarations and Rust API. Consumers import a provider through XML and depend on
its Cargo package; they do not repeat the implementation registration.

```xml
<organon version="1">
  <use from="organon-theoria" name="f32"/>
  <declare name="Choice">
    <input name="choose" type="f32"/>
    <input name="left" type="f32"/>
    <input name="right" type="f32"/>
    <output name="selected" type="f32"/>
    <output name="alias" type="f32"/>
    <output name="forwarded" type="f32"/>
  </declare>
  <export name="Choice"/>
</organon>
```

```rust,ignore
organon_theoria::library!(crate);

#[organon_theoria::implement]
impl Choice {
    #[output(name = "selected")]
    #[output(name = "alias")]
    fn select(input: Input<'_, Self>) -> f32 {
        if *input.choose() > 0.0 {
            *input.left()
        } else {
            *input.right()
        }
    }

    forward!(left => forwarded);
}
```

`implement` belongs on an implementation block. Its target identifies the XML
declaration; attribute arguments and name overrides are not accepted. The macro supplies
the implementation for the generated marker, so no handwritten marker struct is
required. `Input<'_, Self>` is macro syntax for a generated evaluation view, not
an ordinary Rust type that must be imported. Calling an accessor reads and tracks
that input at that point. A branch that does not call `input.right()` does not
depend on `right`. Accessors return `Read<T>`; `into_cloned()` moves an inline
result or clones a borrowed result. Optional inputs return `Read<Option<T>>`.

`input.children()` reads the ordered child collection. `input.read(&source)`
reads a nested interface output in the same evaluation. Functions may explicitly
accept `cx: &ReadContext<'_, '_>` when passing a context to lower-level helpers.
There is no input-handle accessor in this evaluation view. Direct forwarding is
expressed by `forward!`, which returns the existing source without another computation.

Multiple `output` annotations may select one function. They share code, not
cached results; each output retains its own dependency tracking and XML-declared
inline policy. `Calculation::new` stores a computation; binding evaluates once
to classify actual reads. A fixed branch stays fixed even if an unread input is
reactive. Reactive branches can change dependencies during later evaluations.

The native marker is empty. `Builder<E>` stores partial input connections,
`Input<E>` in generated linkage code stores their sources, and `Built<E>` stores
the connected outputs. Their storage is expressed through associated tuple types,
without public `ChoiceInputs` or `ChoiceOutputs` helper structs. Native builders
select their registered implementation automatically:

```rust,ignore
let choice = crate::Choice::builder()
    .choose(1.0)
    .left(12.0)
    .right(state.value())
    .build(&reactive)?;
assert_eq!(choice.selected_fixed(), Some(&12.0));
let value: organon_theoria::Source<f32> = choice.selected();
```

### Values and parsing

`tag="false"` declares a value that is not constructed as an XML tag. Its Rust
implementation explicitly specifies `type Value`; it can be a scalar or a struct
holding several values. There is no separate Inner wrapper. A tag-constructed
declaration instead has the generated `Built<E>` as its `Type::Value`, so parsing
a reference and constructing its tag refer to the same kind of value. Replacing
that generated representation with a custom tag constructor is not currently an
extension point.

```xml
<declare name="Pair" tag="false"/>
```

```rust,ignore
#[organon_theoria::implement]
impl Pair {
    type Value = (f32, String);

    #[parse]
    fn parse(
        text: &str,
        cx: &mut organon_theoria::attribute::ParseContext<'_>,
    ) -> Result<organon_theoria::Source<Self::Value>, String> {
        let (number, label) = text.split_once(':').ok_or("expected number:label")?;
        let number = cx.reference::<organon_theoria::types::f32>(number)?;
        let label = cx.reference::<organon_theoria::types::String>(label)?;
        Ok(organon_theoria::Calculation::new(move |cx| {
            (number.read(cx).into_cloned(), label.read(cx).into_cloned())
        }).into_inline())
    }
}
```

This parser's syntax is its own choice. It requests Organon marker types, which
determine the Rust values returned by the references. Different references can
have different types. The parser does not read their values. `ParseContext`
records names and checks their declared types or implemented interfaces. The
graph is ordered and checked for cycles after interpretation; later declarations
remain usable as forward references.

The outer Result reports interpretation errors, including invalid literals and
unknown or incompatible references. A validated reference yields `Source<T>`;
reads do not introduce another Result. The Source can be Fixed, Reactive or Inline.
A Result belongs inside the Source only when it is part of the value's contract.
Fallible outputs remain Result values; they cannot silently satisfy an infallible
reference. A custom parser can explicitly request one with
`cx.reference_fallible::<T>(name)`, returning `Source<Result<T::Value, &'static str>>`.
The outer Result still reports reference-contract errors; the inner Result is the
computation's value and may recover after a reactive update. Reference-only type
parsers use this route when the receiving input is fallible. Custom parsers return
their declared value, which is lifted into Ok for a fallible receiving input. Dependency ordering guarantees initialization before evaluation.
Violating that ordering or a declared native type is an implementation invariant
violation, not a user-facing parse failure.

There is no additional Parsed/Plan execution layer. `Attribute<T>` contains
`Source<T>` and the collected dependency names. Fixed literals use `Source::from(value)`.

`attribute::reference_or_literal::<Self>(text, cx, parse_literal)` accepts a
whole `{name}` typed reference or passes a literal unchanged to its parser.
It never formats references through Display or reparses formatted strings.

String attributes, including `s.t`, use `attribute::template(text, cx)` instead.
They always interpret format strings: `{name}` requires the standard `Display`
interface (`text: String`), and `{{` / `}}` escape braces. Formatting capability
is checked before evaluation; interface outputs are read in the active evaluation
context so their dependencies are tracked. ParseContext itself only collects
and validates typed references; it does not define either syntax.

Parsers are registered only by `#[parse]`, independently of the method name.
A method named `parse` without that attribute is an ordinary helper. Each implementation
allows one parser; a parser cannot also carry `#[output]`. There is no custom
`coerce` hook: references request a contract, and generated type/interface resolution
extracts the value after checking that contract. Typed reference extraction errors
are not retried as string conversions.

Without a `#[parse]` method, a type or interface accepts a bare reference name.
The definition owner may override this once in its implementation block.
`ColorSource`, for example, accepts a bare reference or a color literal.
There is no per-import parser override and no XML parser selector. These checks
run during Rust construction (or preview preparation), not during proc-macro
execution: the compiler cannot execute arbitrary consuming-crate Rust code.

### Interface outputs and preview

A declaration may contain `<implements interface="Numeric"/>` without declaring
an Element-specific output. Rust implements the interface directly:

```rust,ignore
#[organon_theoria::implement]
impl Twice {
    #[output(interface = "Numeric", name = "value")]
    fn twice(input: Input<'_, Self>) -> f32 {
        *input.value() * 2.0
    }
}
```

Interface output names have their own namespace. Explicit XML `impl` / `provide`
mappings remain the way to share an existing Element output under an interface.
`Rect` and `Polygon` implement Outline, Fill consumes Outline, and Draw consumes
CoverageSource and ColorSource. `s`, `span` and `br` implement TextContent; their
style output is a StyleOverride and their content preserves nested text structure.

The preview host explicitly calls `organon_app::preview_library!("organon.xml")`
in a module to generate its `registry()`. This emits adapters in the host using
the providers' existing typed construction and output APIs. Providers need no
app dependency or preview registration; `library!` emits no preview adapters.
The app's `organon-app-preview-macro` generates these adapters at build time.
It uses the compiler's ordinary provider-path helpers and the generated public
`Type`, builder, output and `Provide` contracts. The framework compiler and
proc-macro crate contain no preview entry point or adapter generator.

Preview retains native values alongside output projections and uses the same
parsers and Rust computations as compiled code. XML-defined interface outputs
are converted into native interface values by app-owned adapters. The attribute
runtime only sees their ordinary interface provider, never an output dictionary.
Providers must be compiled into the host; XML cannot load arbitrary Rust code.
The app edits the core authored tree and interprets its checked IR.

### Output ownership

`Source<T>` is the output of Organon construction, including named outputs and interface
projections. `Calculation::bind` classifies actual dependencies and returns a Source:
fixed data, an ordinary Computed, or an inline expression. It does not create a
separate external adapter.

Cloning a Source shares its fixed data, existing Reactive handle, or inline closure.
Sharing an inline closure does not cache its evaluation result. Rust code can read
Sources in its own Theoria computation using `ReadContext::new(cx)`, and explicitly
share that computation's Value when caching is wanted. Organon does not guarantee
identity across independently constructed Rust adapters.
