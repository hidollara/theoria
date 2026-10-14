# Organon

Organon is a graphics framework built on Theoria, intended to combine graphical
construction, declarative composition, and optimization while remaining usable
alongside handwritten Rust. Its graphical application, declaration model, and
compiler are parts of the same framework. XML is one representation and storage
format, not the definition of Organon itself. The initial implementation can
build a Canvas, expose a Path alongside it, or expose a Path without drawing.
Definitions can be compiled to ordinary Rust builders or evaluated dynamically
with sample inputs for preview. Rust owns Component mounting, events, state,
and evaluation timing.

The `organon` crate provides the import API and shared evaluation support for XML elements and Rust
interface implementations, without GUI dependencies. Its GUI application lives in `apps/organon`,
with a library for editing data and previews and a binary for the preview window/CLI.
The Cargo package is named `organon-app`; its executable is named `organon`.
All packages belong to the Theoria workspace; there is no separate Organon
workspace or lockfile. Work from the repository root.

| Package | Responsibility |
| --- | --- |
| `organon` | `organon::import!`, interface implementation contracts, and tracked input/output evaluation. |
| `organon-format` | The accepted XML format, data types, serialization, and format validation. |
| `organon-compiler` | Validating bindings and generating Rust element APIs and calculations. |
| `organon-proc-macro` | Importing XML as Rust items at compile time and tracking source changes. |
| `organon-app` | Editable definitions, interpreted evaluation, and the initial preview application. |

The supporting crates live directly under `crates`. The workspace lists
`apps/organon`; Cargo discovers the libraries through its path dependencies.
Generated elements and Rust-defined interfaces share construction and evaluation
support in `organon`, with native Theoria output handles. They do not need the
compiler, format, or application at runtime.
The application separately owns its preview evaluation types.

The application and compiler both depend on `organon-format`. The compiler
does not depend on the application or its editable `Element`. It lowers format
data into its own private representation before generating Rust. The application uses
the compiler through `organon::import!` to compile its preview drawings.

## Vocabulary

**Element** is the editing unit with inputs, local definitions, and named
outputs. It may expose a Canvas, a Path, or both; drawing is not mandatory.
An Element currently describes values, not a mounted Theoria Component or an
instance with event handlers and state.

| Concept | Application type | Format type | Meaning |
| --- | --- | --- | --- |
| Element | `organon_app::Element` | `organon_format::ElementDefinition` | Inputs and named expressions, with explicit exports; the `<element>` entry form. |
| Canvas expression | `organon_app::CanvasExpr` | `organon_format::CanvasExpr` | An authored expression such as Draw, Text, or SourceOver that produces a Canvas. |
| Binding | `organon_app::element::Let` | `organon_format::Let` | A name, an expression, and whether to export its value. |
| Value expression | `organon_app::element::ValueExpr` | `organon_format::ValueExpr` | A Path, Canvas, or reference; not an evaluated value. |

`Input`/`InputType` declare host-supplied values. These format types, along
with `CoverageExpr` and `PathExpr`, are also used inside editable Elements.

**Canvas and Path values** are Theoria's native runtime types. A CanvasExpr
produces a Canvas; it is not a renderer's CanvasNode. The preview's
`evaluation::Value` wraps resolved native values, and `Values` maps their names.
The compiler instead generates a named element, its Builder, and typed output methods. Its private
`DrawingIr` and typed expressions are implementation details.

The application and format representations remain separate even when names
match: the application owns editing, while the format describes accepted XML.
Conversions preserve authored data. Rust type names use `Definition` to make
the stored definitions distinct from their evaluated values. XML uses
`<element>` as its only root, including for drawing-only definitions.

XML-decoded attribute strings remain unexpanded and retain their authored text.
The format validates XML structure, serializability, brace syntax, fixed
attributes, and the single-reference syntax of resource attributes. The binding view resolves names and rejects cycles. The compiler
checks reference types and literal values; generated builders interpret
expanded values at runtime. Valid XML is not necessarily valid Organon XML.
Further attribute-range policies can be added to the format. This responsibility
is independent of editable Element state.

This is an early implementation, not yet a graphical editor. Text syntax below
is provisional. References to text measurements from other Elements remain an
open design question.

## XML vocabulary and Theoria

XML names follow the meaning of Theoria operations, without reproducing every
Rust wrapper type or constructor. A tag may express a specific operation or a
convenience that lowers to several Theoria calls. For example, `SourceOver`
constructs a Canvas composition without requiring a `Composite` or `Composition`
wrapper in XML. Names distinguish what an expression produces: `Fill` produces
Coverage, while `Draw` produces Canvas from a source and Coverage.

Language constructs use lowercase (`element`, `input`, `let`, `ref`); value
constructors and operations use PascalCase. Attributes use snake_case. Where XML directly names a
native type or enum value, it uses the corresponding Rust spelling. `input.type` accepts `f32`, `String`, `Color`,
`ColorField`, `Canvas`, `Path`, and `Fonts`. `Fonts` is Organon's shorthand for
an ordered `Vec<Font>`; it is not a separate Theoria type. `element`, `input`,
`let`, and `ref` describe Organon definitions and their interface.

The drawing vocabulary follows Theoria's separation of source and coverage:

```xml
<Draw source="{brush}">
  <Fill><ref name="outline"/></Fill>
</Draw>
```

This constructs `Canvas::draw(brush, Coverage::fill(outline))`. `Draw.source`
accepts a native `ColorField`, `Color`, or `Canvas` reference, or color text.
Rust-supplied gradients, images, and Canvas fields remain native values; Organon
does not convert them to a single color. Constructing gradients or images in XML
is not implemented yet.

| XML expression | Produces / corresponds to |
| --- | --- |
| `Draw` | Canvas from a source ColorField and one Coverage child. |
| `Fill` | Coverage from a Path, using the nonzero fill rule. |
| `All` | Unbounded Coverage, from `Coverage::all()`. |
| `Polygon` | Path from at least three Point children, closed by `Path::polygon`. |
| `Rect` | Path convenience: a four-point polygon from x, y, width, height. |
| `SourceOver space="LinearSrgb"` | Canvas composition; backdrop child first, source child second. |
| `Translate` | Canvas transformed by `Affine2::from_translation`. |
| `ref name="name"` | A named input or Let value; its type must fit the enclosing expression. |
| `let name="name"` | Declares a named value directly under Element without changing its type. |

`Fill` takes exactly one Path expression as its child: `Polygon`, `Rect`, or `ref`.
It does not take a color or produce a Canvas. `Rect` likewise does not draw:

```xml
<Draw source="#182435">
  <Fill><Rect x="0" y="0" width="{width}" height="176"/></Fill>
</Draw>
```

`Rect` and `Translate` are explicit conveniences over Theoria operations, not
additional primitives in Theoria. `SourceOver` names the composition operation;
its fixed `space` accepts `Srgb`, `LinearSrgb`, `DisplayP3`, or `LinearDisplayP3`.
Other composition operations, Coverage forms, and transforms can be added at
their respective boundaries.

The old drawing-level `Fill`/`Rect`, kebab-case attributes,
lowercase type names, and hyphenated color-space names are not accepted aliases.
The repository's previews and fixtures use the new vocabulary.

## Named expressions and exports (provisional)

`<element name="Example" version="1">` names the generated Rust element type and contains input and let declarations. The name is independent of its file path and host Rust module. File-relative imports collect definitions into one namespace per Rust import invocation; same-named definitions from different files are rejected.

```xml
<element name="Example" version="1">
  <input name="width" type="f32"/>
  <input name="fill" type="Color"/>
  <let name="outline" export="true">
    <Rect x="0" y="0" width="{width}" height="40"/>
  </let>
  <let name="drawing" export="true">
    <Draw source="{fill}">
      <Fill><ref name="outline"/></Fill>
    </Draw>
  </let>
</element>
```

Let wraps exactly one child value expression. To alias an existing value, use
`<let name="alias"><ref name="other"/></let>`.
`let.value` and `Fill.path` attributes are not accepted; these arguments always
use child expressions, whether constructed locally or supplied by reference.
Aliases retain the referenced type, including scalar, string, and native input
resources. Constructed expressions currently produce Path or Canvas.

`ref` uses the same name resolution as `{name}` in attributes. It preserves the
referenced type: a Canvas position requires Canvas, and a Path position requires
Path. It does not convert values or select an implicit output. Unknown names,
cycles, and type mismatches are rejected during dependency/type validation.

All Input and Let declarations live directly under Element, with unique names.
Let cannot appear inside another Let or inside an expression. Anonymous
expressions may be nested where needed; to reuse or export a partial result,
extract it into a top-level Let and reference it. All named values and exports
can therefore be read together at the top level. Point, TextStyle, and Coverage
bindings are not implemented in this first step.

`export` is a fixed `true` or `false`, defaulting to false. Only explicitly
exported bindings appear in the public outputs; Element has no default output.
An Element requires at least one export. Each Let may be exported independently.

References may point forward or backward in the file. Each binding is built
in dependency order. The snapshot evaluator evaluates each binding once; aliases reuse that value. Persistent `bind` follows the rules below.
Unknown names, duplicate names, cycles, and values of the wrong type for their
use are errors.

### Fixed and reactive Let values

`inline` is a fixed boolean, defaulting to false, independent of `export`.
Let expressions describe repeatable value computations; evaluation count and
timing are not side effects of the declaration.

* Each ordinary Let and exported inline Let is evaluated during construction.
  Actual reads determine fixedness: no reactive reads means a fixed value,
  including a fixed `Err`. An unread reactive input does not prevent folding.
* A dynamic ordinary Let retains that initial result in a Theoria Computed.
  Later invalidations are evaluated lazily. The first output read reuses the
  construction result; Organon has no separate cache mechanism.
* `inline="true"` does not introduce a Computed for that Let. Generated Rust
  incorporates the expression into its consumer wherever possible. Its reads
  belong to the consumer's evaluation context. Shared local subexpressions may
  be combined; evaluation count is not guaranteed.
* Internal consumers expand inline expressions directly. At the public Rust
  boundary, an exported dynamic inline value receives a stable native Computed
  handle. Repeated output accessor calls reuse that handle; internal consumers
  do not start depending on this public adapter.

```xml
<let name="outline" inline="true" export="true">
  <Rect x="0" y="0" width="{width}" height="40"/>
</let>
```

XML-generated calculations and Rust interface calculations use the same
`organon::Source`, `ReadContext`, and `Bound` implementation. A Source holds a
fixed value, native Reactive handle, or Rc-backed Inline calculation. A temporary
read borrows fixed values, holds Theoria's reactive read guard, or owns the inline
result. Shared evaluation tracks reads, classifies the initial result, and retains
native output handles. XML's internal inline expressions are still expanded into
consumers without allocating callable values.

The interpreted preview has its own `organon_app::evaluation::Source<T>` and
`SourceRead`. `Source::as_fixed()` returns `Option<&T>`, while `read(cx)` observes
a fixed, reactive, or inline value. Reading does not require Clone. The read's
`into_cloned()` moves an owned inline result and clones borrowed results;
`cloned()` always clones. These extraction methods require `T: Clone`.
Errors remain ordinary Result values. The source ownership and evaluation tests
live with the application.

Generated elements expose Theoria types rather than the evaluator's Value:

```rust
let width = reactive.create_state(360.0_f32);
let badge = badge::Badge::builder()
    .fill(fill)
    .width(width.value())
    .build(&reactive)?;
let drawing = badge.drawing(); // reactive::Value<Result<Canvas, &'static str>>
let reader = reactive.create_computed(move |cx| cx.read(&drawing).cloned());
```

Setters consume the Builder and accept a fixed `T`, native `reactive::Value<T>`,
or shared `organon::Source<T>`. String inputs also accept `&str`. Inputs may be
supplied in any order, replaced before construction, or partially configured in
a cloned Builder. For XML elements, `Input<T>` is an alias for
`organon::Source<T>`. A Builder's type is `Builder<Badge>`.

`build` checks required inputs before creating the graph. Each output has two
accessors: `foo()` always returns a native `reactive::Value<T>`, and
`foo_fixed()` returns `Option<&T>`. Fixedness is decided per output from reads
made during initial evaluation, including dependencies reached through inline
expressions. A fixed output is exposed as an immutable native handle; a dynamic
output reuses its initially evaluated Computed. All connected dynamic inputs and
readers must belong to the same Reactive. Construction returns
`organon::BuildError` for missing inputs or evaluation-entry conflicts (such as
building while the same Reactive is being mutated).

Fallibility is independent of fixedness. The compiler propagates primitive
return types through dependent expressions. Scalar aliases and infallible
Canvas expressions expose `T`; Text layout, checked paths/transforms, parsing,
and expressions depending on their results expose `Result<T, &'static str>`.
The error type remains provisional. An evaluation error is an output value,
not a missing-input construction error. `Some(Err(...))` is a fixed error;
`None` means the output is dynamic. An unrelated output remains usable, and a
dynamic error can recover after an input update. No implicit panic or last-good
value policy is applied. Explicit error-handling XML vocabulary is future work.

For a Component-owned graph, call `builder.build_in(&mut mount_context)`.
Alternatively, `builder.component(|result| { /* publish outputs */ })` produces
an implementation of Theoria's Component trait that builds inside its mount
context. The callback receives the construction Result because Component::mount
has no return value. Neither path creates a separate ComponentInstance.

```rust
context.mount(badge::Badge::builder()
    .width(width_value)
    .fill(fill)
    .component(|result| {
        let badge = result.expect("required inputs supplied");
        exported_drawing = Some(badge.drawing());
    }));
```

Sources, Exports, and the value graph are implementation details. The existing
snapshot-generation helpers are test-only and retained for regression
comparison. The graphical application's interpreted `Element::bind` has its
own diagnostic-value representation; fixed expression errors are likewise
retained per output. The snapshot `evaluate` and CLI report errors when they
extract concrete values. File hot reload remains future work.

The format retains authored declaration order for editing and serialization, and exposes
`ordered_bindings()` as a derived view of named expressions and their
dependencies. The app evaluates that view; the compiler checks types and lowers
it to Rust. Neither depends on the other's representation. A drawing-only
Element also exposes its Canvas through an explicitly exported Let.

Rect and Polygon still produce a Path, not a record with path/canvas fields.
Primitive fields such as `{rect.path}` cannot be selected. Named Elements may
export multiple outputs; selecting one is always explicit.

Fixed native outputs can be used directly for Rust-owned interaction:

```rust
let badge = badge::Badge::builder().width(360.0).fill(fill).build(&reactive)?;
let outline = badge.outline_fixed().expect("fixed inputs").as_ref()?;
let hit_region = theoria_interaction::interaction::HitRegion::fill(
    outline.clone(),
    theoria_canvas::path::FillRule::NonZero,
);
```

`organon_app::preview::badge` is a compiled example. Its concave outline is used by
both the drawing and an integration test using Theoria's HitRegion. Outputs are
in authored coordinates: a transform inside a Canvas output does not secretly
transform a separately exported Path. The host controls their placement.

An input may also have `type="Path"`, so Rust can supply a native Path to a
declaration. A definition with only a path output needs no Canvas. This proves
the input/output boundary without introducing a Rust plugin system.

`organon_format::ElementDefinition` describes this schema; `organon_app::Element` owns its
editable bindings, whose Canvas branches contain the app's `CanvasExpr` tree.
`Element::evaluate(&Values)` returns named native values without code generation,
filesystem access, or a GPU. Missing inputs, mismatched types, unresolved paths,
and nonfinite coordinates are errors. Supplied native colors retain their color
space. The compiler remains independent of the app's evaluator and editing data.

### Trying sample inputs

```powershell
cargo run -p organon-app -- --source apps/organon/src/preview/badge.organon.xml --input width=360 --input fill=#49dcb1
cargo run -p organon-app -- --source apps/organon/src/preview/badge.organon.xml --input width=200 --input fill=#fa9260 --png target/badge.png
```

`--source` reads an Element definition at application startup. `--input` supplies
scalar, string, or color samples; `--output` selects a Canvas output (default:
`drawing`). Font inputs receive the host's font chain. Native ColorField, Canvas, and Path
inputs can be supplied through the Rust `Values` API. Sample values are kept
outside the authored definition. After building the application, the executable
can preview arbitrary supported files without recompiling them.

There is no property editor, file watcher, saved preview-case format, or automatic
reload yet. The default scene still demonstrates compiled Text. The dynamic
evaluator supports the same drawing vocabulary through
`Element::evaluate`. Re-evaluating an edited Element with different input values is
the first boundary needed for a future live preview.

The next design questions are dependency lookup for other Organon definitions,
public contracts for Rust-defined parts, and a preview host capable of executing
those implementations. Inputs/outputs here are values, not yet events or stateful
components. Those capabilities should remain optional for drawing-only users.

## Cargo integration

The application imports its `.organon.xml` files with `organon::import!`.
The macro compiles XML into named elements and their Builders within the
enclosing Rust module. No build script or generated-file inclusion is needed.
`build` returns the configured element, with native and fixed accessors for each
explicit export. Text layout and glyph realization
errors currently use static messages, as do the existing drawing errors.

Generated code uses `theoria-canvas`, `theoria-text`, `theoria-reactive`, and
`theoria-component` directly. It needs neither `organon-format` nor
`organon-compiler` at runtime. XML elements and Rust interfaces both use the
shared construction API provided by `organon`. Use
`organon-format` explicitly only when reading or editing XML in the host.
The compiler itself does not load fonts or depend on a renderer.

```toml
[dependencies]
# Example for a package under crates; adjust these paths for the consuming package.
organon = { path = "../organon" }
theoria-canvas = { path = "../theoria-canvas" }
theoria-text = { path = "../theoria-text" }
theoria-reactive = { path = "../theoria-reactive" }
theoria-component = { path = "../theoria-component" }
```

```rust
mod panel {
    organon::import!("src/panel.organon.xml");
}
// <element name="Panel"> provides panel::Panel::builder().
```

Paths are relative to the consuming package's `Cargo.toml`. XML-only changes
trigger recompilation; invalid XML produces a compiler error at the macro call.
Each invocation can compile an `<organon version="1">` file containing multiple
named `<element>` and `<interface>` declarations. Version belongs to the root;
its children have names but no version attribute. The initial standalone
`<element name="..." version="1">` form remains accepted. Rust chooses module
names and module visibility. XML imports and named instances connect definitions
within one invocation; separate Rust macro invocations create independent types.

The compiler remains available to tooling: `compile_xml` accepts source text, and
`generate_element_rust` accepts an `organon_format::ElementDefinition` for callers
already holding storage data.

The public Builder accepts fixed or reactive inputs. Build once and retain its
outputs across updates; the built graph preserves individual Let computations.

## Interfaces implemented in Rust

An interface describes typed inputs and named outputs. `inline="true"` selects
an output evaluation policy; fixedness is determined from actual reads at build
time. Interface types currently use the drawing primitive vocabulary: `f32`,
`String`, `Color`, `ColorField`, `Canvas`, `Path`, and `Fonts`, plus loaded
Element/interface names. `Result<T>` is also
accepted for inputs and outputs, with `&'static str` as the initial error type.
Escape its angle brackets in XML attributes (`Result&lt;Path&gt;`).

```xml
<organon version="1">
  <interface name="Scale">
    <input name="value" type="f32"/>
    <output name="scaled" type="f32"/>
  </interface>
</organon>
```

Import that file in a module and implement the common `Implementation<Scale>`
trait. The interface name itself is the struct holding output computations;
there is no `Output<Scale>` wrapper or per-interface implementation trait.

```rust
use organon::{Implementation, output};
use contracts::{Input, Scale};

struct Multiplier(f32);

impl Implementation<Scale> for Multiplier {
    fn build(&self, input: Input<Scale>) -> Scale {
        let value = input.value().clone();
        let factor = self.0;
        Scale {
            scaled: output(move |cx| *value.read(cx) * factor),
        }
    }
}

let scaled = Scale::builder()
    .value(3.0)
    .build(&reactive, &Multiplier(2.0))?;
assert_eq!(scaled.scaled_fixed(), Some(&6.0));
```

`Interface::Inputs` is a tuple of `Source<T>` handles. The generated local
`Input<I>` wrapper adds named accessors such as `input.value()`; it does not
generate a `ScaleInputs` struct. Builders use optional tuple slots and accept
fixed values, native Theoria Values, or Sources. All inputs must be supplied
before the implementation runs. Rust checks the output struct fields and types.
The connected result is `Built<Scale>`, whose accessors return native Theoria
Values or optional fixed references. `scaled_source()` preserves fixedness and
inline semantics when connecting to another Rust interface implementation.

`output(...)` stores a closure without executing it. After `Implementation::build`
returns its calculations, the enclosing Builder evaluates each output once in a
tracked context before returning the connected result. Reading only fixed Sources makes
that output fixed, even when unused inputs are reactive. Otherwise ordinary
outputs retain their initial Computed result; subsequent evaluation is lazy and
tracks conditional dependencies through `read(cx)`. Inline Sources evaluate in
their consumer's context. Their public native adapter is a stable Computed.
An output that returns `Err` remains a successful output value, independently
of other outputs. Missing inputs and evaluation-entry failures are build errors.

Calculations must depend only on immutable captures and tracked reads. Keep
construction-time resources in the implementation's fields and clone owned
handles into the closures. For mutable resources that should invalidate outputs,
capture a reactive Source and read it through the supplied context.

Run `cargo run -p organon --example native` for a complete fixed/reactive example.
The contract and behavior tests are in `crates/organon/tests/interface.rs`.
XML-level interface invocation and import-time implementation mapping are
supported as described below. Graphical preview registration remains separate. XML Element generation already uses
the same initial evaluation and value storage. The application still
edits/interprets single Elements with its separate diagnostic evaluator.

## Dependency imports and named instances

A file may import other files relative to its own directory:

```xml
<organon version="1">
  <import path="parts/tile.xml"/>
  <element name="Scene">
    <input name="width" type="f32"/>
    <let name="tile"><Tile width="{width}"/></let>
    <let name="path" export="true"><get name="path"><ref name="tile"/></get></let>
    <let name="drawing" export="true">
      <Draw source="#49dcb1"><Fill><get name="path"><ref name="tile"/></get></Fill></Draw>
    </let>
  </element>
</organon>
```

Here `parts/tile.xml` declares `Tile` with a `width` input and exported `path`.
Imports can contain further imports. Canonical paths identify files within one
compilation, so diamond dependencies and repeated imports generate one definition.
Import cycles, missing files, and same-named definitions from different files are
errors. There is no process-global macro cache: each compilation reads current
sources. The proc macro tracks every transitive source with `include_str!`, and
the compiler's file-to-file API emits a Cargo change directive for every source.

All loaded definitions currently share one flat namespace and are emitted into
the enclosing Rust module. Selective/public imports and namespace aliases are not
implemented. A single entry file may contain only imports. `compile_xml` performs
no filesystem access and rejects unresolved imports; tooling can instead call
`load_definitions(path)` followed by `generate_loaded_file(&loaded.file)`.

A capitalized custom tag inside a Let or get constructs an element or
interface. Attributes supply its inputs. Each instance is constructed once per
parent build, even when several outputs are used. File declaration order and
local Let order do not dictate evaluation order; dependencies do. Recursive
construction, missing/unknown inputs, incompatible types, and references to
non-exported outputs are rejected. Instances can be aliased, exported, and passed
to other Elements. An instance construction runs once even if its Let is inline;
inline never rebuilds it. Its named outputs retain their own evaluation policies.
There is no default output, including when an element has only one output.

Use `get` to select an output without naming the constructed instance:

```xml
<let name="drawing" export="true">
  <Draw source="#49dcb1">
    <Fill><get name="path"><Tile width="{width}"/></get></Fill>
  </Draw>
</let>
```

`get` has one fixed output name and exactly one Element-valued child expression.
The child can construct an XML Element or Rust interface, reference a binding,
or select an Element-valued output with another `get`. `ref` accepts only a
single binding identifier: `<ref name="tile.path"/>` is rejected.

```xml
<input name="button" type="Button"/>
<let name="icon" export="true"><get name="icon"><ref name="button"/></get></let>
<let name="path" export="true">
  <get name="path"><get name="icon"><ref name="button"/></get></get>
</let>
```

Here Button exports an Element-valued `icon`, which in turn exports `path`.
A matching input attribute can receive `{button}` or `{button.icon}`.
Attribute templates also support longer chains such as `{button.icon.width}`;
each step selects a declared public output, never an arbitrary Rust field.
A Let may name any intermediate Element or selected value. There is no implicit
output selection based on type, count, or position.

XML inputs use the loaded capitalized definition name as their type. Built-in
type and expression names are reserved and cannot name custom definitions. XML output types are
inferred from expressions; interfaces declare them explicitly, including
`Result&lt;Button&gt;`. Rust receives the generated Button type for XML instances,
or `Built<Contract>` for interface instances. These are concrete types local to
the import invocation. An input accepts either the built object or a native
`reactive::Value` carrying that object; an exported object has the usual native
value and fixed accessors. Recursive XML type dependencies are rejected.

A fixed Element object may contain reactive outputs. Selecting a child output
retains that output's fixed/reactive/inline behavior. A reactive Element input
can be replaced; selection follows the new object and tracks its child values.
A `Result<Element>` target propagates its error through selection as a single
`Result<T>`, and can recover on subsequent reactive updates. Selecting a field
of a primitive value is unsupported.

`get` also works in Canvas or Path child positions when its selected output
matches that position. Each construction expression creates one instance per
parent build; aliases and selections do not reconstruct it.

Arguments accept matching resource references, scalar/color literals, and string
templates. Fixedness and inline reads are preserved through shared Sources. A
`Result<T>` can be passed to an interface input declared as `Result<T>`; an
infallible `T` can also be lifted into that input. Passing `Result<T>` to a `T`
input requires explicit error handling and is rejected for now. Fallible numeric
or color template conversions likewise require a Result input. XML-defined input
types accept non-Result primitives and named Elements/interfaces.

Bind Rust implementations at the entry point:

```rust
mod scene {
    organon::import!("src/scene.xml", implementations {
        Scale = crate::ScaleImpl,
    });
}
```

`Scale` must name a loaded interface. Its XML uses `<Scale value="{width}"/>` like
other instances. The Rust expression is evaluated in the macro's module scope
for each instance construction and must return an `Implementation<Scale>`; it may
be a constructor or factory call. This is ordinary Rust type checking, not a
string-based runtime registry. XML cannot supply Rust implementation expressions.
Independent macro invocations bind their own generated types. For compiler API
callers, `generate_file_with_implementations` takes the resolved file and a map of
interface names to trusted host-authored Rust expressions.

The end-to-end example is exercised by `crates/organon/tests/imports.rs`, with its
XML files under `tests/fixtures/scene.xml` and `tests/fixtures/parts`. Editable
storage preserves instances, get expressions, and projections, but interpreted previews currently
report that component instances require compilation; preview implementation
registration and file-graph editing are separate work.

## Horizontal Text (provisional)

```xml
<element name="Example" version="1">
  <input name="fonts" type="Fonts"/>
  <input name="label" type="String"/>
  <input name="width" type="f32"/>
  <let name="drawing" export="true">
    <Text fonts="{fonts}" font_size="22" foreground="#e6f0ff" wrap_width="{width}">
      <s t="{label}"/>
    </Text>
  </let>
</element>
```

`fonts` is an ordered `Vec<theoria_text::font::Font>` supplied by the host. The
host loads these resources explicitly; the document has no implicit system font
lookup. A future editor can supply the same resources to preview the drawing.
Shaping, wrapping, glyph realization, and composition remain in generated code.

Only `<s t="..."/>` supplies literal text. Its `t` attribute is a template,
using the same `{name}` interpolation of `String` or `f32` values as other
attributes. For example, `<s t="こんにちは、{name}さん。"/>` inserts `name` in place.
Use `{{` and `}}` for literal braces; `$` is ordinary text. XML entities encode special
characters; use `&#9;` for a tab and `&#10;` for a hard line break. Literal whitespace in attributes
follows XML normalization. Canonical saving preserves newlines, tabs, and
carriage returns with numeric character references.

Text content is a sequence of `s`, `span`, and `br` elements:

```xml
<Text fonts="{fonts}" font_size="22" foreground="#e6f0ff">
  <s t="Hello, "/>
  <span font_size="28">
    <s t="{label}"/>
    <br/>
    <s t="Details" foreground="#aabbcc"/>
  </span>
  <s t="&#9;{{literal braces}}"/>
</Text>
```

`s` is a string leaf; it requires `t` and has no child elements. `span` groups
inline content and cannot carry `t`. Both support optional `fonts`, `font_size`,
and `foreground` overrides; unspecified fields inherit from the enclosing span
or Text's base style. `<br/>` inserts one newline using the inherited style.
Empty strings and empty spans are allowed. Adjacent fragments add no separator.
Fragment boundaries do not force shaping boundaries: Theoria may shape adjacent
compatible fragments together, including ligatures across them.

Element indentation is ignored. Non-whitespace raw text (including CDATA text)
is rejected; there is no trim/dedent mode or alternate literal syntax. Expanded
values retain whitespace and control characters, and are not parsed again as
XML or templates. Thus an input containing `<br/>` displays those characters,
whereas an actual newline in an input performs a line break. No `t`, `run`, or
`r` tag aliases are supported, and the former `Text.content` attribute is rejected.

Named text ranges and `.text` outputs are design directions, not implemented
interfaces. Paragraph/Path placement and ruby are separate future work.

Text requires `fonts`, `font_size`, and `foreground` attributes for its base
style. Its children contain only inline content; a `TextStyle` child is not
accepted. `font_size` must expand to a finite nonnegative scalar,
including zero as supported by Theoria. `foreground` accepts the same color
text or native ColorField/Color/Canvas references as `Draw.source`. Color codes
are Organon's straight-alpha sRGB bytes; native colors retain their color space. The
editable `organon_app::element::TextStyle` data is also available to Rust drawing
authors. It is not a resolved `theoria_text::text::TextStyle`.

Omit `wrap_width` to disable soft wrapping. A supplied wrap width must be finite
and nonnegative. Hard line breaks still apply. Wrapping does **not** clip long
unbreakable words or create a rectangular layout requirement for other
Elements. Text is drawn at the origin; use `Translate` to place it and
`SourceOver` to composite it. Empty or whitespace-only text yields a transparent
Canvas, so it can still participate in composition.

Theoria's measured horizontal `Text::width()` retains the requested wrap width,
even for short text. `height()` reports the resulting typographic layout
height. These differ from ink bounds and may exclude overflow. The current
Organon builder consumes the Text result to produce Canvas; it does not yet
expose these measurements or allow other Elements to reference them. A later
step will address measurement connections and padding without conflating
measurement, placement, and Coverage clipping.

## Preview and checks

From the repository root:

```powershell
cargo run -p organon-app
cargo run -p organon-app -- --png target/organon-text.png
cargo run -p organon-app -- --font C:/Windows/Fonts/segoeui.ttf --png target/organon-text.png
cargo test -p organon-app -p organon -p organon-format -p organon-compiler -p organon-proc-macro
```

The window redraws the generated Text when resized. Windows defaults to Segoe
UI; use repeated `--font` options to supply an explicit fallback chain. Other
platforms require `--font` and support the offscreen PNG path when Vulkan is
available. Tests use a bundled Ahem font and do not require installed fonts.
The preview's panel dimensions remain explicit; it is not yet a background
automatically sized to text measurements.

Tests follow the package boundaries:

| Location | Coverage |
| --- | --- |
| `crates/organon-format/tests` | XML structure, escaping, nesting limits, and format round trips using local fixtures. |
| `apps/organon/tests/sources.rs` | Fixed folding, Computed sharing, inline dependency tracking, and recovery from reactive errors. |
| `crates/organon-compiler/tests/compile.rs` and `text.rs` | Compilation entry points, input validation, and generated Rust syntax. |
| `crates/organon/tests/interface.rs` | Shared XML/Rust initial evaluation, actual dependency classification, inline forwarding, and evaluation-entry errors. |
| `crates/organon-compiler/tests/generated.rs` | Generates Rust during the test, then compiles and runs the Canvas, Text, and Rust-owned reactivity checks from `tests/fixtures`. |
| `crates/organon-compiler/tests/imports.rs` and `instances.rs` | File graphs, canonical-path deduplication, cycles, instance types and output visibility. |
| `crates/organon/tests/elements.rs` | Element-valued XML/native ports, nested selection, dynamic object replacement, and Result recovery. |
| `crates/organon/tests/imports.rs` | Imported XML and Rust instances, one construction for multiple outputs, and reactive error recovery. |
| `crates/organon-proc-macro/tests` | Imports from consumer-relative paths, independent modules, XML-only rebuilds, and compile-time diagnostics. |
| `apps/organon/tests` | Editing conversions, compiled/interpreted agreement, native hit testing, and preview integration. |

The generated-code test runs with ordinary `cargo test -p organon-compiler`.
It calls the compiler directly from the test function and creates a temporary
consumer under Cargo's target directory to compile and execute the generated
Rust. There is no test package in the workspace and no test build script.
The temporary consumer depends directly on Theoria crates (and test font data),
without the graphical application or Vulkan.
Its separate build cache avoids nested Cargo lock contention; test artifacts
are retained under `organon-generated-tests` for inspection. Its dependencies
come from the compiler's dev-dependencies and the workspace lockfile, so the
nested build can run offline.

XML fixtures belong to the compiler tests and are independent of the preview's
drawing files. The application builds the preview's panel, label, and badge.

`organon_app::CanvasExpr` is the application's editable drawing expression;
`organon_format::CanvasExpr` represents its stored structure. The compiler's typed expressions
are private implementation details.

## Attribute references

`{name}` refers to an input or Let; `{instance.output}` selects a public output.
Chains such as `{button.icon.path}` select successive Element outputs. Names currently use ASCII letters, digits,
and underscores, beginning with a letter or underscore; the compiler also checks
that they can be emitted as Rust field identifiers. Empty references, nested
braces, arbitrary Rust property access, arithmetic, and Rust format specifiers are unsupported.
The escape syntax matches Rust format strings, but this is not arbitrary Rust.

| Attribute | Accepted form |
| --- | --- |
| `s.t` | Literal text and interpolation of string or scalar inputs; the only inline text source. |
| Coordinates, dimensions, font size | Literal numbers or interpolated text, interpreted as `f32`. |
| `Draw.source`, `Text.foreground`, `s.foreground`, `span.foreground` | Literal or interpolated hex color text, or `{name}` selecting a native Color, ColorField, or Canvas input. |
| `Text.fonts`, `s.fonts`, `span.fonts` | Exactly one `{name}` selecting Fonts. |
| `ref.name` | A single fixed Input/Let identifier; no dots, braces, or interpolation. |
| `get.name` | A single fixed exported output identifier; its child supplies the Element. |
| `let.export` | Fixed `true` or `false`; defaults to false. |
| `let.inline` | Fixed `true` or `false`; defaults to false. Suppresses an independent Computed, without preventing fixed folding. |
| element version, input name/type, composition space | Fixed values; interpolation is not allowed. |

For textual attributes, both a lone `{name}` and in-place interpolation follow
the same expansion-then-interpretation rule. A string input can supply a whole
number (`width="{width}"`) or part of one (`width="{whole}.{fraction}"`).
String and scalar inputs can similarly construct color text (`source="#{rgb}"`).
Invalid expanded numbers or colors produce an `Err` output value; input values
are never parsed again as templates or XML. Literal-only templates are folded
at compile time. A lone native scalar can bypass its display/parse round trip.

Native Color, ColorField, Fonts, Canvas, and Path values remain typed resources rather than being
converted to strings. Native Color references preserve their color space; these
resource values cannot be interpolated into surrounding text.

The old `$name` reference syntax is no longer supported. In text it is literal;
in a numeric or resource attribute it fails validation. XML serialization keeps
the original brace escapes and references, not their expanded values.
