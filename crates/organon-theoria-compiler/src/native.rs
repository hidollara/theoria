//! Generate calls to library contracts, without matching primitive names.
mod effects;
mod flow;
use crate::{Error, ir as t, types as ty};
use organon_core::types as core_types;
use organon_core::{loader, semantic as s, syntax};
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
};
pub(crate) fn compile(
    loaded: &loader::Loaded<std::path::PathBuf>,
    program: &s::Program,
    consumer: &Path,
    providers: BTreeMap<PathBuf, String>,
) -> Result<String, Error> {
    let mut context = Context {
        effects: effects::Effects::default(),
        loaded,
        contracts: &program.contracts,
        providers,
        children_inputs: program
            .contracts
            .iter()
            .filter_map(|(name, signature)| {
                signature.children.as_ref()?;
                let mut input = "__children".to_owned();
                if let Some(element) = program.bodies.iter().find(|e| e.name == *name) {
                    while signature.inputs.iter().any(|i| i.name == input)
                        || element.bindings.iter().any(|b| b.name == input)
                    {
                        input.push('_');
                    }
                }
                Some((name.clone(), input))
            })
            .collect(),
    };
    context.effects = effects::generate(program, &context)?;
    let mut definitions = vec![];
    let mut aliases = context.effects.aliases.clone();
    for (name, signature) in &program.contracts {
        if let Some(children) = &signature.children {
            writeln!(
                aliases,
                "pub type __children_{name}=Vec<r#{}>;",
                children.kind.spelling()
            )
            .unwrap();
        }
    }
    let mut prefix = String::new();
    prefix.push_str(
        "const __ORGANON_CONTRACTS: &[(&str, ::organon_theoria::attribute::Contract<'static>)] = &[\n",
    );
    for (name, signature) in &program.contracts {
        let id = &loaded.origins[name];
        let identity = organon_cargo::identity(&id.source, &id.name)?;
        let interfaces = signature
            .implements
            .iter()
            .map(|name| {
                let id = &loaded.origins[name];
                organon_cargo::identity(&id.source, &id.name).map(|name| format!("{name:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?
            .join(",");
        writeln!(prefix, "({name:?}, ::organon_theoria::attribute::Contract {{name:{identity:?}, interfaces:&[{interfaces}]}}),").unwrap();
    }
    prefix.push_str("];\n");
    prefix.push_str(&context.effects.code);
    prefix.push_str("fn __organon_registry()->::organon_theoria::attribute::Registry {let mut registry=::organon_theoria::attribute::Registry::default();");
    for path in loaded
        .definitions
        .definitions()
        .filter(|d| matches!(d, syntax::Definition::DeclareElement(e) if !e.tag))
        .map(|d| &context.providers[&loaded.origins[d.name()].source])
        .collect::<std::collections::BTreeSet<_>>()
    {
        write!(
            prefix,
            "registry.extend({path}::__organon::attribute_types());"
        )
        .unwrap();
    }
    for (name, signature) in &program.contracts {
        if !signature.constructible() {
            continue;
        }
        let ty = format!("__organon_values::r#{name}");
        let mut provide = String::from("|v,interface|{let _=v;match interface{");
        for interface in &signature.implements {
            let id = &loaded.origins[interface];
            let identity = organon_cargo::identity(&id.source, &id.name)?;
            let target = organon_cargo::declaration_path(id, &context.providers[&id.source])?;
            write!(provide,"{identity:?}=>Ok(::organon_theoria::attribute::Value::new(::organon_theoria::Provide::<{target}>::provide(&v.get::<{ty}>()?))),").unwrap();
        }
        provide.push_str("_=>Err(\"interface not provided\".into()),}}");
        writeln!(prefix,"registry.types.insert(concat!(\"__local_\",{name:?}).into(),::organon_theoria::attribute::TypeEntry{{contract: ::organon_theoria::attribute::contract({name:?},__ORGANON_CONTRACTS),provide:{provide}}});").unwrap();
    }
    prefix.push_str("registry}\n");
    let mut libraries = std::collections::HashMap::new();
    for (name, signature) in &program.contracts {
        if program.bodies.iter().any(|e| e.name == *name) {
            continue;
        }
        if signature.kind == (s::DefinitionKind::DeclareElement { tag: false }) {
            continue;
        }
        let mut inputs: Vec<_> = signature
            .inputs
            .iter()
            .map(|p| declared_port(&context, &p.name, &p.ty, &mut aliases))
            .collect::<Result<_, _>>()?;
        for input in &mut inputs {
            if signature.optional_inputs.contains(&input.name) {
                let alias = format!("__optional_{name}_{}", input.name);
                writeln!(
                    aliases,
                    "pub type {alias}=Option<{}>;",
                    crate::element::type_name(&input.ty)
                        .trim_start_matches("super::__organon_values::")
                )
                .unwrap();
                input.ty = ty::Type(alias);
            }
        }
        if signature.children.is_some() {
            let alias = format!("__children_{name}");
            inputs.push(ty::Port {
                name: context.children_inputs[name].clone(),
                ty: ty::Type(alias),
            });
        }
        definitions.push(if signature.constructible() {
            t::Definition::DeclareElement(t::DeclareElement {
                name: name.clone(),
                implements: signature.implements.clone(),
                inputs,
                outputs: signature
                    .outputs
                    .iter()
                    .map(|p| {
                        declared_port(
                            &context,
                            &p.name,
                            p.ty.declared().expect("native output contract"),
                            &mut aliases,
                        )
                    })
                    .collect::<Result<_, Error>>()?,
            })
        } else {
            t::Definition::Interface(t::Interface {
                name: name.clone(),
                outputs: signature
                    .outputs
                    .iter()
                    .map(|p| {
                        declared_port(
                            &context,
                            &p.name,
                            p.ty.declared().expect("interface output contract"),
                            &mut aliases,
                        )
                    })
                    .collect::<Result<_, _>>()?,
            })
        });
        let id = &loaded.origins[name];
        {
            let symbol = organon_cargo::identity(&id.source, &id.name)?;
            let provider = &context.providers[&id.source];
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
    for element in &program.bodies {
        let id = &loaded.origins[&element.name];
        if context.providers.contains_key(&id.source)
            && organon_cargo::manifest(&id.source)?
                != std::fs::canonicalize(consumer).map_err(|e| Error(e.to_string()))?
        {
            let name = &element.name;
            let symbol = organon_cargo::identity(&id.source, &id.name)?;
            let provider = &context.providers[&id.source];
            let module = format!("__provider_{name}");
            writeln!(
                prefix,
                "mod {module} {{pub use {provider}::__organon::{symbol} as r#{name};}}"
            )
            .unwrap();
            libraries.insert(name.clone(), module);
        }
    }
    for element in &program.bodies {
        let contract = &program.contracts[&element.name];
        let mut inputs: Vec<_> = contract
            .inputs
            .iter()
            .map(|i| {
                Ok(ty::Port {
                    name: i.name.to_string(),
                    ty: if i.ty.fallible() {
                        ty::Type(flow::value_alias(&context, &i.ty, &mut aliases)?)
                    } else {
                        kind(&i.ty.kind)
                    },
                })
            })
            .collect::<Result<_, Error>>()?;
        let incoming = context.children_inputs.get(&element.name).cloned();
        if let Some(name) = &incoming {
            inputs.push(ty::Port {
                name: name.clone(),
                ty: ty::Type(format!("__children_{}", element.name)),
            });
        }
        let fallible_interface = element.implementations.iter().any(|imp| {
            program.contracts[&imp.interface]
                .outputs
                .iter()
                .any(|port| port.ty.declared().is_some_and(|ty| ty.fallible()))
        });
        let body = if context.effects.fallible_bodies.contains(&element.name)
            || contract.inputs.iter().any(|p| p.ty.fallible())
            || fallible_interface
            || flow::needed(element)
        {
            t::Body::Scoped(element.clone())
        } else {
            let mut lower = Lower {
                context: &context,
                incoming,
                names: inputs
                    .iter()
                    .map(|i| i.name.clone())
                    .chain(element.bindings.iter().map(|b| b.name.clone()))
                    .collect(),
                bindings: vec![],
                next: 0,
            };
            for b in &element.bindings {
                lower.binding(&b.name, &b.value)?;
            }
            t::Body::Bindings(lower.bindings)
        };
        let output = |o: &s::OutputBinding, c: &s::Contract| {
            let mut result = lower_output(o, c);
            if c.kind == s::DefinitionKind::Element {
                result.ty = ty::Type(context.effects.alias(&element.name, &o.name));
            } else if let Some(ty) = c
                .outputs
                .iter()
                .find(|p| p.name == o.name)
                .unwrap()
                .ty
                .declared()
                && ty.fallible()
            {
                result.ty = ty::Type(flow::alias(ty));
            }
            result
        };
        definitions.push(t::Definition::Element(t::Element {
            name: element.name.clone(),
            inputs,
            body,
            outputs: element
                .outputs
                .iter()
                .map(|o| output(o, contract))
                .collect(),
            implementations: element
                .implementations
                .iter()
                .map(|i| t::Implementation {
                    interface: i.interface.clone(),
                    outputs: i
                        .outputs
                        .iter()
                        .map(|o| output(o, &program.contracts[&i.interface]))
                        .collect(),
                })
                .collect(),
        }));
    }
    for d in loaded.definitions.definitions() {
        if let syntax::Definition::DeclareElement(n) = d
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
    let plan = t::Program { definitions };
    for element in plan
        .elements()
        .filter(|element| !libraries.contains_key(&element.name))
    {
        let name = &element.name;
        let id = &loaded.origins[name];
        let identity = organon_cargo::identity(&id.source, &id.name)?;
        writeln!(prefix,"impl ::organon_theoria::Type for __organon_values::r#{name} {{const ID: &'static str={identity:?};type Value=Self;}}").unwrap();
    }
    let code = crate::definition::generate_program(&plan, &libraries, &mut |body, inputs| {
        flow::generate(body, program, &context, inputs, &mut aliases)
    })?;
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
    incoming: Option<String>,
    names: std::collections::BTreeSet<String>,
    bindings: Vec<t::Binding>,
    next: usize,
}
impl Lower<'_, '_> {
    fn temporary(&mut self) -> String {
        loop {
            let n = format!("__organon_child_{}", self.next);
            self.next += 1;
            if self.names.insert(n.clone()) {
                return n;
            }
        }
    }
    fn binding(&mut self, name: &str, expression: &s::Expression) -> Result<(), Error> {
        let value = match &expression.kind {
            s::ExpressionKind::Select(_) | s::ExpressionKind::Try { .. } => {
                unreachable!("the containing element was selected for lexical graph generation");
            }
            s::ExpressionKind::Reference(n) => t::Expression::Reference(n.clone()),
            s::ExpressionKind::AsInterface(n) => t::Expression::Native {
                expression: format!(
                    "::organon_theoria::Provide::<super::r#{}>::provide(binding_{n})",
                    expression.ty.spelling()
                ),
            },
            s::ExpressionKind::Projection { parent, output } => t::Expression::Projection {
                parent: parent.clone(),
                output: output.clone(),
            },
            s::ExpressionKind::Construct {
                name: element,
                arguments,
                children,
            } => {
                let signature = &self.context.contracts[element];
                let mut args = BTreeMap::new();
                for (field, arg) in arguments {
                    let argument = match arg {
                        s::Argument::Attribute { text, expected } => t::Argument::Interpreted {
                            marker: self.context.marker(expected.kind.spelling())?,
                            text: text.clone(),
                            optional: signature.optional_inputs.contains(field),
                        },
                        s::Argument::Reference { name, .. }
                            if !signature.optional_inputs.contains(field) =>
                        {
                            t::Argument::Reference { name: name.clone() }
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
                        if let s::Child::Incoming { kind } = child {
                            let name = self.incoming.as_ref().expect("checked children contract");
                            values.push(if *kind == contract.kind {
                                format!("children.extend(binding_{name}.iter().cloned());")
                            } else {
                                format!("children.extend(binding_{name}.iter().map(|child| ::organon_theoria::Provide::<super::r#{}>::provide(child)));", contract.kind.spelling())
                            });
                            references.push(name.clone());
                            continue;
                        }
                        let s::Child::Value(child) = child else {
                            unreachable!()
                        };
                        let child_name = self.temporary();
                        self.binding(&child_name, child)?;
                        values.push(if child.ty == contract.kind {
                            format!("children.push(binding_{child_name}.clone());")
                        } else {
                            format!("children.push(::organon_theoria::Provide::<super::r#{}>::provide(binding_{child_name}));", contract.kind.spelling())
                        });
                        references.push(child_name);
                    }
                    let children_name = self.temporary();
                    let ty = core_types::TypeName::parse(&format!("__children_{element}"))?;
                    self.bindings.push(t::Binding {
                        name: children_name.clone(),
                        ty: kind(&ty),
                        references,
                        value: t::Expression::Native {
                            expression: format!(
                                "{{let mut children=Vec::new();{} children}}",
                                values.join("")
                            ),
                        },
                    });
                    args.insert(
                        self.context.children_inputs[element].clone(),
                        t::Argument::Reference {
                            name: children_name,
                        },
                    );
                }
                t::Expression::Instance {
                    element: element.clone(),
                    arguments: args,
                }
            }
        };
        self.bindings.push(t::Binding {
            name: name.into(),
            ty: kind(&expression.ty),
            references: references(expression),
            value,
        });
        Ok(())
    }
}

fn kind(v: &core_types::TypeName) -> ty::Type {
    ty::Type(v.spelling().into())
}
fn declared_port(
    ctx: &Context<'_>,
    name: &str,
    value: &core_types::ValueType,
    aliases: &mut String,
) -> Result<ty::Port, Error> {
    Ok(ty::Port {
        name: name.into(),
        ty: if value.fallible() {
            ty::Type(flow::value_alias(ctx, value, aliases)?)
        } else {
            kind(&value.kind)
        },
    })
}
fn lower_output(e: &s::OutputBinding, contract: &s::Contract) -> t::Output {
    t::Output {
        name: e.name.clone(),
        binding: e.binding.clone(),
        ty: kind(
            contract
                .outputs
                .iter()
                .find(|p| p.name == e.name)
                .expect("checked output")
                .ty
                .kind(),
        ),
    }
}
struct Context<'a> {
    effects: effects::Effects,
    loaded: &'a loader::Loaded<std::path::PathBuf>,
    contracts: &'a BTreeMap<String, s::Contract>,
    providers: BTreeMap<PathBuf, String>,
    children_inputs: BTreeMap<String, String>,
}
impl Context<'_> {
    fn marker(&self, name: &str) -> Result<String, Error> {
        let id = &self.loaded.origins[name];
        organon_cargo::declaration_path(id, &self.providers[&id.source])
    }
    fn argument(&self, arg: &s::Argument) -> Result<String, Error> {
        Ok(match arg {
            s::Argument::Missing { .. } => "None".into(),
            s::Argument::Reference { name, .. } => format!("binding_{name}.clone()"),
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
        s::ExpressionKind::Select(_) | s::ExpressionKind::Try { .. } => vec![],
        s::ExpressionKind::Reference(n) | s::ExpressionKind::AsInterface(n) => vec![n.clone()],
        s::ExpressionKind::Projection { parent, .. } => vec![parent.clone()],
        s::ExpressionKind::Construct {
            arguments,
            children,
            ..
        } => arguments
            .values()
            .flat_map(arg_references)
            .chain(children.iter().flat_map(|child| match child {
                s::Child::Value(value) => references(value),
                s::Child::Incoming { .. } => vec![],
            }))
            .collect(),
    }
}
