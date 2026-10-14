use crate::Error;
use crate::instance::Signature;
use crate::types::{Input, Type};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

pub(crate) fn generate_resolved(
    resolved: &crate::ir::Element,
    signatures: &HashMap<String, Signature>,
) -> Result<String, Error> {
    let definition = resolved;
    crate::api::validate(definition)?;
    let definitions = &resolved.bindings;
    for name in definition
        .inputs()
        .map(|v| v.name.as_str())
        .chain(definitions.iter().map(|v| v.name.as_str()))
    {
        if !crate::api::identifier(name) {
            return Err(Error(format!("invalid Rust binding {name:?}")));
        }
    }
    let mut bindings = definition.inputs().cloned().collect::<Vec<_>>();
    let mut plans: Vec<crate::bind::Binding> = Vec::new();
    let used: HashSet<_> = definition
        .inputs()
        .map(|i| i.name.clone())
        .chain(definitions.iter().map(|d| d.name.clone()))
        .collect();
    let mut projections = crate::instance::Projections::new(used);
    for definition in definitions {
        use crate::ir::Expression;
        if let Expression::Projection {
            parent,
            output: field,
            output_type,
            parent_fallible,
        } = &definition.value
        {
            let name = &definition.name;
            let selected = projections.emit(
                parent,
                field,
                output_type,
                *parent_fallible,
                &mut bindings,
                &mut plans,
            );
            let source = crate::bind::source(&selected, &plans);
            plans.push(crate::bind::Binding {
                name: name.clone(),
                provided: false,
                inline: false,
                kind: definition.ty.kind.clone(),
                fallible: definition.ty.fallible,
                expression: String::new(),
                references: vec![selected.clone()],
                constructed: Some(crate::bind::Construction::Source(source)),
            });
            bindings.push(Input {
                name: name.clone(),
                kind: definition.ty.kind.clone(),
            });
            continue;
        }
        if let Expression::Instance { element, arguments } = &definition.value {
            let signature = &signatures[element];
            crate::instance::construct(
                &definition.name,
                element,
                signature,
                arguments,
                &mut bindings,
                &mut plans,
            )?;
            continue;
        }
        let resolved_binding = definition;
        let kind = resolved_binding.ty.kind.clone();
        let expression = match &resolved_binding.value {
            crate::ir::Expression::Native { expression } => expression.clone(),
            crate::ir::Expression::Instance { .. } | crate::ir::Expression::Projection { .. } => {
                unreachable!()
            }
            crate::ir::Expression::Reference(name) => format!("binding_{name}.clone()"),
        };
        let references = resolved_binding.references.clone();
        let fallible = resolved_binding.ty.fallible;
        plans.push(crate::bind::Binding {
            name: definition.name.clone(),
            provided: false,
            inline: definition.inline,
            kind: kind.clone(),
            expression: expression.clone(),
            references,
            fallible,
            constructed: None,
        });
        let binding = Input {
            name: definition.name.clone(),
            kind: kind.clone(),
        };
        bindings.push(binding);
    }
    for plan in &mut plans {
        plan.provided = definition.outputs().any(|e| e.binding == plan.name)
            || definition
                .implementations
                .iter()
                .flat_map(|i| &i.outputs)
                .any(|e| e.binding == plan.name);
    }
    let mut outputs: Vec<_> = definition
        .outputs()
        .map(|e| crate::bind::OutputBinding {
            name: e.public_name().to_owned(),
            binding: e.binding.clone(),
            kind: e.ty.kind.clone(),
            fallible: e.ty.fallible,
        })
        .collect();
    let mut contract_fields = Vec::new();
    for (i, implementation) in definition.implementations.iter().enumerate() {
        let mut fields = Vec::new();
        for (j, value) in implementation.outputs.iter().enumerate() {
            let mut name = format!("__organon_interface_{i}_{j}");
            while outputs.iter().any(|e| e.name == name) {
                name.push('_');
            }
            outputs.push(crate::bind::OutputBinding {
                name: name.clone(),
                binding: value.binding.clone(),
                kind: value.ty.kind.clone(),
                fallible: value.ty.fallible,
            });
            fields.push((value.name.clone(), name));
        }
        contract_fields.push((&implementation.interface, fields));
    }
    let mut output = String::from("// Generated by Organon. Edit the .organon.xml source.\n");
    crate::bind::generate(
        &definition.inputs().cloned().collect::<Vec<_>>(),
        &plans,
        &outputs,
        &mut output,
    );
    crate::api::generate(definition, &outputs, &mut output);
    for (contract, fields) in contract_fields {
        writeln!(output, "impl ::organon_theoria::Provide<super::r#{contract}> for r#{} {{ fn provide(&self) -> super::r#{contract} {{ super::r#{contract} {{", definition.name).unwrap();
        for (field, storage) in fields {
            writeln!(output, "r#{field}: self.r#{storage}.clone(),").unwrap();
        }
        output.push_str("} } }\n");
    }
    Ok(output)
}

pub(crate) fn type_name(kind: &Type) -> String {
    let Type::Element(name) = kind;
    format!("super::__organon_values::r#{name}")
}
