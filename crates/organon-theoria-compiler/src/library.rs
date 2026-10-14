//! Native library ABI generation. Contracts contain no graphics vocabulary.
use crate::Error;
use organon_cargo::{CargoSources, declaration_path as marker, identity as symbol, manifest};
use organon_core::types as core_types;
use organon_core::{loader, syntax};
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
};

pub fn compile(entry: &Path, location: &str) -> Result<String, Error> {
    let mut sources = CargoSources::default();
    let loaded = sources.load(entry)?;
    let program = organon_core::semantic::resolve(&loaded.definitions)?;
    let owner = manifest(entry)?;
    let paths = sources.provider_paths(&owner, &loaded)?;
    let names: BTreeMap<_, _> = loaded
        .origins
        .iter()
        .map(|(n, id)| Ok((n.clone(), symbol(&id.source, &id.name)?)))
        .collect::<Result<_, Error>>()?;
    let kind = |t: &core_types::TypeName| crate::types::Type(names[t.spelling()].clone());
    // Keep the declaration's actual Rust error type in the native ABI.
    let mut aliases = String::new();
    let mut result_aliases = BTreeMap::new();
    for ty in program.contracts.values().flat_map(|c| {
        c.inputs
            .iter()
            .map(|p| &p.ty)
            .chain(c.outputs.iter().filter_map(|p| p.ty.declared()))
    }) {
        if !ty.fallible() {
            continue;
        }
        let key = ty.spelling();
        if result_aliases.contains_key(&key) {
            continue;
        }
        let alias = format!("__organon_result_{}", result_aliases.len());
        writeln!(
            aliases,
            "pub type {alias}={};",
            rust_value(&loaded, &paths, ty)?
        )
        .unwrap();
        result_aliases.insert(key, alias);
    }
    let port = |p: &core_types::Port| crate::types::Port {
        name: p.name.to_string(),
        ty: result_aliases
            .get(&p.ty.spelling())
            .map_or_else(|| kind(&p.ty.kind), |name| crate::types::Type(name.clone())),
    };
    let mut definitions = vec![];
    let mut foreign = std::collections::HashMap::new();
    let mut prefix = String::new();
    for d in loaded.definitions.definitions() {
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        let own = manifest(&id.source)? == owner;
        match d {
            syntax::Definition::Predicate(_) | syntax::Definition::DeclarePredicate(_) => {
                if own {
                    writeln!(prefix, "pub struct {name};").unwrap();
                    if let syntax::Definition::DeclarePredicate(p) = d {
                        let mut params = Vec::new();
                        let mut args = Vec::new();
                        for (index, input) in p.inputs().enumerate() {
                            params.push(format!(
                                "a{index}: &{}",
                                rust_value(&loaded, &paths, &input.ty)?
                            ));
                            args.push(format!("a{index}"));
                        }
                        let errors: Vec<_> = p.errors().cloned().collect();
                        let result = match errors.as_slice() {
                            [] => "bool".to_owned(),
                            [reason] => format!(
                                "Result<bool,{}>",
                                rust_value(
                                    &loaded,
                                    &paths,
                                    &core_types::ValueType {
                                        kind: reason.clone(),
                                        errors: vec![]
                                    }
                                )?
                            ),
                            _ => format!("Result<bool,{}>", error_set(&loaded, &paths, &errors)?),
                        };
                        writeln!(prefix, "pub type __PredicateResult_{name}={result};").unwrap();
                        writeln!(prefix,"impl {name} {{#[doc(hidden)]pub fn __organon_test({})->{result}{{Self::test({})}}}}",params.join(","),args.join(",")).unwrap();
                    }
                }
                continue;
            }
            syntax::Definition::DeclareElement(v) if !v.tag => {
                let marker = marker(id, &paths[&id.source])?;
                writeln!(
                    aliases,
                    "pub type {name}=<{marker} as ::organon_theoria::Type>::Value;"
                )
                .unwrap();
                if own {
                    writeln!(prefix, "pub struct {name};").unwrap();
                }
            }
            syntax::Definition::Interface(i) => {
                definitions.push(crate::ir::Definition::Interface(crate::ir::Interface {
                    name: name.clone(),
                    outputs: i.outputs.iter().map(&port).collect(),
                }))
            }
            syntax::Definition::DeclareElement(e) => {
                let mut inputs: Vec<_> = e.inputs().map(&port).collect();
                for input in &mut inputs {
                    if e.is_optional_input(&input.name) {
                        let alias = format!("{name}_optional_{}", input.name);
                        writeln!(
                            aliases,
                            "pub type {alias}=Option<{}>;",
                            rust_value(
                                &loaded,
                                &paths,
                                &e.inputs().find(|p| p.name == input.name).unwrap().ty
                            )?
                        )
                        .unwrap();
                        input.ty = crate::types::Type(alias);
                    }
                }
                if let Some(c) = e.children.as_ref() {
                    let alias = format!("{name}_children");
                    writeln!(
                        aliases,
                        "pub type {alias}=Vec<{}>;",
                        rust_value(
                            &loaded,
                            &paths,
                            &core_types::ValueType {
                                kind: c.kind.clone(),
                                errors: vec![]
                            }
                        )?
                    )
                    .unwrap();
                    inputs.push(crate::types::Port {
                        name: "__children".into(),
                        ty: crate::types::Type(alias),
                    });
                }
                definitions.push(crate::ir::Definition::DeclareElement(
                    crate::ir::DeclareElement {
                        name: name.clone(),
                        inputs,
                        outputs: native_outputs(&loaded, e)?.iter().map(port).collect(),
                        implements: e.implements().map(|n| names[n].clone()).collect(),
                    },
                ));
            }
            syntax::Definition::Element(_) => {
                let target = marker(id, &paths[&id.source])?;
                writeln!(aliases, "pub type {name}={target};").unwrap();
                continue;
            }
        }
        if !own && !matches!(d, syntax::Definition::DeclareElement(v) if !v.tag) {
            let module = format!("__provider_{name}");
            let provider = &paths[&id.source];
            writeln!(prefix, "mod {module} {{pub use {provider}::__organon::*;}}").unwrap();
            foreign.insert(name.clone(), module);
        }
    }
    let mut code = crate::definition::generate_program(
        &crate::ir::Program { definitions },
        &foreign,
        &mut |_, _| unreachable!("native declarations have no construction body"),
    )?;
    code = code.replacen(
        "pub mod __organon_values {\n",
        &format!("pub mod __organon_values {{\n{aliases}"),
        1,
    );
    prefix.push_str(&code);
    if loaded
        .definitions
        .definitions()
        .any(|d| matches!(d, syntax::Definition::DeclareElement(e) if e.tag && !foreign.contains_key(&names[e.name.as_str()])))
    {
        prefix.push_str("pub struct ReadInput<'a,'r,'e,I: ::organon_theoria::Declaration> { input: &'a Input<I>, cx: &'a ::organon_theoria::ReadContext<'r,'e> }\nimpl<'a,'r,'e,I: ::organon_theoria::Declaration> ReadInput<'a,'r,'e,I> {pub fn new(input:&'a Input<I>,cx:&'a ::organon_theoria::ReadContext<'r,'e>)->Self {Self{input,cx}} pub fn read<T:'static>(&self,source:&'a ::organon_theoria::Source<T>)->::organon_theoria::Read<'a,T>{source.read(self.cx)}}\n");
    }
    for d in loaded.definitions.definitions() {
        let syntax::Definition::DeclareElement(e) = d else {
            continue;
        };
        let id = &loaded.origins[e.name.as_str()];
        if manifest(&id.source)? != owner {
            continue;
        }
        let name = &names[e.name.as_str()];
        if !e.tag {
            let input = rust_value(
                &loaded,
                &paths,
                &core_types::ValueType {
                    kind: core_types::TypeName::parse(&e.name)?,
                    errors: vec![],
                },
            )?;
            for o in native_outputs(&loaded, e)? {
                let field = &o.name;
                let ty = rust_value(&loaded, &paths, &o.ty)?;
                writeln!(prefix,"pub struct {name}_{field}; impl ::organon_theoria::Output for {name}_{field} {{type Input={input};type Value={ty};}}").unwrap();
            }
            continue;
        }
        writeln!(prefix, "impl<'a,'r,'e> ReadInput<'a,'r,'e,{name}> {{").unwrap();
        for input in e.inputs() {
            let field = &input.name;
            let mut ty = rust_value(&loaded, &paths, &input.ty)?;
            if e.is_optional_input(field) {
                ty = format!("Option<{ty}>");
            }
            writeln!(prefix,"pub fn r#{field}(&self)->::organon_theoria::Read<'a,{ty}> {{self.input.r#{field}().read(self.cx)}}").unwrap();
        }
        if let Some(c) = e.children.as_ref() {
            let ty = rust_value(
                &loaded,
                &paths,
                &core_types::ValueType {
                    kind: c.kind.clone(),
                    errors: vec![],
                },
            )?;
            writeln!(prefix,"pub fn children(&self)->::organon_theoria::Read<'a,Vec<{ty}>> {{self.input.__children().read(self.cx)}}").unwrap();
        }
        prefix.push_str("}\n");
        writeln!(prefix,"impl ::organon_theoria::Type for {name} {{const ID: &'static str = {name:?};type Value=Built<Self>;  fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self::Value>,String>{{use ::organon_theoria::DefaultParse as _;Self::__organon_parse(text,scope)}}}}").unwrap();
        writeln!(prefix,"pub struct Impl_{name}; impl ::organon_theoria::Implementation<{name}> for Impl_{name} {{fn build(&self,input:Input<{name}>)-><{name} as ::organon_theoria::Declaration>::Definition {{let input=std::rc::Rc::new(input); (").unwrap();
        for o in native_outputs(&loaded, e)? {
            let field = &o.name;
            writeln!(
                prefix,
                "<{name}_{field} as ::organon_theoria::Evaluate>::calculation(input.clone()),"
            )
            .unwrap();
        }
        prefix.push_str(")}}\n");
        for o in native_outputs(&loaded, e)? {
            let field = &o.name;
            let ty = rust_value(&loaded, &paths, &o.ty)?;
            writeln!(prefix,"pub struct {name}_{field};impl ::organon_theoria::Output for {name}_{field} {{type Input=Input<{name}>;type Value={ty};}}").unwrap();
        }
        for interface in e.implements() {
            let target = &names[interface];
            let outputs = &program.contracts[interface].outputs;
            writeln!(prefix,"impl ::organon_theoria::Provide<{target}> for Built<{name}> {{fn provide(&self)->{target} {{{target} {{").unwrap();
            for output in outputs {
                let source = interface_output_name(e, interface, &output.name);
                writeln!(prefix, "r#{}:self.r#{}(),", output.name, source).unwrap();
            }
            prefix.push_str("}}}\n");
        }
    }
    for d in loaded.definitions.definitions() {
        if manifest(&loaded.origins[d.name()].source)? == owner
            && !matches!(d, syntax::Definition::DeclareElement(v) if !v.tag)
            && !matches!(d, syntax::Definition::Element(_))
        {
            writeln!(prefix, "pub use {} as r#{};", names[d.name()], d.name()).unwrap();
        }
    }
    for d in loaded.definitions.definitions() {
        if let syntax::Definition::Interface(i) = d {
            if manifest(&loaded.origins[i.name.as_str()].source)? != owner {
                continue;
            }
            let name = &names[i.name.as_str()];
            let resolve = interface_resolution(name, name);
            writeln!(prefix,"impl ::organon_theoria::Type for {name} {{const ID: &'static str = {name:?};type Value=Self;fn resolve(v:&::organon_theoria::attribute::Value)->Result<Self,String>{{{resolve}}}fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self>,String>{{#[allow(unused_imports)] use ::organon_theoria::DefaultParse as _; Self::__organon_parse(text,scope)}}}}").unwrap();
        }
    }
    let registry = registry_code(&loaded, &paths, &names, &owner)?;
    for d in loaded.definitions.definitions() {
        if matches!(d, syntax::Definition::Element(_)) {
            writeln!(
                prefix,
                "pub use super::__organon_elements::r#{} as {};",
                d.name(),
                names[d.name()]
            )
            .unwrap();
        }
    }
    prefix.push_str("pub mod types {\n");
    for d in loaded.definitions.definitions() {
        let id = &loaded.origins[d.name()];
        let target = marker(id, &paths[&id.source])?;
        writeln!(prefix, "pub use {target} as r#{};", d.name()).unwrap();
    }
    prefix.push_str("}\n");
    let dependencies = sources.dependency_exports(&owner, &loaded)?;
    let mut result =
        format!("#[doc(hidden)] pub mod __organon {{\n{prefix}\n{registry}\n{dependencies}\n}}\n",);
    result.push_str("/// Type contracts used to interpret attributes.\n#[doc(inline)] pub use __organon::attribute_types;\n");
    result.push_str("/// Organon declaration types, distinct from their Rust value representations.\npub mod types {\n");
    for (public, local) in &loaded.public {
        writeln!(
            result,
            "pub use crate::__organon::types::r#{local} as r#{public};"
        )
        .unwrap();
    }
    result.push_str("}\n");
    let tracked = &loaded.source_order;
    if loaded
        .definitions
        .definitions()
        .any(|d| matches!(d, syntax::Definition::Element(_)))
    {
        let compiled = crate::native::compile(
            &loaded,
            &program,
            &owner,
            sources.import_provider_paths(&owner, &loaded)?,
        )?;
        writeln!(
            result,
            "#[doc(hidden)] pub mod __organon_elements {{\n{}\n}}",
            compiled
        )
        .unwrap();
    }
    result.push_str(
        &organon_cargo::track_files(tracked.iter().chain(sources.tracked_paths()))?.to_string(),
    );
    for (public, local) in &loaded.public {
        let definition = loaded
            .definitions
            .definitions()
            .find(|d| d.name() == local)
            .unwrap();
        let target = match definition {
            syntax::Definition::Element(_) => format!("__organon_elements::r#{local}"),
            syntax::Definition::DeclareElement(v) if !v.tag => {
                writeln!(result, "#[allow(non_camel_case_types)] pub type r#{public} = <__organon::types::r#{local} as ::organon_theoria::Type>::Value;").unwrap();
                continue;
            }
            _ => format!("__organon::types::r#{local}"),
        };
        writeln!(result, "pub use {target} as r#{public};").unwrap();
    }
    organon_cargo::connect_library_paths(&result, Some(location))
}
/// Internal storage names keep interface outputs separate from public Element outputs.
pub fn interface_output_name(e: &syntax::DeclareElement, interface: &str, field: &str) -> String {
    let mut name = format!("__interface_{}_{}_{}", interface.len(), interface, field);
    while e.outputs().any(|o| o.name == name) {
        name.push('_');
    }
    name
}
pub(crate) fn native_outputs(
    loaded: &loader::Loaded<std::path::PathBuf>,
    e: &syntax::DeclareElement,
) -> Result<Vec<core_types::Port>, Error> {
    let mut outputs: Vec<_> = e.outputs().cloned().collect();
    for interface in e.implements() {
        let contract = loaded
            .definitions
            .definitions()
            .find_map(|d| match d {
                syntax::Definition::Interface(v) if v.name == interface => Some(v),
                _ => None,
            })
            .ok_or_else(|| Error(format!("unknown interface {interface:?}")))?;
        for p in &contract.outputs {
            let mut port = p.clone();
            port.name = interface_output_name(e, interface, &p.name).parse()?;
            outputs.push(port);
        }
    }
    Ok(outputs)
}
/// Rust representation of a successful value, through its declaration's Type contract.
pub fn value_type(
    loaded: &loader::Loaded<PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    kind: &core_types::TypeName,
) -> Result<String, Error> {
    let id = &loaded.origins[kind.spelling()];
    let marker = marker(id, &paths[&id.source])?;
    Ok(format!("<{marker} as ::organon_theoria::Type>::Value"))
}
pub(crate) fn rust_value(
    loaded: &loader::Loaded<std::path::PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    value: &core_types::ValueType,
) -> Result<String, Error> {
    let ty = value_type(loaded, paths, &value.kind)?;
    match value.errors.as_slice() {
        [] => Ok(ty),
        [reason] => {
            let reason = rust_value(
                loaded,
                paths,
                &core_types::ValueType {
                    kind: reason.clone(),
                    errors: vec![],
                },
            )?;
            Ok(format!("Result<{ty},{reason}>"))
        }
        errors => Ok(format!(
            "Result<{ty},{}>",
            error_set(loaded, paths, errors)?
        )),
    }
}
/// Use declaration identity to keep the Rust type identical across imports, aliases,
/// and different orderings of the same XML contract.
pub fn error_set(
    loaded: &loader::Loaded<PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    errors: &[core_types::TypeName],
) -> Result<String, Error> {
    let mut reasons = BTreeMap::new();
    for reason in errors {
        let origin = &loaded.origins[reason.spelling()];
        reasons.insert(
            symbol(&origin.source, &origin.name)?,
            marker(origin, &paths[&origin.source])?,
        );
    }
    let set = reasons
        .into_values()
        .rev()
        .fold("()".to_owned(), |tail, head| format!("({head},{tail})"));
    Ok(format!("::organon_theoria::DeclaredFailure<{set}>"))
}
fn registry_code(
    loaded: &loader::Loaded<std::path::PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    names: &BTreeMap<String, String>,
    owner: &Path,
) -> Result<String, Error> {
    let mut code = String::from(
        "pub fn attribute_types()->::organon_theoria::attribute::Registry {let mut registry=::organon_theoria::attribute::Registry::default();\n",
    );
    for d in loaded.definitions.definitions() {
        if matches!(
            d,
            syntax::Definition::Element(_)
                | syntax::Definition::Predicate(_)
                | syntax::Definition::DeclarePredicate(_)
        ) {
            continue;
        }
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        if manifest(&id.source)? != owner {
            writeln!(code,"registry.types.insert({name:?}.into(),{}::__organon::attribute_types().types.remove({name:?}).expect(\"registered contract\"));",paths[&id.source]).unwrap();
        } else {
            let ty = rust_value(
                loaded,
                paths,
                &core_types::ValueType {
                    kind: core_types::TypeName::parse(d.name())?,
                    errors: vec![],
                },
            )?;
            let provide = if let syntax::Definition::DeclareElement(e) = d {
                provide_code(loaded, paths, e, &ty)?
            } else {
                "|_,_|Err(\"interface not provided\".into())".into()
            };
            let contracts = if let syntax::Definition::DeclareElement(e) = d {
                e.implements()
                    .map(|i| format!("{:?}", names[i]))
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                String::new()
            };
            writeln!(code,"registry.types.insert({name:?}.into(),::organon_theoria::attribute::TypeEntry{{contract: ::organon_theoria::attribute::Contract{{name:{name:?},interfaces:&[{contracts}]}},provide:{provide}}});").unwrap();
        }
        writeln!(
            code,
            "registry.types.insert({:?}.into(),registry.types[{name:?}].clone());",
            d.name()
        )
        .unwrap();
    }
    code.push_str("registry}\n");
    Ok(code)
}
pub(crate) fn provide_code(
    loaded: &loader::Loaded<std::path::PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    declaration: &syntax::DeclareElement,
    ty: &str,
) -> Result<String, Error> {
    provide_interfaces(loaded, paths, declaration.implements(), ty)
}
pub(crate) fn provide_interfaces<'a>(
    loaded: &loader::Loaded<std::path::PathBuf>,
    paths: &BTreeMap<PathBuf, String>,
    interfaces: impl Iterator<Item = &'a str>,
    ty: &str,
) -> Result<String, Error> {
    let mut code = String::from("|v,interface|{let _=v;match interface{");
    for interface in interfaces {
        let id = &loaded.origins[interface];
        let target = marker(id, &paths[&id.source])?;
        let name = symbol(&id.source, &id.name)?;
        write!(code,"{name:?}=>Ok(::organon_theoria::attribute::Value::new(::organon_theoria::Provide::<{target}>::provide(&v.get::<{ty}>()?))),").unwrap();
    }
    code.push_str("_=>Err(\"interface not provided\".into()),}}");
    Ok(code)
}
pub(crate) fn interface_resolution(name: &str, identity: &str) -> String {
    format!(
        "if let Ok(value)=v.get::<{name}>(){{return Ok(value);}}v.provide({identity:?})?.get::<{name}>()"
    )
}
