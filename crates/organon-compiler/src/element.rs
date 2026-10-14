use crate::instance::Signature;
use crate::{Error, generate, ir, lower};
use organon_format::{ElementDefinition, Input, InputType, Template, ValueExpr};
use organon_format::{Port, PortType};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

/// Generates a named element, a consuming Builder, and native output accessors.
pub fn generate_element_rust(definition: &ElementDefinition) -> Result<String, Error> {
    crate::generate_loaded_file(&organon_format::DefinitionFile {
        imports: vec![],
        definitions: vec![organon_format::Definition::Element(definition.clone())],
    })
}

pub(crate) fn generate(
    definition: &ElementDefinition,
    signatures: &HashMap<String, Signature>,
) -> Result<(String, Vec<Port>), Error> {
    crate::api::validate(definition)?;
    let definitions = definition
        .ordered_bindings()
        .map_err(|e| Error(e.to_string()))?;
    for name in definition
        .inputs
        .iter()
        .map(|v| &v.name)
        .chain(definitions.iter().map(|v| &v.name))
    {
        if !ir::identifier(name) {
            return Err(Error(format!("invalid Rust binding {name:?}")));
        }
    }
    let mut bindings = definition.inputs.clone();
    let mut body = String::from("let _ = inputs;\n");
    for input in &bindings {
        writeln!(
            body,
            "let binding_{} = &inputs.r#{};\nlet _ = binding_{};",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    let mut exports = Vec::new();
    let mut plans: Vec<crate::bind::Binding> = Vec::new();
    let used: HashSet<_> = definition
        .inputs
        .iter()
        .map(|i| i.name.clone())
        .chain(definitions.iter().map(|d| d.name.clone()))
        .collect();
    let mut projections = crate::instance::Projections::new(used);
    let has_calls = definition
        .inputs
        .iter()
        .any(|i| matches!(i.kind, InputType::Element(_)))
        || definitions
            .iter()
            .any(|d| matches!(d.value, ValueExpr::Instance(_) | ValueExpr::Get(_)));
    let mut next = 0;
    for (index, authored) in definitions.iter().enumerate() {
        let mut definition = authored.clone();
        crate::references::rewrite(&mut definition.value, &mut |path| {
            projections.resolve(path, signatures, &mut bindings, &mut plans)
        })?;
        if let ValueExpr::Get(get) = &definition.value {
            let ValueExpr::Reference(target) = &*get.target else {
                unreachable!()
            };
            let template = Template::parse(target).map_err(|e| Error(e.to_string()))?;
            let path = format!("{}.{}", template.reference().unwrap(), get.name);
            let resolved = projections.resolve(&path, signatures, &mut bindings, &mut plans)?;
            definition.value = ValueExpr::Reference(format!("{{{resolved}}}"));
        }
        if let ValueExpr::Instance(instance) = &definition.value {
            let signature = signatures
                .get(&instance.element)
                .ok_or_else(|| Error(format!("unknown element {:?}", instance.element)))?;
            crate::instance::construct(
                &definition,
                instance,
                signature,
                &mut bindings,
                &mut plans,
            )?;
            if definition.export {
                exports.push(bindings.last().unwrap().clone());
            }
            continue;
        }
        let start = body.len();
        let (kind, expression, own_fallibility) = match &definition.value {
            ValueExpr::Instance(_) | ValueExpr::Get(_) => unreachable!(),
            ValueExpr::Reference(reference) => {
                let parsed = Template::parse(reference).map_err(|e| Error(e.to_string()))?;
                let name = parsed.reference().ok_or_else(|| {
                    Error("let reference requires a single {name} reference".into())
                })?;
                let kind = bindings
                    .iter()
                    .find(|b| b.name == name)
                    .ok_or_else(|| Error(format!("unknown reference {name:?}")))?
                    .kind
                    .clone();
                if kind == InputType::Path {
                    writeln!(body, "if !binding_{name}.is_finite() {{ return Err(\"Path coordinates must be finite\"); }}").unwrap();
                }
                (
                    kind.clone(),
                    binding_value(name, &kind),
                    kind == InputType::Path,
                )
            }
            ValueExpr::Path(path) => {
                let path = lower::lower_path(path, &bindings)?;
                let typed = lower::lower_inputs(&bindings);
                path.validate(&typed.iter().map(|v| (v.name.as_str(), v.kind)).collect())?;
                (InputType::Path, generate::generate_path(&path), true)
            }
            ValueExpr::Canvas(node) => {
                let canvas = lower::lower(node, &bindings)?;
                canvas.validate()?;
                (
                    InputType::Canvas,
                    generate::generate_node(&canvas.root, &mut next, &mut body),
                    canvas.root.is_fallible(),
                )
            }
        };
        let references = definition.references().map_err(|e| Error(e.to_string()))?;
        let fallible = own_fallibility
            || plans
                .iter()
                .any(|p| p.fallible && references.contains(&p.name));
        plans.push(crate::bind::Binding {
            name: definition.name.clone(),
            export: definition.export,
            inline: definition.inline,
            kind: kind.clone(),
            statements: body[start..].to_owned(),
            expression: expression.clone(),
            references,
            fallible,
            constructed: None,
        });
        writeln!(body, "let value_{index} = {expression};\nlet binding_{} = &value_{index};\nlet _ = binding_{};", definition.name, definition.name).unwrap();
        let binding = Input {
            name: definition.name.clone(),
            kind: kind.clone(),
        };
        if definition.export {
            exports.push(binding.clone());
        }
        bindings.push(binding);
    }
    let mut output = String::from("// Generated by Organon. Edit the .organon.xml source.\n");
    if !has_calls {
        write_struct("Inputs", &definition.inputs, &mut output);
        write_struct("Outputs", &exports, &mut output);
        output.push_str("#[cfg(test)]\n#[allow(dead_code)]\npub(crate) fn build(inputs: &Inputs) -> Result<Outputs, &'static str> {\n");
        output.push_str(&body);
        let fields: Vec<_> = exports
            .iter()
            .map(|b| format!("r#{}: {}", b.name, binding_value(&b.name, &b.kind)))
            .collect();
        writeln!(output, "Ok(Outputs {{ {} }})\n}}", fields.join(", ")).unwrap();
    }
    crate::bind::generate(&definition.inputs, &plans, &exports, &mut output);
    crate::api::generate(definition, &plans, &exports, &mut output);
    let ports = exports
        .iter()
        .map(|e| Port {
            name: e.name.clone(),
            ty: PortType {
                kind: e.kind.clone(),
                fallible: plans.iter().find(|p| p.name == e.name).unwrap().fallible,
            },
        })
        .collect();
    Ok((output, ports))
}

fn binding_value(name: &str, kind: &InputType) -> String {
    if *kind == InputType::Scalar {
        format!("*binding_{name}")
    } else {
        format!("binding_{name}.clone()")
    }
}

fn write_struct(name: &str, bindings: &[Input], output: &mut String) {
    writeln!(
        output,
        "#[cfg(test)]\n#[allow(dead_code)]\n#[derive(Clone, Debug)]\npub(crate) struct {name} {{"
    )
    .unwrap();
    for binding in bindings {
        let ty = type_name(&binding.kind);
        writeln!(output, "pub r#{}: {ty},", binding.name).unwrap();
    }
    output.push_str("}\n");
}

pub(crate) fn type_name(kind: &InputType) -> String {
    match kind {
        InputType::Element(name) => return format!("super::__organon_values::r#{name}"),
        InputType::Scalar => "f32",
        InputType::String => "::std::string::String",
        InputType::Color => "::theoria_canvas::Color",
        InputType::ColorField => "::theoria_canvas::ColorField",
        InputType::Canvas => "::theoria_canvas::Canvas",
        InputType::Path => "::theoria_canvas::path::Path",
        InputType::Fonts => "::std::vec::Vec<::theoria_text::font::Font>",
    }
    .into()
}
