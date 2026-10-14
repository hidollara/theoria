# Organon

Organon describes construction graphs independently of their execution backend.
The Theoria integration builds graphics and reactive values; handwritten Rust owns
component mounting, events, state and evaluation timing.

## Crate boundaries

| Package | Responsibility |
| --- | --- |
| `organon-core` | Shared definition content, XML reading, import resolution, semantic validation and construction IR. No serialization, Cargo or Theoria dependency. |
| `organon-cargo` | Cargo package discovery, dependency aliases, declaration identities, Rust library connections and source-change tracking. |
| `organon-theoria` | Theoria construction contracts, value sources, attribute interpretation and native Element implementations. |
| `organon-theoria-compiler` | Rust generation from checked declarations; its construction plan is private. |
| `organon-theoria-proc-macro` | Macro input syntax, diagnostics and Theoria implementation binding. |

`organon-cargo` implements core's `loader::Sources` contract with `CargoSources`.
It owns Cargo metadata lookup, package-root `organon.xml` discovery, dependency
aliases and within-package declaration identities. Its Rust connection helpers
export and consume the `organon!` provider macro, check the declared module
location and register files read during expansion as rustc inputs. Facade crate
paths are rewritten structurally, preserving literals and qualified associated
items; generated code uses the runtime facade from the outset. These helpers
do not generate Theoria types or evaluation code.

Compilation uses this package adapter. Core contains no Cargo feature or package
discovery API. `library!` uses the compiler during macro expansion.

Core's shared vocabulary lives outside its syntax structures: `identifier` defines
`Identifier`, which syntax stores for definition, binding and port names, while
`types` defines input/output types and children
contracts, and `Error` is exported at the crate root. Neither `types` nor the
semantic IR depends on XML. `syntax` represents Organon constructs before name resolution;
`xml` reads that content from XML and owns XML-specific syntax checks and diagnostics.
Core has no XML writer or canonical serialization format.
File publication uses `Export`; Element and interface output mappings share `Provide`.

`TypeName` identifies a definition; `ValueType` pairs that name with a failure contract.
A ValueType always describes a complete declared contract. Semantic expressions
carry only their successful TypeName. OutputType distinguishes a declared contract
from an Element output whose failures are inferred; PredicateBody likewise separates
native error declarations from XML implementations. Inferred failure information
lives only in semantic::effects::Graph, including unresolved native requirements.
Core checks the known lower bound without storing it as an output contract; the
backend resolves the graph before selecting a public Rust result type.
`ChildrenType` describes the required child type and count range, not child values.
These types are shared by syntax and semantic contracts.
The default `xml` feature enables `roxmltree` and the XML reader. With
`default-features = false`, core has no dependency crates and still supports
programmatically constructed modules, import resolution and semantic analysis.

## Definition content and evaluation

`organon_core::syntax::Module` holds imports, contracts, declarations and
construction expressions before name resolution. Its `items` are import/use/export
directives or named `Definition`s: `Element`, `DeclareElement`, `Interface`,
`Predicate`, and `DeclarePredicate`.
An Element's ordered `members` use `ElementMember`; a DeclareElement's use
`DeclareElementMember`. Predicate definitions follow the same naming:
`Predicate`/`PredicateMember` and `DeclarePredicate`/`DeclarePredicateMember`.
`Module` names one source scope, not a new XML tag or Rust module.
Item, member and child order is
retained, while attribute values remain uninterpreted strings. It contains no
quotation, indentation, comment or attribute-order information, and imposes no
XML character restrictions on programmatically supplied attribute values.
This is shared language content, not an editable document or a persistence format.
It remains distinct from semantic IR because local names, imports and attribute
text have not yet been resolved.

The private modules under `syntax` follow language responsibilities:

| Module | Content |
| --- | --- |
| `module` | Source scope, ordered items and named definition variants |
| `element`, `declare_element` | Organon Element bodies and backend-supplied Element contracts |
| `predicate`, `declare_predicate` | Organon predicate bodies and backend-supplied predicate contracts |
| `expression` | `ValueExpression`, `Construction` and `PredicateExpression` |
| `binding` | `Let`, `Bind` and the shared `LocalBinding` alternatives |
| `control` | `Select`, `Try`, ordered handlers, and the `Branch` contents shared by case and catch |
| `interface` | Output contracts and explicit `Implementation`s |
| `provide` | Publishing an existing binding as an Element or interface output |
| `import`, `export` | Import/use directives and publication from a source scope |

All public syntax types are re-exported directly from `syntax`. A `Predicate` is
a named definition; a `PredicateExpression` calls one or combines predicates with
all/any/not. A `Construction` describes an Element construction request. Its
`arguments`, like those of a predicate call, contain attribute text supplied to
declared inputs. `Select::result_type` and `Try::result_type` name their successful
result type; an `Exit` can instead select a throw type through `parser_type()`.
Validation of each named definition belongs to that definition;
`Module::validate()` delegates through `Definition::validate()`.

`organon_core::xml::parse` reads a complete source; `xml::read` extracts definitions
from an already parsed XML root. Neither operation specifies how those definitions
should be written back. An editor owns its source representation and extracts
shared definitions for analysis when the edited content is valid.

`organon_core::loader` resolves file imports, public names and aliases using a
host-supplied source resolver. `Sources::Id` is the host's stable source identity;
`Sources::read` supplies a `Module`, and `Sources::resolve` resolves relative or
named dependency imports to another ID. Core performs no filesystem operations.
`ImportSource::Dependency` is interpreted as a Cargo dependency only by the Cargo host.
An ID can represent an unsaved document or a generated module without a file.
Each source is read once per load, and its unresolved content is retained in `Loaded::sources` before import
names are rewritten. `Loaded::definitions` is the flattened analysis input, not
a document to save. Package discovery can inspect the retained imports without
reparsing source text.
`Loaded::source_order` records dependency order; `DefinitionId` combines a source
ID with the original definition name. `CargoSources` uses normalized paths as IDs
and handles disk reading, relative paths and Cargo lookup. An unsaved file location
can be used beneath an existing directory without creating that file.
Type names are resolved in their original file's scope before flattening. An
unimported private type cannot become visible by matching a name in another file.

`organon_core::semantic` resolves declarations, input/output contracts and
construction dependencies into an IR. `Program::contracts` contains a `Contract`
for each Element or interface: inputs, output types, optional inputs, child constraints and
interface conformance. `Program::bodies` contains only Organon Element implementations
in dependency order. A `Body` stores bindings, output mappings (`OutputBinding`)
and interface implementations; it does not duplicate the input or output contracts.
Consumers read the corresponding contract when generating or evaluating a body.
Contracts come from `declare-element`, `interface` and `element`;
there is no separate programmatic constructor registry in core.
`DefinitionKind` preserves this distinction in the IR; whether a definition can
be constructed as a tag is derived from that kind, not used to identify interfaces.

Predicate contracts and bodies are stored in `Program::predicates`.
`declare-predicate` declares a native condition and `predicate` defines one
from a single predicate call or an `all`, `any` or `not` expression.
Both cases and handlers may introduce local `let` and `bind` declarations.
Their single optional condition guards the branch. Sibling branches have
separate scopes, and a handler can see enclosing names and its own error
binding. A name declared inside the protected body is private to that body.

```xml
<try type="String" result="{label}">
  <let name="label">
    <select type="String">
      <case result="ready">
        <Ready item="{item}"/>
      </case>
      <case throw="Unavailable" result="{reason}"/>
    </select>
  </let>
  <catch type="Unavailable" result="offline"/>
  <unreachable/>
</try>
```

The enclosing success type selects the parser for `result`. A branch with
`throw` parses its result as the given error type instead. A port lists the
error types its contract admits with `<error type="…"/>`. Core tracks errors
visible from declarations and resolved references. A typed, unconditional
handler covers matching reasons, including reasons implementing its interface.
References embedded in native attribute text require the library parser.
The compiler emits Rust closures for branches and connects each output to a
Theoria calculation. The preview prepares the same branch operations from the
current IR. Both evaluate cases in order and read the selected result. A failed
guard propagates its reason. A failure raised inside a handler escapes to the
enclosing try. A try checks its `catch` and `unreachable` clauses in document
order. The first match decides the outcome. An empty `<unreachable type="…"/>`
asserts that the matching failure cannot occur. Omitting `type` covers every
remaining failure. Reaching either assertion reports a contract violation that
bypasses enclosing catches, as does a select with no matching case. In the Rust
API this is a build error on first evaluation or a panic if a later reactive
update violates the contract.

A native predicate supplies `fn test` in its `#[implement]` block. Parameters
borrow its XML inputs in declaration order. It returns `bool` for a total
predicate or `Result<bool, E>` for a fallible predicate. A single declared error
uses its native Rust payload. Multiple allowed types form a closed
`DeclaredFailure` set. In an `#[implement]` method, write `Result<T, _>` to
use the error type from that output or predicate's declaration. For example,
`Err(DeclaredFailure::new::<types::InvalidSize, _>(size))` requires that
the port list `InvalidSize` and that `size` have its Rust payload type.
A typed catch can bind the payload as a concrete type or an interface.

An XML `predicate` can list `<error type="…"/>` on an input, just like an
Element or native predicate. The argument is evaluated when the predicate
reads it. A local `try` can recover a declared reason. A reason outside
the input contract propagates to the caller, even past a local catch-all.
The predicate's own failure types are inferred from its body.

XML Elements can accept failures on their inputs and handle them locally:

```xml
<input name="text" type="String">
  <error type="Unavailable"/>
</input>
<let name="label">
  <try type="String" result="{text}">
    <catch type="Unavailable" result="offline"/>
  </try>
</let>
<provide name="label" ref="label"/>
```

Rust callers can pass a `Result` or a reactive value carrying that `Result`.
Providing `text` directly preserves both outcomes. The handler above returns a
literal, so the public type of `label` is `Source<String>`. Reading `{text}`
again in the handler would expose its failure on that output.

Native attribute parsers are const functions. They record typed reference
requirements through ParseContext while interpreting the text. Core preserves
these unresolved requirements in an effect graph; generated Rust runs the parsers
in const evaluation and computes each output's failure set from those exact
requirements. No reference syntax is assumed by core and no XML declaration of
that syntax is needed. The same attribute has the same effects in a root binding,
a case, and a handler. Merely sharing a scope with a fallible binding adds no error.

The generated Rust type is T for an empty failure set, Result<T, E> for one reason,
and Result<T, DeclaredFailure<...>> for multiple reasons. Interface output bounds
are checked after these requirements are known. Native parsers and XML definitions
can remain in the same crate; there is no separate analysis build or cfg mode.
Dynamic interpretation invokes the same parsers at runtime.

There is one XML schema and one semantic frontend. The compiler has no implicit
Rect/Text/Color syntax and no fallback for files without native value declarations.
Standard elements are imported from `organon-theoria` like any external library.

## Runtime modules and public entry points

| Implementation module | Responsibility | Public entry point |
| --- | --- | --- |
| `source` | Calculation binding, fixed values and shared reactive computations | `Calculation`, `Source`, `Read`, `ReadContext` |
| `contract` | Rust declaration, output and interface contracts | Root traits: `Type`, `Declaration`, `Output`, `Evaluate`, `Provide` |
| `attribute` | String interpretation, typed dependencies and opaque reference values | `ParseContext`, parser helpers, `Registry` and `Value` |
| `elements` | Individual standard Element implementations | Generated public types, `types`, `attribute_types()` |
| `effects` | Const failure-set evaluation and Rust result-type selection | Doc-hidden support for generated code |

The source modules `source`, `contract`, and `elements` are private.
Generated cross-crate contracts remain public but doc-hidden. Application and
library authors do not need to reference `__organon` directly.

`organon-theoria` does not interpret IR or retain output dictionaries. Its
attribute registry contains type contracts used by ordinary generated code;
it has no entry point for constructing an Element by name.

## XML construction

Every file has an `organon` root. Elements receive named inputs, construct local
values and publish selected bindings. There is no implicit default output.

```xml
<organon version="1">
  <import from="organon-theoria"/>
  <element name="Panel">
    <input name="width" type="number"/>
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
`<ref name="panel"/>` uses the binding's own type. With `type="Outline"` the
same reference uses its declared `Outline` implementation and preserves any
failures of the binding. A `bind` around that reference sees the outputs of
`Outline`; other uses of `panel` retain its original type.
`bind` extracts named outputs into local
bindings and is an Element-level declaration, not an expression wrapper.
`provide` supplies an output: `name` is the output name and `ref` is the local
binding supplying its value. Inside an interface `impl`, the declared output
supplies the expected type, so a binding implementing that interface can be
provided directly.
Nested `get` and dotted reference selection are not supported.

Generic element tags are library constructions. Their receiving declarations
define inputs and children. The standard declarations in
[`organon.xml`](../crates/organon-theoria/organon.xml) are the authoritative port
and interface definitions. `Fill` accepts Outline children, `Draw` combines
ColorSource and CoverageSource, and `Text` accepts TextContent children.
`s`, `span`, and `br` implement TextContent through ordinary declarations.

Both `declare-element` and `element` accept `child="T"` to declare the type of each
incoming child. `min-children` and `max-children` constrain the count; omitted
bounds mean zero and unbounded, respectively. Without `child`, the definition
accepts no children, and count attributes are invalid.

Inside an XML element, `<children/>` inserts that element's incoming sequence
into a construction's children. It is not a single value or a declaration:

```xml
<element name="Quoted" child="TextContent">
  <bind text="text">
    <span>
      <s t="「"/>
      <children/>
      <s t="」"/>
    </span>
  </bind>
  <provide name="text" ref="text"/>
</element>
```

The sequence belongs to the enclosing element definition, regardless of the
depth of the `<children/>` tag. Its items retain their order and are shared when
inserted more than once. Semantic analysis checks both the child type and that
the full incoming count range fits the receiving construction's contract.
Native declarations receive the same ordered sequence through `input.children()`.

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

## Validation

```sh
cargo test -p organon-core -p organon-cargo -p organon-theoria -p organon-theoria-compiler -p organon-theoria-proc-macro -j 1
cargo check -p organon-core --no-default-features
```

Tests cover ordered editing, import visibility and aliases, generated-library
linkage, dynamic dependencies, fixed propagation and shared evaluation, attribute contracts,
and agreement between const and dynamic attribute parsing.

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
Rust construction plan. Straight-line bindings and scoped try/select bodies remain
structured until code generation; the plan does not hold pre-generated body code.
Both routes preserve Source forwarding and sharing. It does not own another XML
schema or primitive parser.

## Native libraries: standard and external

`organon-theoria/organon.xml` declares the standard library, and
`organon-theoria/src/elements/` implements it, grouped into value, color, geometry,
drawing, and text modules. `elements.rs` groups these individual Element
implementations. External providers use the same
declarations and Rust API. Consumers import a provider through XML and depend on
its Cargo package; they do not repeat the implementation registration.

```xml
<organon version="1">
  <use from="organon-theoria" name="number"/>
  <declare-element name="Choice">
    <input name="choose" type="number"/>
    <input name="left" type="number"/>
    <input name="right" type="number"/>
    <output name="selected" type="number"/>
    <output name="alias" type="number"/>
    <output name="forwarded" type="number"/>
  </declare-element>
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
depend on `right`. Accessors return `Read<T>`; `into_cloned()` moves an owned
result or clones a borrowed result. Optional inputs return `Read<Option<T>>`.

`input.children()` reads the ordered child collection. `input.read(&source)`
reads a nested interface output in the same evaluation. Functions may explicitly
accept `cx: &ReadContext<'_, '_>` when passing a context to lower-level helpers.
There is no input-handle accessor in this evaluation view. Direct forwarding is
expressed by `forward!`, which returns the existing source without another computation.

Implementation methods remain inherent methods on the generated marker. Helper
methods and `Self::helper(...)` calls, including calls inside Rust macros, retain
ordinary Rust behavior. Only the `Input<'_, Self>` shorthand and `Self::Value`
types are expanded; unrelated Rust types named `Input` are left intact.

Multiple `output` annotations may select one function. When constructing a native
Element, outputs of that function share one Source and its bound result.
`Calculation::new` stores a computation; binding evaluates once
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
<declare-element name="Pair" tag="false"/>
```

```rust,ignore
#[organon_theoria::implement]
impl Pair {
    type Value = (f32, String);

    #[parse]
    const fn parse<'a>(
        text: &'a str,
        cx: &mut organon_theoria::attribute::ParseContext<'_>,
    ) -> Result<
        impl FnOnce(&mut organon_theoria::attribute::ResolveContext<'_>)
            -> Result<organon_theoria::Source<Self::Value>, String> + use<'a>,
        organon_theoria::attribute::ParseError,
    > {
        let mut colon = 0;
        while colon < text.len() && text.as_bytes()[colon] != b':' {
            colon += 1;
        }
        if colon == text.len() {
            return Err(organon_theoria::attribute::ParseError::new("expected number:label"));
        }
        let (number, rest) = text.split_at(colon);
        let label = rest.split_at(1).1;
        let number = match cx.reference::<organon_theoria::types::number>(number) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        let label = match cx.reference::<organon_theoria::types::String>(label) {
            Ok(value) => value,
            Err(error) => return Err(error),
        };
        Ok(move |cx: &mut organon_theoria::attribute::ResolveContext<'_>| {
            let number = number.resolve(cx)?;
            let label = label.resolve(cx)?;
            Ok(organon_theoria::Calculation::new(move |cx| {
                (number.read(cx).into_cloned(), label.read(cx).into_cloned())
            }).into_deferred())
        })
    }
}
```

This parser's syntax is its own choice. It requests Organon marker types, which
determine the Rust values returned by the references. Different references can
have different types. `ParseContext` contains only binding names, declaration
identities, interface contracts and fallibility. It returns checked reference
descriptors without reading or constructing Sources. The returned closure connects
them through `ResolveContext`. Static preparation also verifies that construction
uses only the references recorded by the parser. The graph is
ordered and checked for cycles at construction; later declarations remain usable
as forward references.

The #[parse] method must be a const fn. For static XML, its syntax and
reference-contract errors fail compilation. It returns an ordinary construction
closure; this closure never reparses on reactive updates. The closure needs neither
Copy nor Clone and can move its captured data into a Source. The parser and its
constructors remain in the same library crate. Runtime interpretation calls the
same const-capable parser with dynamic text.

Const parsing does not execute the construction closure or its evaluation functions.
For example, the color parser computes RGBA bytes and validates references at compile
time, but calls the native Color constructor during construction. The standard
color, composition-space, string-template and scalar parsers are const.

Organon attributes use the type name `number`. Its current Theoria implementation
stores values as Rust `f32`. The literal syntax follows the [XSD 1.1 decimal lexical representation](https://www.w3.org/TR/xmlschema11-2/#decimal):
an optional sign, ASCII digits and an optional decimal point, with at least one
digit. Surrounding XML whitespace is ignored; internal whitespace, exponents,
NaN and infinity are rejected. This borrows the lexical syntax, not XSD's
arbitrary-precision decimal value space. Scalars currently use IEEE 754 binary32
and round to nearest, ties to even, without intermediate floating-point rounding.
Subnormal values and signed zero are preserved; sufficiently small magnitudes
round to signed zero. A result that would round to infinity is an error.

The const converter compares the exact decimal input against binary32 rounding
midpoints using integer arithmetic. It neither truncates long inputs nor parses
through f64. Static attributes capture the resulting f32 in their construction
closure; dynamic interpretation runs the same converter once per construction.
This deliberately simple implementation is checked against Rust's standard f32
parser across rounding boundaries and varied decimal inputs. Literal validation
does not prohibit nonfinite values produced by later computations or Rust inputs.

A const parser with variable-length data can accept a third argument:
`ParseBuffer<'a, Part<'a>, N>`, with a `const N: usize` parameter. `Part` is a
library-defined Copy type; it is not a compiler IR. The parser pushes entries
into the buffer and moves it into its construction closure, which reads them
through `iter()`. It does not implement a separate size calculation.

For static XML, generated code first runs this same parser with counting storage,
without executing its construction closure, then runs it with an exact-sized
owned array. Both passes occur in Rust const evaluation. The returned closure
retains the parsed entries; construction does not scan the attribute again.
For dynamic text, the generated entry counts at runtime, allocates a slice, and
runs the same parser and construction closure while that storage is alive.
The resulting Source owns the data it needs beyond construction.
The standard string parser uses this mechanism for literals, escaped braces and
checked Display references. It still creates Strings and connects Sources at
construction, and performs concatenation during evaluation; it does not generate
individual `format!` expressions.

The outer Result reports interpretation errors, including invalid literals and
unknown or incompatible references. Resolving a validated descriptor yields
`Source<T>`; reads do not introduce another Result. Before binding, a Source can
defer reference reads until its environment is ready.
Binding evaluates actual dependencies and retains a fixed value or a shared Computed.
A Result belongs inside the Source only when it is part of the value's contract.
Declared failures are handled by the typed evaluation graph, not by an attribute-specific
Result API. Parsers describe the successful value and its required references. Before
invoking the constructed calculation, evaluation obtains those dependencies: an
unaccepted failure propagates to the caller, while a receiving input that declares
that reason gets its typed Result. Successful values are lifted into Ok for such
an input. This is the same path for literals, references and compound attributes.
The attribute Scope stores only successful values; it has no string-error variant.
Each binding declares its Organon identity and implemented interfaces before parsing.
Sharing a Rust value type does not make two Organon declarations interchangeable;
the registry selects interface implementations by the declared identity, including
when that declaration is imported under an alias.
Dependency ordering guarantees initialization before evaluation.
Violating that ordering or a declared native type is an implementation invariant
violation, not a user-facing parse failure.

There is no additional Parsed/Plan interpreter. After the construction closure
runs, `Attribute<T>` contains `Source<T>` and the collected dependency names.
Fixed literals use `Source::from(value)`.

A parser can use `attribute::braced_reference(text)` to distinguish a whole
`{name}` reference from a literal. Literal parsing and reference requirements
are both decided before the construction closure is returned.

String attributes, including `s.t`, use `attribute::template(text, cx, parts)` instead.
They always interpret format strings: `{name}` requires the standard `Display`
interface (`text: String`), and `{{` / `}}` escape braces. Formatting capability
is checked before evaluation; interface outputs are read in the active evaluation
context so their dependencies are tracked. ParseContext validates and records
typed requirements; it does not define either syntax.
Template scanning belongs to `organon-theoria`; `organon-core` does not prescribe
the attribute grammar of a library's string type.

Parsers are registered only by `#[parse]`, independently of the method name.
A method named `parse` without that attribute is an ordinary helper. Each implementation
allows one parser; a parser cannot also carry `#[output]`. There is no custom
`coerce` hook: references request a contract, and generated type/interface resolution
extracts the value after checking that contract. Typed reference extraction errors
are not retried as string conversions.

Without a `#[parse]` method, a type or interface accepts a bare reference name.
The definition owner may override this once in its implementation block.
`ColorSource`, for example, accepts a bare reference or a color literal.
There is no per-import parser override and no XML parser selector. The proc macro
does not execute library Rust code: it emits calls in Rust const contexts. Rustc
evaluates the parsers, and only the returned construction closures run at startup.

### Interface outputs

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

Interface output names have their own namespace. A `declare-element` contains
contracts and uses `implements` to declare interface support. Its Rust
implementation supplies every output, including interface outputs. A method can
carry both `#[output(name = "path")]` and
`#[output(interface = "Outline", name = "path")]` to share its calculation.
Inputs can be forwarded directly with `forward!(input => Interface.output)`.

XML `impl` / `provide` mappings belong to `element` definitions. An XML Element
can wrap a native Element through its public outputs and implement an interface
with those values. Each Element's implementation is complete in either XML or Rust.
`Rect` and `Polygon` implement Outline, Fill consumes Outline, and Draw consumes
CoverageSource and ColorSource. `s`, `span` and `br` implement TextContent; their
style output is a StyleOverride and their content preserves nested text structure.

### Output ownership

`Source<T>` is the output of Organon construction, including named outputs and interface
projections. `Calculation::bind` classifies actual dependencies and returns a Source:
fixed data or a shared Computed. It does not create a separate external adapter.
`let` names a value; it does not specify caching or code expansion. A `ref` alias
forwards the existing Source without reading or copying its value. There is no
`inline` attribute. Optimization policy belongs to the backend.

Cloning a bound Source shares its fixed data or existing Reactive handle.
Rust code can read Sources in its own Theoria computation using `ReadContext::new(cx)`.
Organon does not guarantee identity across independently constructed Rust adapters.

Internal adapters defer reads while resolving forward references, selecting nested
outputs, or constructing within an active evaluation. This is evaluation plumbing,
not an XML optimization policy. Such adapters run in the consuming calculation's
context so that actual dependencies and fixedness are preserved.
