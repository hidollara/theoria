//! Generate calls to library contracts, without matching primitive names.
use crate::{Error, ir as t, types as ty};
use organon_core::types as core_types;
use organon_core::{format as f, loader, semantic as s};
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
};
pub(crate) fn compile(
    loaded: &loader::Loaded,
    program: &s::Program,
    consumer: &Path,
    providers: BTreeMap<PathBuf, String>,
) -> Result<String, Error> {
    let context = Context {
        loaded,
        signatures: &program.signatures,
        providers,
    };
    let mut definitions = vec![];
    let mut aliases = String::new();
    let mut prefix = String::new();
    prefix.push_str("fn __organon_registry()->::organon_theoria::attribute::Registry {let mut registry=::organon_theoria::attribute::Registry::default();");
    for path in loaded
        .file
        .definitions()
        .filter(|d| matches!(d, f::Definition::External(e) if !e.tag))
        .map(|d| &context.providers[&loaded.origins[d.name()].file])
        .collect::<std::collections::BTreeSet<_>>()
    {
        write!(
            prefix,
            "registry.extend({path}::__organon::attribute_types());"
        )
        .unwrap();
    }
    for (name, signature) in &program.signatures {
        if !signature.constructible() {
            continue;
        }
        let ty = format!("__organon_values::r#{name}");
        let mut interface_types = Vec::new();
        let mut provide = String::from("|v,interface|{let _=v;match interface{");
        for interface in &signature.implements {
            let id = &loaded.origins[interface];
            let identity = organon_cargo::identity(&id.file, &id.name)?;
            let target = organon_cargo::declaration_path(id, &context.providers[&id.file])?;
            interface_types.push(format!("std::any::TypeId::of::<{target}>()"));
            write!(provide,"{identity:?}=>Ok(::organon_theoria::attribute::Value::new(::organon_theoria::Provide::<{target}>::provide(&v.get::<{ty}>()?))),").unwrap();
        }
        provide.push_str("_=>Err(\"interface not provided\".into()),}}");
        let interfaces = interface_types.join(",");
        writeln!(prefix,"registry.types.insert(concat!(\"__local_\",{name:?}).into(),::organon_theoria::attribute::TypeEntry{{interfaces:vec![{interfaces}],name:{name:?},rust_type:std::any::TypeId::of::<{ty}>,provide:{provide}}});").unwrap();
    }
    prefix.push_str("registry}\n");
    let mut libraries = std::collections::HashMap::new();
    for (name, signature) in &program.signatures {
        if program.elements.iter().any(|e| e.name == *name) {
            continue;
        }
        if signature.kind == (s::DefinitionKind::External { tag: false }) {
            continue;
        }
        let mut inputs: Vec<_> = signature.inputs.iter().map(port).collect();
        for input in &mut inputs {
            if signature.optional_inputs.contains(&input.name) {
                let alias = format!("__optional_{name}_{}", input.name);
                writeln!(
                    aliases,
                    "pub type {alias}=Option<{}>;",
                    crate::element::type_name(&input.ty.kind)
                        .trim_start_matches("super::__organon_values::")
                )
                .unwrap();
                input.ty = ty::ValueType {
                    kind: ty::Type::Element(alias),
                    fallible: false,
                };
            }
        }
        if let Some(c) = &signature.children {
            let alias = format!("__children_{name}");
            writeln!(aliases, "pub type {alias}=Vec<r#{}>;", c.kind.spelling()).unwrap();
            inputs.push(ty::Port {
                name: "__children".into(),
                ty: ty::ValueType {
                    kind: ty::Type::Element(alias),
                    fallible: false,
                },
            });
        }
        definitions.push(if signature.constructible() {
            t::Definition::External(t::External {
                name: name.clone(),
                implements: signature.implements.clone(),
                inputs,
                outputs: signature
                    .outputs
                    .iter()
                    .map(|p| t::DeclaredOutput {
                        port: port(p),
                        inline: signature.inline_outputs.contains(&p.name),
                    })
                    .collect(),
            })
        } else {
            t::Definition::Interface(t::Interface {
                name: name.clone(),
                outputs: signature.outputs.iter().map(port).collect(),
            })
        });
        let id = &loaded.origins[name];
        {
            let symbol = organon_cargo::identity(&id.file, &id.name)?;
            let provider = &context.providers[&id.file];
            let module = format!("__provider_{name}");
            let built = if signature.constructible() {
                "Built,"
            } else {
                ""
            };
            writeln!(
                prefix,
                "mod {module} {{pub use {provider}::__organon::{{{built}{symbol} as r#{name}}};}}"
            )
            .unwrap();
            libraries.insert(name.clone(), module);
        }
    }
    for element in &program.elements {
        let id = &loaded.origins[&element.name];
        if context.providers.contains_key(&id.file)
            && organon_cargo::manifest(&id.file)?
                != std::fs::canonicalize(consumer).map_err(|e| Error(e.to_string()))?
        {
            let name = &element.name;
            let symbol = organon_cargo::identity(&id.file, &id.name)?;
            let provider = &context.providers[&id.file];
            let module = format!("__provider_{name}");
            writeln!(
                prefix,
                "mod {module} {{pub use {provider}::__organon::{symbol} as r#{name};}}"
            )
            .unwrap();
            libraries.insert(name.clone(), module);
        }
    }
    let mut order = vec![];
    for element in &program.elements {
        let mut lower = Lower {
            context: &context,
            bindings: vec![],
            types: element
                .inputs
                .iter()
                .map(|i| {
                    (
                        i.name.clone(),
                        core_types::ValueType {
                            kind: i.kind.clone(),
                            fallible: false,
                        },
                    )
                })
                .collect(),
            next: 0,
        };
        for b in &element.bindings {
            lower.binding(&b.name, &b.value, b.inline)?;
        }
        order.push(definitions.len());
        definitions.push(t::Definition::Element(t::Element {
            name: element.name.clone(),
            inputs: element
                .inputs
                .iter()
                .map(|i| ty::Input {
                    name: i.name.clone(),
                    kind: kind(&i.kind),
                })
                .collect(),
            bindings: lower.bindings,
            outputs: element.outputs.iter().map(lower_output).collect(),
            implementations: element
                .implementations
                .iter()
                .map(|i| t::Implementation {
                    interface: i.interface.clone(),
                    outputs: i.outputs.iter().map(lower_output).collect(),
                })
                .collect(),
        }));
    }
    for d in loaded.file.definitions() {
        if let f::Definition::External(n) = d
            && !n.tag
        {
            writeln!(
                aliases,
                "pub type r#{}=<{} as ::organon_theoria::Type>::Value;",
                n.name,
                context.marker(&n.name)?
            )
            .unwrap();
        }
    }
    let program = t::Program {
        definitions,
        construction_order: order,
    };
    let code = crate::definition::generate_program(&program, &libraries)?;
    let code = format!(
        "{prefix}{}",
        code.replacen(
            "pub mod __organon_values {\n",
            &format!("pub mod __organon_values {{\n{aliases}"),
            1
        )
    );
    Ok(code)
}
struct Lower<'a, 'b> {
    context: &'a Context<'b>,
    bindings: Vec<t::Binding>,
    types: BTreeMap<String, core_types::ValueType>,
    next: usize,
}
impl Lower<'_, '_> {
    fn temporary(&mut self) -> String {
        let n = format!("__organon_child_{}", self.next);
        self.next += 1;
        n
    }
    fn binding(
        &mut self,
        name: &str,
        expression: &s::Expression,
        inline: bool,
    ) -> Result<(), Error> {
        let value = match &expression.kind {
            s::ExpressionKind::Reference(n) => t::Expression::Reference(n.clone()),
            s::ExpressionKind::Projection {
                parent,
                output,
                output_type,
                parent_fallible,
            } => t::Expression::Projection {
                parent: parent.clone(),
                output: output.clone(),
                output_type: value_type(output_type),
                parent_fallible: *parent_fallible,
            },
            s::ExpressionKind::Construct {
                name: element,
                arguments,
                children,
            } => {
                let signature = &self.context.signatures[element];
                let mut args = BTreeMap::new();
                for (field, arg) in arguments {
                    let argument = match arg {
                        s::Argument::Attribute { text, expected } => t::Argument::Interpreted {
                            marker: self.context.marker(expected.kind.spelling())?,
                            text: text.clone(),
                            optional: signature.optional_inputs.contains(field),
                        },
                        s::Argument::Reference { name, expected }
                            if !signature.optional_inputs.contains(field) =>
                        {
                            let actual = &self.types[name];
                            if actual.kind != expected.kind
                                && matches!(
                                    self.context.signatures[expected.kind.spelling()].kind,
                                    s::DefinitionKind::Interface
                                )
                            {
                                t::Argument::Coerce {
                                    name: name.clone(),
                                    interface: expected.kind.spelling().into(),
                                    source_fallible: actual.fallible,
                                    target_fallible: expected.fallible,
                                }
                            } else if actual.kind == expected.kind {
                                t::Argument::Reference {
                                    name: name.clone(),
                                    lift_result: expected.fallible && !actual.fallible,
                                }
                            } else {
                                t::Argument::Native {
                                    expression: self.context.argument(arg)?,
                                    references: arg_references(arg),
                                }
                            }
                        }
                        _ => {
                            let mut expression = self.context.argument(arg)?;
                            if signature.optional_inputs.contains(field)
                                && !matches!(arg, s::Argument::Missing { .. })
                            {
                                expression = format!("Some({expression})");
                            }
                            t::Argument::Native {
                                expression,
                                references: arg_references(arg),
                            }
                        }
                    };
                    args.insert(field.clone(), argument);
                }
                if let Some(contract) = &signature.children {
                    let mut references = vec![];
                    let mut values = vec![];
                    for child in children {
                        let child_name = self.temporary();
                        self.binding(&child_name, child, true)?;
                        values.push(if child.ty.kind==contract.kind {format!("binding_{child_name}.clone()")}else{format!("::organon_theoria::Provide::<super::r#{}>::provide(binding_{child_name})",contract.kind.spelling())});
                        references.push(child_name);
                    }
                    let children_name = self.temporary();
                    let ty = core_types::ValueType {
                        kind: core_types::Type::parse(&format!("__children_{element}"))?,
                        fallible: false,
                    };
                    self.bindings.push(t::Binding {
                        name: children_name.clone(),
                        inline: true,
                        ty: value_type(&ty),
                        references,
                        value: t::Expression::Native {
                            expression: format!("vec![{}]", values.join(",")),
                        },
                    });
                    self.types.insert(children_name.clone(), ty);
                    args.insert(
                        "__children".into(),
                        t::Argument::Reference {
                            name: children_name,
                            lift_result: false,
                        },
                    );
                }
                t::Expression::Instance {
                    element: element.clone(),
                    arguments: args,
                }
            }
        };
        self.types.insert(name.into(), expression.ty.clone());
        self.bindings.push(t::Binding {
            name: name.into(),
            inline,
            ty: value_type(&expression.ty),
            references: references(expression),
            value,
        });
        Ok(())
    }
}

fn kind(v: &core_types::Type) -> ty::Type {
    ty::Type::Element(v.spelling().into())
}
fn value_type(v: &core_types::ValueType) -> ty::ValueType {
    ty::ValueType {
        kind: kind(&v.kind),
        fallible: v.fallible,
    }
}
fn port(v: &core_types::Port) -> ty::Port {
    ty::Port {
        name: v.name.clone(),
        ty: value_type(&v.ty),
    }
}
fn lower_output(e: &s::Output) -> t::Output {
    t::Output {
        name: e.name.clone(),
        binding: e.binding.clone(),
        ty: value_type(&e.ty),
    }
}
struct Context<'a> {
    loaded: &'a loader::Loaded,
    signatures: &'a BTreeMap<String, s::Signature>,
    providers: BTreeMap<PathBuf, String>,
}
impl Context<'_> {
    fn marker(&self, name: &str) -> Result<String, Error> {
        let id = &self.loaded.origins[name];
        organon_cargo::declaration_path(id, &self.providers[&id.file])
    }
    fn argument(&self, arg: &s::Argument) -> Result<String, Error> {
        Ok(match arg {
            s::Argument::Missing { .. } => "None".into(),
            s::Argument::Reference { name, .. } => format!("binding_{name}.clone().into()"),
            s::Argument::Attribute { .. } => {
                return Err(Error(
                    "attribute interpretation requires a construction plan".into(),
                ));
            }
        })
    }
}

fn arg_references(arg: &s::Argument) -> Vec<String> {
    match arg {
        s::Argument::Reference { name, .. } => vec![name.clone()],
        _ => vec![],
    }
}
fn references(e: &s::Expression) -> Vec<String> {
    match &e.kind {
        s::ExpressionKind::Reference(n) => vec![n.clone()],
        s::ExpressionKind::Projection { parent, .. } => vec![parent.clone()],
        s::ExpressionKind::Construct {
            arguments,
            children,
            ..
        } => arguments
            .values()
            .flat_map(arg_references)
            .chain(children.iter().flat_map(references))
            .collect(),
    }
}
