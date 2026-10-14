//! Public construction API. Evaluation storage stays behind native output handles.
use std::{collections::HashSet, fmt::Write};

use organon_format::{ElementDefinition, Input};

use crate::{Error, bind::Binding, element::type_name, ir};

pub(crate) fn validate(definition: &ElementDefinition) -> Result<(), Error> {
    if !ir::identifier(&definition.name)
        || definition.name.starts_with("__")
        || matches!(
            definition.name.as_str(),
            "Builder"
                | "Input"
                | "Inputs"
                | "Outputs"
                | "Sources"
                | "Exports"
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
    for input in &definition.inputs {
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
    for export in definition.definitions.iter().filter(|d| d.export) {
        for name in [
            export.name.clone(),
            format!("{}_fixed", export.name),
            format!("{}_source", export.name),
        ] {
            if !methods.insert(name.clone()) {
                return Err(Error(format!("output method {name:?} is ambiguous")));
            }
        }
    }
    Ok(())
}

pub(crate) fn generate(
    definition: &ElementDefinition,
    plans: &[Binding],
    exports: &[Input],
    output: &mut String,
) {
    output.push_str("pub type Input<T> = ::organon::Source<T>;\n");
    let name = &definition.name;
    writeln!(
        output,
        "#[allow(dead_code, non_camel_case_types)]\n#[derive(Clone, Debug)]\npub struct r#{name} {{"
    )
    .unwrap();
    for export in exports {
        let plan = plans.iter().find(|p| p.name == export.name).unwrap();
        writeln!(
            output,
            "r#{}: ::organon::Bound<{}>,",
            export.name,
            plan.result_type()
        )
        .unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "#[allow(dead_code)]\nimpl r#{name} {{\npub fn builder() -> Builder<Self> {{ Builder {{ __element: ::std::marker::PhantomData,").unwrap();
    for input in &definition.inputs {
        writeln!(output, "r#{}: None,", input.name).unwrap();
    }
    output.push_str("} }\n");
    for export in exports {
        let plan = plans.iter().find(|p| p.name == export.name).unwrap();
        let field = &export.name;
        let ty = plan.result_type();
        writeln!(output, "pub fn r#{field}(&self) -> ::theoria_reactive::Value<{ty}> {{ self.r#{field}.value() }}\npub fn r#{field}_fixed(&self) -> Option<&{ty}> {{ self.r#{field}.as_fixed() }}").unwrap();
    }
    for export in exports {
        let plan = plans.iter().find(|p| p.name == export.name).unwrap();
        let field = &export.name;
        let ty = plan.result_type();
        writeln!(output, "pub(crate) fn r#{field}_source(&self) -> ::organon::Source<{ty}> {{ self.r#{field}.source() }}").unwrap();
    }
    output.push_str("}\n");
    writeln!(output, "#[allow(dead_code)]\n#[derive(Clone, Debug)]\npub struct Builder<E> {{\n__element: ::std::marker::PhantomData<fn() -> E>,").unwrap();
    for input in &definition.inputs {
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
    for input in &definition.inputs {
        writeln!(output, "pub fn r#{}(mut self, value: impl Into<Input<{}>>) -> Self {{ self.r#{} = Some(value.into()); self }}", input.name, type_name(&input.kind), input.name).unwrap();
    }
    writeln!(output, "pub fn build(self, mut context: &::theoria_reactive::Reactive) -> Result<r#{name}, ::organon::BuildError> {{ self.__build(&mut context) }}\npub fn build_in(self, context: &mut ::theoria_component::MountContext<'_>) -> Result<r#{name}, ::organon::BuildError> {{ self.__build(context) }}").unwrap();
    // Component::mount returns nothing. A construction callback explicitly hands
    // output handles to the parent without creating another ComponentInstance.
    writeln!(output, "pub fn component<__Receive: FnOnce(Result<r#{name}, ::organon::BuildError>)>(self, receive: __Receive) -> impl ::theoria_component::Component {{\nstruct __Mount<__Receive> {{ builder: Builder<r#{name}>, receive: __Receive }}\nimpl<__Receive: FnOnce(Result<r#{name}, ::organon::BuildError>)> ::theoria_component::Component for __Mount<__Receive> {{\nfn mount(self, context: &mut ::theoria_component::MountContext<'_>) {{ (self.receive)(self.builder.build_in(context)); }}\n}}\n__Mount {{ builder: self, receive }}\n}}").unwrap();
    writeln!(output, "pub(crate) fn __build(self, context: &mut impl ::organon::BindingContext) -> Result<r#{name}, ::organon::BuildError> {{\nlet inputs = Sources {{").unwrap();
    for input in &definition.inputs {
        writeln!(
            output,
            "r#{}: self.r#{}.ok_or(::organon::BuildError::MissingInput({:?}))?,",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    output.push_str("};\nlet exports = bind_graph(context, &inputs)?;\n");
    writeln!(output, "Ok(r#{name} {{").unwrap();
    for export in exports {
        writeln!(output, "r#{}: exports.r#{},", export.name, export.name).unwrap();
    }
    output.push_str("})\n}\n}\n");
}
