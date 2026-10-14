//! Native library ABI generation. Contracts contain no graphics vocabulary.
use crate::Error;
use organon_cargo::{CargoSources, declaration_path as marker, identity as symbol, manifest};
use organon_core::types as core_types;
use organon_core::{format as f, loader};
use std::{
    collections::BTreeMap,
    fmt::Write,
    path::{Path, PathBuf},
};

pub fn compile(entry: &Path, location: &str) -> Result<String, Error> {
    let mut sources = CargoSources::default();
    let loaded = loader::load(entry, &mut sources)?;
    let program = organon_core::semantic::resolve(&loaded.file)?;
    let owner = manifest(entry)?;
    let paths = sources.provider_paths(&owner, &loaded)?;
    let names: BTreeMap<_, _> = loaded
        .origins
        .iter()
        .map(|(n, id)| Ok((n.clone(), symbol(&id.file, &id.name)?)))
        .collect::<Result<_, Error>>()?;
    let kind = |t: &core_types::Type| crate::types::Type::Element(names[t.spelling()].clone());
    let port = |p: &core_types::Port| crate::types::Port {
        name: p.name.clone(),
        ty: crate::types::ValueType {
            kind: kind(&p.ty.kind),
            fallible: p.ty.fallible,
        },
    };
    let mut definitions = vec![];
    let mut foreign = std::collections::HashMap::new();
    let mut prefix = String::new();
    let mut aliases = String::new();
    for d in loaded.file.definitions() {
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        let own = manifest(&id.file)? == owner;
        match d {
            f::Definition::External(v) if !v.tag => {
                let marker = marker(id, &paths[&id.file])?;
                writeln!(
                    aliases,
                    "pub type {name}=<{marker} as ::organon_theoria::Type>::Value;"
                )
                .unwrap();
                if own {
                    writeln!(prefix, "pub struct {name};").unwrap();
                }
            }
            f::Definition::Interface(i) => {
                definitions.push(crate::ir::Definition::Interface(crate::ir::Interface {
                    name: name.clone(),
                    outputs: i.outputs.iter().map(&port).collect(),
                }))
            }
            f::Definition::External(e) => {
                let mut inputs: Vec<_> = e.inputs().map(&port).collect();
                for input in &mut inputs {
                    if e.optional(&input.name) {
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
                        input.ty = crate::types::ValueType {
                            kind: crate::types::Type::Element(alias),
                            fallible: false,
                        };
                    }
                }
                if let Some(c) = e.children() {
                    let alias = format!("{name}_children");
                    writeln!(
                        aliases,
                        "pub type {alias}=Vec<{}>;",
                        rust_value(
                            &loaded,
                            &paths,
                            &core_types::ValueType {
                                kind: c.kind.clone(),
                                fallible: false
                            }
                        )?
                    )
                    .unwrap();
                    inputs.push(crate::types::Port {
                        name: "__children".into(),
                        ty: crate::types::ValueType {
                            kind: crate::types::Type::Element(alias),
                            fallible: false,
                        },
                    });
                }
                definitions.push(crate::ir::Definition::External(crate::ir::External {
                    name: name.clone(),
                    inputs,
                    outputs: native_outputs(&loaded, e)?
                        .iter()
                        .map(|o| crate::ir::DeclaredOutput {
                            port: port(&o.port),
                            inline: o.inline,
                        })
                        .collect(),
                    implements: e.implements().map(|n| names[n].clone()).collect(),
                }));
            }
            f::Definition::Element(_) => {
                let target = marker(id, &paths[&id.file])?;
                writeln!(aliases, "pub type {name}={target};").unwrap();
                continue;
            }
        }
        if !own && !matches!(d, f::Definition::External(v) if !v.tag) {
            let module = format!("__provider_{name}");
            let provider = &paths[&id.file];
            writeln!(prefix, "mod {module} {{pub use {provider}::__organon::*;}}").unwrap();
            foreign.insert(name.clone(), module);
        }
    }
    let mut code = crate::definition::generate_program(
        &crate::ir::Program {
            definitions,
            construction_order: vec![],
        },
        &foreign,
    )?;
    code = code.replacen(
        "pub mod __organon_values {\n",
        &format!("pub mod __organon_values {{\n{aliases}"),
        1,
    );
    prefix.push_str(&code);
    if loaded
        .file
        .definitions()
        .any(|d| matches!(d, f::Definition::External(e) if e.tag && !foreign.contains_key(&names[&e.name])))
    {
        prefix.push_str("pub struct ReadInput<'a,'r,'e,I: ::organon_theoria::Declaration> { input: &'a Input<I>, cx: &'a ::organon_theoria::ReadContext<'r,'e> }\nimpl<'a,'r,'e,I: ::organon_theoria::Declaration> ReadInput<'a,'r,'e,I> {pub fn new(input:&'a Input<I>,cx:&'a ::organon_theoria::ReadContext<'r,'e>)->Self {Self{input,cx}} pub fn read<T:'static>(&self,source:&'a ::organon_theoria::Source<T>)->::organon_theoria::Read<'a,T>{source.read(self.cx)}}\n");
    }
    for d in loaded.file.definitions() {
        let f::Definition::External(e) = d else {
            continue;
        };
        let id = &loaded.origins[&e.name];
        if manifest(&id.file)? != owner {
            continue;
        }
        let name = &names[&e.name];
        if !e.tag {
            let input = rust_value(
                &loaded,
                &paths,
                &core_types::ValueType {
                    kind: core_types::Type::parse(&e.name)?,
                    fallible: false,
                },
            )?;
            for o in native_outputs(&loaded, e)? {
                let field = &o.port.name;
                let ty = rust_value(&loaded, &paths, &o.port.ty)?;
                writeln!(prefix,"pub struct {name}_{field}; impl ::organon_theoria::Output for {name}_{field} {{type Input={input};type Value={ty};}}").unwrap();
            }
            continue;
        }
        writeln!(prefix, "impl<'a,'r,'e> ReadInput<'a,'r,'e,{name}> {{").unwrap();
        for input in e.inputs() {
            let field = &input.name;
            let mut ty = rust_value(&loaded, &paths, &input.ty)?;
            if e.optional(field) {
                ty = format!("Option<{ty}>");
            }
            writeln!(prefix,"pub fn r#{field}(&self)->::organon_theoria::Read<'a,{ty}> {{self.input.r#{field}().read(self.cx)}}").unwrap();
        }
        if let Some(c) = e.children() {
            let ty = rust_value(
                &loaded,
                &paths,
                &core_types::ValueType {
                    kind: c.kind.clone(),
                    fallible: false,
                },
            )?;
            writeln!(prefix,"pub fn children(&self)->::organon_theoria::Read<'a,Vec<{ty}>> {{self.input.__children().read(self.cx)}}").unwrap();
        }
        prefix.push_str("}\n");
        writeln!(prefix,"impl ::organon_theoria::Type for {name} {{type Value=Built<Self>; fn reference_only()->bool{{#[allow(unused_imports)] use ::organon_theoria::DefaultParse as _;Self::__ORGANON_REFERENCE_ONLY}} fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self::Value>,String>{{use ::organon_theoria::DefaultParse as _;Self::__organon_parse(text,scope)}}}}").unwrap();
        writeln!(prefix,"pub struct Impl_{name}; impl ::organon_theoria::Implementation<{name}> for Impl_{name} {{fn build(&self,input:Input<{name}>)-><{name} as ::organon_theoria::Declaration>::Definition {{let input=std::rc::Rc::new(input); (").unwrap();
        for o in native_outputs(&loaded, e)? {
            let field = &o.port.name;
            writeln!(
                prefix,
                "<{name}_{field} as ::organon_theoria::Evaluate>::calculation(input.clone()),"
            )
            .unwrap();
        }
        prefix.push_str(")}}\n");
        for o in native_outputs(&loaded, e)? {
            let field = &o.port.name;
            let ty = rust_value(&loaded, &paths, &o.port.ty)?;
            writeln!(prefix,"pub struct {name}_{field};impl ::organon_theoria::Output for {name}_{field} {{type Input=Input<{name}>;type Value={ty};}}").unwrap();
        }
        for interface in e.implements() {
            let target = &names[interface];
            let outputs = &program.signatures[interface].outputs;
            writeln!(prefix,"impl ::organon_theoria::Provide<{target}> for Built<{name}> {{fn provide(&self)->{target} {{{target} {{").unwrap();
            for output in outputs {
                let source = provided(e, interface, &output.name);
                writeln!(prefix, "r#{}:self.r#{}(),", output.name, source).unwrap();
            }
            prefix.push_str("}}}\n");
        }
    }
    for d in loaded.file.definitions() {
        if manifest(&loaded.origins[d.name()].file)? == owner
            && !matches!(d, f::Definition::External(v) if !v.tag)
            && !matches!(d, f::Definition::Element(_))
        {
            writeln!(prefix, "pub use {} as r#{};", names[d.name()], d.name()).unwrap();
        }
    }
    for d in loaded.file.definitions() {
        if let f::Definition::Interface(i) = d {
            if manifest(&loaded.origins[&i.name].file)? != owner {
                continue;
            }
            let name = &names[&i.name];
            let resolve = interface_resolution(name, name);
            writeln!(prefix,"impl ::organon_theoria::Type for {name} {{type Value=Self;fn reference_only()->bool{{#[allow(unused_imports)] use ::organon_theoria::DefaultInterpret as _;Self::__ORGANON_REFERENCE_ONLY}}fn resolve(v:&::organon_theoria::attribute::Value)->Result<Self,String>{{{resolve}}}fn interpret(text:&str,scope:&::organon_theoria::attribute::Scope)->Result<::organon_theoria::attribute::Attribute<Self>,String>{{#[allow(unused_imports)] use ::organon_theoria::DefaultInterpret as _; Self::__organon_interpret(text,scope)}}}}").unwrap();
        }
    }
    let registry = registry_code(&loaded, &paths, &names, &owner)?;
    for d in loaded.file.definitions() {
        if matches!(d, f::Definition::Element(_)) {
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
    for d in loaded.file.definitions() {
        let id = &loaded.origins[d.name()];
        let target = marker(id, &paths[&id.file])?;
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
    let tracked = &loaded.paths;
    if loaded
        .file
        .definitions()
        .any(|d| matches!(d, f::Definition::Element(_)))
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
            .file
            .definitions()
            .find(|d| d.name() == local)
            .unwrap();
        let target = match definition {
            f::Definition::Element(_) => format!("__organon_elements::r#{local}"),
            f::Definition::External(v) if !v.tag => {
                writeln!(result, "#[allow(non_camel_case_types)] pub type r#{public} = <__organon::types::r#{local} as ::organon_theoria::Type>::Value;").unwrap();
                continue;
            }
            _ => format!("__organon::types::r#{local}"),
        };
        writeln!(result, "pub use {target} as r#{public};").unwrap();
    }
    organon_cargo::connect_library_paths(
        &result
            .replace("::theoria_reactive", "::organon_theoria::reactive")
            .replace("::theoria_component", "::organon_theoria::component"),
        Some(location),
    )
}
/// Internal storage names keep interface outputs separate from public Element outputs.
pub fn interface_output_name(e: &f::ExternalDefinition, interface: &str, field: &str) -> String {
    let mut name = format!("__interface_{}_{}_{}", interface.len(), interface, field);
    while e.outputs().any(|o| o.port.name == name) {
        name.push('_');
    }
    name
}
pub(crate) fn provided(e: &f::ExternalDefinition, interface: &str, field: &str) -> String {
    e.implementation(interface)
        .and_then(|i| i.provides.iter().find(|p| p.name == field))
        .map(|p| p.reference.clone())
        .unwrap_or_else(|| interface_output_name(e, interface, field))
}
pub(crate) fn native_outputs(
    loaded: &loader::Loaded,
    e: &f::ExternalDefinition,
) -> Result<Vec<f::DeclaredOutput>, Error> {
    let mut outputs: Vec<_> = e.outputs().cloned().collect();
    for interface in e.implements().filter(|i| e.implementation(i).is_none()) {
        let contract = loaded
            .file
            .definitions()
            .find_map(|d| match d {
                f::Definition::Interface(v) if v.name == interface => Some(v),
                _ => None,
            })
            .ok_or_else(|| Error(format!("unknown interface {interface:?}")))?;
        for p in &contract.outputs {
            let mut port = p.clone();
            port.name = interface_output_name(e, interface, &p.name);
            outputs.push(f::DeclaredOutput {
                port,
                inline: false,
            });
        }
    }
    Ok(outputs)
}
pub(crate) fn rust_value(
    loaded: &loader::Loaded,
    paths: &BTreeMap<PathBuf, String>,
    value: &core_types::ValueType,
) -> Result<String, Error> {
    let id = &loaded.origins[value.kind.spelling()];
    let marker = marker(id, &paths[&id.file])?;
    let ty = match loaded
        .file
        .definitions()
        .find(|d| d.name() == value.kind.spelling())
        .unwrap()
    {
        f::Definition::External(v) if !v.tag => {
            format!("<{marker} as ::organon_theoria::Type>::Value")
        }
        f::Definition::External(_) => format!("{}::__organon::Built<{marker}>", paths[&id.file]),
        _ => marker,
    };
    Ok(if value.fallible {
        format!("Result<{ty},&'static str>")
    } else {
        ty
    })
}
fn registry_code(
    loaded: &loader::Loaded,
    paths: &BTreeMap<PathBuf, String>,
    names: &BTreeMap<String, String>,
    owner: &Path,
) -> Result<String, Error> {
    let mut code = String::from(
        "pub fn attribute_types()->::organon_theoria::attribute::Registry {let mut registry=::organon_theoria::attribute::Registry::default();\n",
    );
    for d in loaded.file.definitions() {
        if matches!(d, f::Definition::Element(_)) {
            continue;
        }
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        if manifest(&id.file)? != owner {
            writeln!(code,"registry.types.insert({name:?}.into(),{}::__organon::attribute_types().types.remove({name:?}).expect(\"registered contract\"));",paths[&id.file]).unwrap();
        } else {
            let ty = rust_value(
                loaded,
                paths,
                &core_types::ValueType {
                    kind: core_types::Type::parse(d.name())?,
                    fallible: false,
                },
            )?;
            let (interfaces, provide) = if let f::Definition::External(e) = d {
                let interfaces = e
                    .implements()
                    .map(|i| {
                        let id = &loaded.origins[i];
                        marker(id, &paths[&id.file])
                            .map(|m| format!("std::any::TypeId::of::<{m}>()"))
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",");
                (interfaces, provide_code(loaded, paths, e, &ty)?)
            } else {
                (
                    String::new(),
                    "|_,_|Err(\"interface not provided\".into())".into(),
                )
            };
            writeln!(code,"registry.types.insert({name:?}.into(),::organon_theoria::attribute::TypeEntry{{interfaces:vec![{interfaces}],name:{name:?},rust_type:std::any::TypeId::of::<{ty}>,provide:{provide}}});").unwrap();
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
    loaded: &loader::Loaded,
    paths: &BTreeMap<PathBuf, String>,
    declaration: &f::ExternalDefinition,
    ty: &str,
) -> Result<String, Error> {
    provide_interfaces(loaded, paths, declaration.implements(), ty)
}
pub(crate) fn provide_interfaces<'a>(
    loaded: &loader::Loaded,
    paths: &BTreeMap<PathBuf, String>,
    interfaces: impl Iterator<Item = &'a str>,
    ty: &str,
) -> Result<String, Error> {
    let mut code = String::from("|v,interface|{let _=v;match interface{");
    for interface in interfaces {
        let id = &loaded.origins[interface];
        let target = marker(id, &paths[&id.file])?;
        let name = symbol(&id.file, &id.name)?;
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
