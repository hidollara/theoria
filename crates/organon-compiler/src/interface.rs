//! Contracts use tuple storage and local generic wrappers, not named input structs.
use crate::{Error, element::type_name, ir};
use organon_format::{Definition, DefinitionFile, InterfaceDefinition, PortType};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt::Write,
};

fn ty(value: &PortType) -> String {
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

pub fn generate_file_rust(file: &DefinitionFile) -> Result<String, Error> {
    generate_file_with_implementations(file, &BTreeMap::new())
}

/// Implementation expressions are trusted Rust supplied by the host, not XML.
pub fn generate_file_with_implementations(
    file: &DefinitionFile,
    implementations: &BTreeMap<String, String>,
) -> Result<String, Error> {
    if !file.imports.is_empty() {
        return Err(Error(
            "unresolved imports: use load_definitions with an entry file path".into(),
        ));
    }
    // Also validate manually constructed format values and duplicate definitions.
    file.to_xml().map_err(|e| Error(e.to_string()))?;
    let mut file = file.clone();
    for definition in &mut file.definitions {
        if let Definition::Element(element) = definition {
            *element = crate::selection::normalize(element)?;
        }
    }
    let mut signatures = HashMap::new();
    let mut generated = HashMap::new();
    for name in implementations.keys() {
        if !file
            .definitions
            .iter()
            .any(|d| matches!(d,Definition::Interface(i) if &i.name == name))
        {
            return Err(Error(format!(
                "implementation binding {name:?} does not name an interface"
            )));
        }
    }
    for definition in &file.definitions {
        if let Definition::Interface(value) = definition {
            signatures.insert(
                value.name.clone(),
                crate::instance::Signature {
                    inputs: value.inputs.clone(),
                    outputs: value.outputs.iter().map(|o| o.port.clone()).collect(),
                    interface: true,
                    implemented: implementations.contains_key(&value.name),
                },
            );
        }
    }
    let mut pending: Vec<_> = file
        .definitions
        .iter()
        .filter_map(|d| {
            if let Definition::Element(v) = d {
                Some(v)
            } else {
                None
            }
        })
        .collect();
    while !pending.is_empty() {
        let before = pending.len();
        let mut later = vec![];
        for value in pending {
            let mut targets: Vec<_> = value
                .definitions
                .iter()
                .filter_map(|d| {
                    if let organon_format::ValueExpr::Instance(i) = &d.value {
                        Some(&i.element)
                    } else {
                        None
                    }
                })
                .collect();
            targets.extend(value.inputs.iter().filter_map(|i| {
                if let organon_format::InputType::Element(name) = &i.kind {
                    Some(name)
                } else {
                    None
                }
            }));
            // Interfaces may expose XML-defined Elements. Their signatures are
            // known already, but selecting those outputs also needs their types.
            let mut cursor = 0;
            while cursor < targets.len() {
                if let Some(Definition::Interface(interface)) = file
                    .definitions
                    .iter()
                    .find(|d| d.name() == targets[cursor])
                {
                    for port in interface
                        .inputs
                        .iter()
                        .chain(interface.outputs.iter().map(|o| &o.port))
                    {
                        if let organon_format::InputType::Element(name) = &port.ty.kind
                            && !targets.contains(&name)
                        {
                            targets.push(name);
                        }
                    }
                }
                cursor += 1;
            }
            for target in &targets {
                if !file.definitions.iter().any(|d| d.name() == target.as_str()) {
                    return Err(Error(format!("{}: unknown element {target:?}", value.name)));
                }
            }
            if targets.iter().any(|n| !signatures.contains_key(*n)) {
                later.push(value);
                continue;
            }
            let (code, outputs) = crate::element::generate(value, &signatures)
                .map_err(|e| Error(format!("{}: {e}", value.name)))?;
            signatures.insert(
                value.name.clone(),
                crate::instance::Signature {
                    inputs: value
                        .inputs
                        .iter()
                        .map(|i| organon_format::Port {
                            name: i.name.clone(),
                            ty: PortType {
                                kind: i.kind.clone(),
                                fallible: false,
                            },
                        })
                        .collect(),
                    outputs,
                    interface: false,
                    implemented: true,
                },
            );
            generated.insert(value.name.clone(), code);
        }
        if later.len() == before {
            return Err(Error(format!(
                "cyclic element construction or type dependency: {}",
                later
                    .iter()
                    .map(|v| v.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        pending = later;
    }
    let mut code = String::from(
        "#[doc(hidden)]\n#[allow(non_camel_case_types)]\npub mod __organon_values {\n",
    );
    for definition in &file.definitions {
        let name = definition.name();
        let target = if matches!(definition, Definition::Interface(_)) {
            format!("super::Built<super::r#{name}>")
        } else {
            format!("super::r#{name}")
        };
        writeln!(code, "pub type r#{name} = {target};").unwrap();
        if let Definition::Interface(interface) = definition {
            for port in interface
                .inputs
                .iter()
                .chain(interface.outputs.iter().map(|o| &o.port))
            {
                if let organon_format::InputType::Element(name) = &port.ty.kind
                    && !file.definitions.iter().any(|d| d.name() == name)
                {
                    return Err(Error(format!("unknown element type {name:?}")));
                }
            }
        }
    }
    code.push_str("}\n");
    if file
        .definitions
        .iter()
        .any(|v| matches!(v, Definition::Interface(_)))
    {
        code.push_str(
            "pub struct Input<I: ::organon::Interface> { values: I::Inputs }\n\
            pub struct Builder<I: ::organon::Interface> { values: I::Arguments }\n\
            pub struct Built<I: ::organon::Interface> { values: I::Outputs }\n",
        );
        code.push_str("impl<I: ::organon::Interface> Clone for Built<I> where I::Outputs: Clone { fn clone(&self) -> Self { Self { values: self.values.clone() } } }\n");
        code.push_str("impl<I: ::organon::Interface> ::std::fmt::Debug for Built<I> where I::Outputs: ::std::fmt::Debug { fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result { self.values.fmt(f) } }\n");
    }
    for (index, definition) in file.definitions.iter().enumerate() {
        let name = definition.name();
        if !ir::identifier(name)
            || name.starts_with("__")
            || matches!(name, "Input" | "Builder" | "Built")
        {
            return Err(Error(format!(
                "definition name {name:?} conflicts with generated Rust vocabulary"
            )));
        }
        match definition {
            Definition::Interface(value) => generate(value, &mut code)?,
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
    for (name, expression) in implementations {
        writeln!(code,"fn __organon_impl_{name}() -> impl ::organon::Implementation<r#{name}> {{ {expression} }}").unwrap();
    }
    Ok(code)
}

fn generate(value: &InterfaceDefinition, code: &mut String) -> Result<(), Error> {
    let name = &value.name;
    let mut methods = HashSet::new();
    for input in &value.inputs {
        if !ir::identifier(&input.name) || matches!(input.name.as_str(), "build" | "build_in") {
            return Err(Error(format!(
                "invalid or reserved input name {:?}",
                input.name
            )));
        }
    }
    for output in &value.outputs {
        let field = &output.port.name;
        if !ir::identifier(field) {
            return Err(Error(format!("invalid output name {field:?}")));
        }
        for method in [
            field.clone(),
            format!("{field}_fixed"),
            format!("{field}_source"),
        ] {
            if !methods.insert(method.clone()) {
                return Err(Error(format!("ambiguous output method {method:?}")));
            }
        }
    }
    writeln!(code, "pub struct r#{name} {{").unwrap();
    for output in &value.outputs {
        writeln!(
            code,
            "pub r#{}: ::organon::Calculation<{}>,",
            output.port.name,
            ty(&output.port.ty)
        )
        .unwrap();
    }
    code.push_str("}\n");
    writeln!(code, "impl ::organon::Interface for r#{name} {{\n\
        type Inputs = {};\n\
        type Input = Input<Self>;\n\
        type Arguments = {};\n\
        type Outputs = {};\n\
        fn bind(self, context: &mut impl ::organon::BindingContext) -> ::std::result::Result<Self::Outputs, ::theoria_reactive::ReactiveError> {{\n\
        ::std::result::Result::Ok({})\n}}\n}}",
        tuple(value.inputs.iter().map(|v| format!("::organon::Source<{}>", ty(&v.ty)))),
        tuple(value.inputs.iter().map(|v| format!("::std::option::Option<::organon::Source<{}>>", ty(&v.ty)))),
        tuple(value.outputs.iter().map(|v| format!("::organon::Bound<{}>", ty(&v.port.ty)))),
        tuple(value.outputs.iter().map(|v| format!("self.r#{}.bind_in(context, {})?", v.port.name, v.inline))),
    ).unwrap();
    writeln!(
        code,
        "impl r#{name} {{ pub fn builder() -> Builder<Self> {{ Builder {{ values: {} }} }} }}",
        tuple(
            value
                .inputs
                .iter()
                .map(|_| "::std::option::Option::None".into())
        )
    )
    .unwrap();
    writeln!(code, "impl Input<r#{name}> {{").unwrap();
    for (index, port) in value.inputs.iter().enumerate() {
        writeln!(
            code,
            "pub fn r#{}(&self) -> &::organon::Source<{}> {{ &self.values.{index} }}",
            port.name,
            ty(&port.ty)
        )
        .unwrap();
    }
    writeln!(code, "}}\nimpl Builder<r#{name}> {{").unwrap();
    for (index, port) in value.inputs.iter().enumerate() {
        writeln!(code, "pub fn r#{}(mut self, value: impl ::std::convert::Into<::organon::Source<{}>>) -> Self {{ self.values.{index} = ::std::option::Option::Some(value.into()); self }}", port.name, ty(&port.ty)).unwrap();
    }
    writeln!(code, "pub fn build(self, reactive: &::theoria_reactive::Reactive, implementation: &impl ::organon::Implementation<r#{name}>) -> ::std::result::Result<Built<r#{name}>, ::organon::BuildError> {{\n\
        self.build_in(&mut &*reactive, implementation) }}\n\
        pub fn build_in(self, context: &mut impl ::organon::BindingContext, implementation: &impl ::organon::Implementation<r#{name}>) -> ::std::result::Result<Built<r#{name}>, ::organon::BuildError> {{\n\
        let input = Input {{ values: {} }};\n\
        let definition = implementation.build(input);\n\
        ::std::result::Result::Ok(Built {{ values: <r#{name} as ::organon::Interface>::bind(definition, context)? }})\n}}\n}}",
        tuple(value.inputs.iter().enumerate().map(|(index, v)| format!("self.values.{index}.ok_or(::organon::BuildError::MissingInput({:?}))?", v.name))),
    ).unwrap();
    writeln!(code, "impl Built<r#{name}> {{").unwrap();
    for (index, output) in value.outputs.iter().enumerate() {
        let field = &output.port.name;
        let ty = ty(&output.port.ty);
        writeln!(code, "pub fn r#{field}(&self) -> ::theoria_reactive::Value<{ty}> {{ self.values.{index}.value() }}\n\
            pub fn r#{field}_fixed(&self) -> ::std::option::Option<&{ty}> {{ self.values.{index}.as_fixed() }}\n\
            pub fn r#{field}_source(&self) -> ::organon::Source<{ty}> {{ self.values.{index}.source() }}").unwrap();
    }
    code.push_str("}\n");
    Ok(())
}
