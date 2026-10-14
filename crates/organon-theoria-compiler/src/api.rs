//! Public construction API. Evaluation storage stays behind native output handles.
use std::{collections::HashSet, fmt::Write};

use crate::ir::Element;

use crate::{Error, bind::OutputBinding, element::type_name};

pub(crate) fn validate(definition: &Element) -> Result<(), Error> {
    if !identifier(&definition.name)
        || definition.name.starts_with("__")
        || matches!(
            definition.name.as_str(),
            "Builder"
                | "Input"
                | "Inputs"
                | "Outputs"
                | "Sources"
                | "Result"
                | "Option"
                | "Ok"
                | "Err"
                | "Some"
                | "None"
                | "From"
                | "Into"
                | "Clone"
                | "Debug"
                | "Fn"
                | "FnOnce"
                | "f32"
                | "f64"
                | "str"
        )
    {
        return Err(Error(format!(
            "element name {:?} conflicts with generated Rust vocabulary",
            definition.name
        )));
    }
    for input in definition.inputs() {
        if matches!(
            input.name.as_str(),
            "build" | "build_in" | "component" | "__build" | "__element"
        ) {
            return Err(Error(format!(
                "input {:?} conflicts with a construction method",
                input.name
            )));
        }
    }
    let mut methods = HashSet::from(["builder".to_owned()]);
    for provided in definition.outputs() {
        let name = provided.public_name();
        if !identifier(name) {
            return Err(Error(format!("invalid Rust output {name:?}")));
        }
        for name in [name.to_owned(), format!("{name}_fixed")] {
            if !methods.insert(name.clone()) {
                return Err(Error(format!("output method {name:?} is ambiguous")));
            }
        }
    }
    Ok(())
}

pub(crate) fn generate(definition: &Element, outputs: &[OutputBinding], output: &mut String) {
    output.push_str("pub type Input<T> = ::organon_theoria::Source<T>;\n");
    let name = &definition.name;
    writeln!(
        output,
        "#[allow(dead_code, non_camel_case_types)]\n#[derive(Clone, Debug)]\npub struct r#{name} {{"
    )
    .unwrap();
    for provided in outputs {
        writeln!(
            output,
            "r#{}: ::organon_theoria::Source<{}>,",
            provided.name,
            provided.result_type()
        )
        .unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "#[allow(dead_code)]\nimpl r#{name} {{\npub fn builder() -> Builder<Self> {{ Builder {{ __element: ::std::marker::PhantomData,").unwrap();
    for input in definition.inputs() {
        writeln!(output, "r#{}: None,", input.name).unwrap();
    }
    output.push_str("} }\n");
    for provided in outputs {
        if !definition.outputs().any(|e| e.name == provided.name) {
            continue;
        }
        let field = &provided.name;
        let ty = provided.result_type();
        writeln!(output, "pub fn r#{field}(&self) -> ::organon_theoria::Source<{ty}> {{ self.r#{field}.clone() }}\npub fn r#{field}_fixed(&self) -> Option<&{ty}> {{ self.r#{field}.as_fixed() }}").unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "#[allow(dead_code)]\n#[derive(Clone, Debug)]\npub struct Builder<E> {{\n__element: ::std::marker::PhantomData<fn() -> E>,").unwrap();
    for input in definition.inputs() {
        writeln!(
            output,
            "r#{}: Option<Input<{}>>,",
            input.name,
            type_name(&input.kind)
        )
        .unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "#[allow(dead_code)]\nimpl Builder<r#{name}> {{").unwrap();
    for input in definition.inputs() {
        writeln!(output, "pub fn r#{}(mut self, value: impl Into<Input<{}>>) -> Self {{ self.r#{} = Some(value.into()); self }}", input.name, type_name(&input.kind), input.name).unwrap();
    }
    writeln!(output, "pub fn build(self, mut context: &::theoria_reactive::Reactive) -> Result<r#{name}, ::organon_theoria::BuildError> {{ self.__build(&mut context) }}\npub fn build_in(self, context: &mut ::theoria_component::MountContext<'_>) -> Result<r#{name}, ::organon_theoria::BuildError> {{ self.__build(context) }}").unwrap();
    // Component::mount returns nothing. A construction callback explicitly hands
    // output handles to the parent without creating another ComponentInstance.
    writeln!(output, "pub fn component<__Receive: FnOnce(Result<r#{name}, ::organon_theoria::BuildError>)>(self, receive: __Receive) -> impl ::theoria_component::Component {{\nstruct __Mount<__Receive> {{ builder: Builder<r#{name}>, receive: __Receive }}\nimpl<__Receive: FnOnce(Result<r#{name}, ::organon_theoria::BuildError>)> ::theoria_component::Component for __Mount<__Receive> {{\nfn mount(self, context: &mut ::theoria_component::MountContext<'_>) {{ (self.receive)(self.builder.build_in(context)); }}\n}}\n__Mount {{ builder: self, receive }}\n}}").unwrap();
    writeln!(output, "#[doc(hidden)] pub fn __build(self, context: &mut impl ::organon_theoria::BindingContext) -> Result<r#{name}, ::organon_theoria::BuildError> {{\nlet inputs = Sources {{").unwrap();
    for input in definition.inputs() {
        writeln!(
            output,
            "r#{}: self.r#{}.ok_or(::organon_theoria::BuildError::MissingInput({:?}))?,",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    output.push_str("};\nlet outputs = bind_graph(context, &inputs)?;\n");
    writeln!(output, "Ok(r#{name} {{").unwrap();
    for provided in outputs {
        writeln!(output, "r#{}: outputs.r#{},", provided.name, provided.name).unwrap();
    }
    output.push_str("})\n}\n}\n");
}

pub(crate) fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !matches!(name, "_" | "self" | "Self" | "super" | "crate")
}
