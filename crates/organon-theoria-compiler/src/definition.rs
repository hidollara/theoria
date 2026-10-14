//! Contracts use tuple storage and local generic wrappers, not named input structs.
use crate::ir::{Definition, External};
use crate::types::ValueType;
use crate::{Error, element::type_name};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

fn ty(value: &ValueType) -> String {
    let name = type_name(&value.kind)
        .trim_start_matches("super::")
        .to_owned();
    if value.fallible {
        format!("::std::result::Result<{name}, &'static str>")
    } else {
        name
    }
}

fn tuple(values: impl Iterator<Item = String>) -> String {
    format!("({})", values.map(|v| format!("{v},")).collect::<String>())
}

pub(crate) fn generate_program(
    program: &crate::ir::Program,
    libraries: &HashMap<String, String>,
) -> Result<String, Error> {
    let file = program;
    let mut signatures = HashMap::new();
    let mut generated = HashMap::new();
    for definition in file.definitions() {
        if let Definition::External(value) = definition {
            signatures.insert(
                value.name.clone(),
                crate::instance::Signature {
                    inputs: value.inputs().cloned().collect(),
                    external: true,
                },
            );
        }
    }
    for value in program.elements() {
        let code = crate::element::generate_resolved(value, &signatures)?;
        signatures.insert(
            value.name.clone(),
            crate::instance::Signature {
                inputs: value
                    .inputs()
                    .map(|i| crate::types::Port {
                        name: i.name.clone(),
                        ty: ValueType {
                            kind: i.kind.clone(),
                            fallible: false,
                        },
                    })
                    .collect(),
                external: false,
            },
        );
        generated.insert(value.name.clone(), code);
    }
    let mut code = String::from(
        "#[doc(hidden)]\n#[allow(non_camel_case_types)]\npub mod __organon_values {\n",
    );
    for definition in file.definitions() {
        let name = definition.name();
        let target = if let Some(provider) = libraries.get(name) {
            if matches!(definition, Definition::External(_)) {
                format!("super::{provider}::Built<super::{provider}::r#{name}>")
            } else {
                format!("super::{provider}::r#{name}")
            }
        } else if matches!(definition, Definition::External(_)) {
            format!("super::Built<super::r#{name}>")
        } else {
            format!("super::r#{name}")
        };
        writeln!(code, "pub type r#{name} = {target};").unwrap();
    }
    code.push_str("}\n");
    if file
        .definitions()
        .any(|v| matches!(v, Definition::External(_)) && !libraries.contains_key(v.name()))
    {
        code.push_str(
            "pub struct Input<I: ::organon_theoria::Declaration> { values: I::Inputs }\n\
            pub struct Builder<I: ::organon_theoria::Declaration> { values: I::Arguments }\n\
            pub struct Built<I: ::organon_theoria::Declaration> { values: I::Outputs }\n",
        );
        code.push_str("impl<I: ::organon_theoria::Declaration> Clone for Built<I> where I::Outputs: Clone { fn clone(&self) -> Self { Self { values: self.values.clone() } } }\n");
        code.push_str("impl<I: ::organon_theoria::Declaration> ::std::fmt::Debug for Built<I> where I::Outputs: ::std::fmt::Debug { fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result { self.values.fmt(f) } }\n");
    }
    for (index, definition) in file.definitions().enumerate() {
        let name = definition.name();
        if !crate::api::identifier(name)
            || name.starts_with("__")
            || matches!(name, "Input" | "Builder" | "Built")
        {
            return Err(Error(format!(
                "definition name {name:?} conflicts with generated Rust vocabulary"
            )));
        }
        if let Some(provider) = libraries.get(name) {
            writeln!(code, "pub use {provider}::r#{name};").unwrap();
            continue;
        }
        match definition {
            Definition::Interface(value) => generate_contract(value, &mut code)?,
            Definition::External(value) => generate(value, &mut code)?,
            Definition::Element(value) => {
                writeln!(
                    code,
                    "mod __element_{index} {{\n{}\n}}\npub use __element_{index}::r#{name};",
                    generated[&value.name]
                )
                .unwrap();
            }
        }
    }
    Ok(code)
}

fn generate(value: &External, code: &mut String) -> Result<(), Error> {
    let name = &value.name;
    let mut methods = HashSet::new();
    for input in value.inputs() {
        if !crate::api::identifier(&input.name)
            || matches!(input.name.as_str(), "build" | "build_in")
        {
            return Err(Error(format!(
                "invalid or reserved input name {:?}",
                input.name
            )));
        }
    }
    for output in value.outputs() {
        let field = &output.port.name;
        if !crate::api::identifier(field) {
            return Err(Error(format!("invalid output name {field:?}")));
        }
        for method in [field.clone(), format!("{field}_fixed")] {
            if !methods.insert(method.clone()) {
                return Err(Error(format!("ambiguous output method {method:?}")));
            }
        }
    }
    writeln!(code, "pub struct r#{name};").unwrap();
    let definition = tuple(
        value
            .outputs()
            .map(|v| format!("::organon_theoria::Calculation<{}>", ty(&v.port.ty))),
    );
    writeln!(code, "impl ::organon_theoria::Declaration for r#{name} {{\n\
        type Definition = {definition};\n\
        type Inputs = {};\n\
        type Input = Input<Self>;\n\
        type Arguments = {};\n\
        type Outputs = {};\n\
        fn bind(definition: Self::Definition, context: &mut impl ::organon_theoria::BindingContext) -> ::std::result::Result<Self::Outputs, ::theoria_reactive::ReactiveError> {{\n\
        ::std::result::Result::Ok({})\n}}\n}}",
        tuple(value.inputs().map(|v| format!("::organon_theoria::Source<{}>", ty(&v.ty)))),
        tuple(value.inputs().map(|v| format!("::std::option::Option<::organon_theoria::Source<{}>>", ty(&v.ty)))),
        tuple(value.outputs().map(|v| format!("::organon_theoria::Source<{}>", ty(&v.port.ty)))),
        tuple(value.outputs().enumerate().map(|(i,v)| format!("definition.{i}.bind_in(context, {})?",v.inline))),
    ).unwrap();
    writeln!(
        code,
        "impl r#{name} {{ pub fn builder() -> Builder<Self> {{ Builder {{ values: {} }} }} }}",
        tuple(value.inputs().map(|_| "::std::option::Option::None".into()))
    )
    .unwrap();
    writeln!(code, "impl Input<r#{name}> {{").unwrap();
    for (index, port) in value.inputs().enumerate() {
        writeln!(
            code,
            "pub fn r#{}(&self) -> &::organon_theoria::Source<{}> {{ &self.values.{index} }}",
            port.name,
            ty(&port.ty)
        )
        .unwrap();
    }
    writeln!(code, "}}\nimpl Builder<r#{name}> {{").unwrap();
    for (index, port) in value.inputs().enumerate() {
        writeln!(code, "pub fn r#{}(mut self, value: impl ::std::convert::Into<::organon_theoria::Source<{}>>) -> Self {{ self.values.{index} = ::std::option::Option::Some(value.into()); self }}", port.name, ty(&port.ty)).unwrap();
    }
    writeln!(code, "pub fn build(self, reactive: &::theoria_reactive::Reactive) -> ::std::result::Result<Built<r#{name}>, ::organon_theoria::BuildError> {{\n\
        self.build_in(&mut &*reactive) }}\n\
        pub fn build_in(self, context: &mut impl ::organon_theoria::BindingContext) -> ::std::result::Result<Built<r#{name}>, ::organon_theoria::BuildError> {{\n\
        let input = Input {{ values: {} }};\n\
        let definition = ::organon_theoria::Implementation::build(&Impl_{name}, input);\n\
        ::std::result::Result::Ok(Built {{ values: <r#{name} as ::organon_theoria::Declaration>::bind(definition, context)? }})\n}}\n}}",
        tuple(value.inputs().enumerate().map(|(index, v)| format!("self.values.{index}.ok_or(::organon_theoria::BuildError::MissingInput({:?}))?", v.name))),
    ).unwrap();
    writeln!(code, "impl Built<r#{name}> {{").unwrap();
    for (index, output) in value.outputs().enumerate() {
        let field = &output.port.name;
        let ty = ty(&output.port.ty);
        writeln!(code, "pub fn r#{field}(&self) -> ::organon_theoria::Source<{ty}> {{ self.values.{index}.clone() }}\n\
            pub fn r#{field}_fixed(&self) -> ::std::option::Option<&{ty}> {{ self.values.{index}.as_fixed() }}").unwrap();
    }
    code.push_str("}\n");
    for (index, contract) in value.implements.iter().enumerate() {
        writeln!(code, "fn __check_{name}_{index}(value: &Built<r#{name}>) -> r#{contract} {{ ::organon_theoria::Provide::provide(value) }}").unwrap();
    }
    Ok(())
}

fn generate_contract(value: &crate::ir::Interface, code: &mut String) -> Result<(), Error> {
    let name = &value.name;
    let mut methods = HashSet::new();
    writeln!(code, "#[derive(Clone, Debug)] pub struct r#{name} {{").unwrap();
    for output in &value.outputs {
        let field = &output.name;
        if !crate::api::identifier(field) {
            return Err(Error(format!("invalid output name {field:?}")));
        }
        for method in [field.clone(), format!("{field}_fixed")] {
            if !methods.insert(method) {
                return Err(Error("ambiguous interface output methods".into()));
            }
        }
        writeln!(
            code,
            "pub r#{field}: ::organon_theoria::Source<{}>,",
            ty(&output.ty)
        )
        .unwrap();
    }
    writeln!(code, "}} impl r#{name} {{").unwrap();
    for output in &value.outputs {
        let field = &output.name;
        let ty = ty(&output.ty);
        writeln!(code, "pub fn r#{field}(&self) -> ::organon_theoria::Source<{ty}> {{ self.r#{field}.clone() }}
            pub fn r#{field}_fixed(&self) -> Option<&{ty}> {{ self.r#{field}.as_fixed() }}").unwrap();
    }
    writeln!(code, "}} impl ::organon_theoria::Provide<r#{name}> for r#{name} {{ fn provide(&self) -> r#{name} {{ self.clone() }} }}").unwrap();
    Ok(())
}
