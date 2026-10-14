//! Generate app-owned adapters from the same native contracts used by compiled code.
//! Nothing here is emitted by library!: the preview host explicitly requests it.
use organon_cargo::declaration_path as marker;
use organon_cargo::{CargoSources, identity as symbol};
use organon_core::Error;
use organon_core::types as core_types;
use organon_core::{format as f, loader};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    path::Path,
};

pub fn compile(entry: &Path, consumer: &Path) -> Result<String, Error> {
    let consumer = std::fs::canonicalize(consumer).map_err(|e| Error(e.to_string()))?;
    let consumer = consumer.as_path();
    let mut sources = CargoSources::default();
    let loaded = loader::load(entry, &mut sources)?;
    organon_core::semantic::resolve(&loaded.file)?;
    let paths = sources.import_provider_paths(consumer, &loaded)?;
    let names: BTreeMap<_, _> = loaded
        .origins
        .iter()
        .map(|(n, id)| Ok((n.clone(), symbol(&id.file, &id.name)?)))
        .collect::<Result<_, Error>>()?;
    let mut code = String::from(
        "pub fn registry()->::organon_preview::evaluation::Registry {let mut registry=::organon_preview::evaluation::Registry::default();\n",
    );
    let providers: BTreeSet<_> = loaded
        .file
        .definitions()
        .filter(|d| !matches!(d, f::Definition::Element(_)))
        .map(|d| paths[&loaded.origins[d.name()].file].clone())
        .collect();
    for path in providers {
        writeln!(
            code,
            "registry.attributes.extend({path}::__organon::attribute_types());"
        )
        .unwrap();
    }
    for d in loaded.file.definitions() {
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        if matches!(d, f::Definition::Element(_)) {
            continue;
        }
        let target = marker(id, &paths[&id.file])?;
        writeln!(
            code,
            "registry.attributes.alias({:?},{name:?}).expect(\"registered type\");",
            d.name()
        )
        .unwrap();
        writeln!(code,"registry.types.insert({name:?}.into(),::organon_preview::evaluation::TypeAdapter::of::<{target}>({name:?}));registry.types.insert({:?}.into(),registry.types[{name:?}].clone());",d.name()).unwrap();
        if let f::Definition::Interface(i) = d {
            let mut fields = String::new();
            for p in &i.outputs {
                let ty = rust_value(&loaded, &paths, &p.ty)?;
                let get = if p.ty.fallible {
                    let inner = rust_value(
                        &loaded,
                        &paths,
                        &core_types::ValueType {
                            kind: p.ty.kind.clone(),
                            fallible: false,
                        },
                    )?;
                    format!(
                        "match values.get({:?}).ok_or(\"missing interface output\")?{{Ok(v)=>Ok(v.get::<{inner}>()?),Err(_)=>Err(\"interface output failed\")}}",
                        p.name
                    )
                } else {
                    format!(
                        "values.get({:?}).ok_or(\"missing interface output\")?.as_ref().map_err(Clone::clone)?.get::<{ty}>()?",
                        p.name
                    )
                };
                write!(
                    fields,
                    "r#{}: ::organon_theoria::Source::from({get}),",
                    p.name
                )
                .unwrap();
            }

            writeln!(code,"registry.interfaces.insert({name:?}.into(),|values|Ok(::organon_theoria::attribute::Value::new({target}{{{fields}}}))); ").unwrap();
        }
        if let f::Definition::External(e) = d
            && e.tag
        {
            let mut build = format!("{target}::builder()");
            for p in e.inputs() {
                let ty = rust_value(&loaded, &paths, &p.ty)?;
                let read = if p.ty.fallible {
                    let inner = rust_value(
                        &loaded,
                        &paths,
                        &core_types::ValueType {
                            kind: p.ty.kind.clone(),
                            fallible: false,
                        },
                    )?;
                    format!(
                        "v.get::<Result<{inner},&'static str>>().or_else(|_| v.get::<{inner}>().map(Ok))?"
                    )
                } else {
                    format!("v.get::<{ty}>()?")
                };
                let value = if e.optional(&p.name) {
                    format!("values.next().flatten().map(|v|v.get::<{ty}>()).transpose()?")
                } else {
                    format!("{{let v=values.next().flatten().ok_or(\"missing input\")?;{read}}}")
                };
                write!(build, ".r#{}({value})", p.name).unwrap();
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
                write!(build,".__children(children.into_iter().map(|v|v.get::<{ty}>()).collect::<Result<Vec<_>,_>>()?)").unwrap();
            }
            build.push_str(".build(&reactive).map_err(|e|e.to_string())?");
            let mut values = String::new();
            for o in e.outputs() {
                let get = format!(
                    "built.r#{}_fixed().ok_or(\"expected fixed preview\")?.clone()",
                    o.port.name
                );
                let value = if o.port.ty.fallible {
                    format!(
                        "({get}).map(::organon_preview::evaluation::Value::new).map_err(str::to_owned)"
                    )
                } else {
                    format!("Ok(::organon_preview::evaluation::Value::new({get}))")
                };
                write!(values, "outputs.insert({:?}.into(),{value});", o.port.name).unwrap();
            }
            let mut interfaces = String::new();
            for i in e.implements() {
                let sig = loaded
                    .file
                    .definitions()
                    .find_map(|d| match d {
                        f::Definition::Interface(v) if v.name == i => Some(v),
                        _ => None,
                    })
                    .unwrap();
                let id = &loaded.origins[i];
                let interface = marker(id, &paths[&id.file])?;
                write!(interfaces,
                    "{{let provided: {interface}=::organon_theoria::Provide::provide(&built);interfaces.insert({:?}.into(),std::collections::BTreeMap::from([",
                    names[i]
                ).unwrap();
                for p in &sig.outputs {
                    let get = format!(
                        "provided.r#{}.as_fixed().ok_or(\"expected fixed preview\")?.clone()",
                        p.name
                    );
                    let value = if p.ty.fallible {
                        format!(
                            "({get}).map(::organon_preview::evaluation::Value::new).map_err(str::to_owned)"
                        )
                    } else {
                        format!("Ok(::organon_preview::evaluation::Value::new({get}))")
                    };
                    write!(interfaces, "({:?}.into(),{value}),", p.name).unwrap();
                }
                interfaces.push_str("]));}");
            }
            let inputs = e
                .inputs()
                .map(|p| format!("{:?}", p.name))
                .collect::<Vec<_>>()
                .join(",");
            let child = e
                .children()
                .map(|c| format!("Some({:?})", names[c.kind.spelling()]))
                .unwrap_or_else(|| "None".into());
            writeln!(code,"registry.elements.insert({name:?}.into(),::organon_preview::evaluation::Entry{{inputs:vec![{inputs}],children:{child},evaluate:|values,children|{{let reactive=::theoria_reactive::Reactive::new();let mut values=values.into_iter();let _=(&mut values,&children);let built={build};let mut outputs=std::collections::BTreeMap::new();{values}let mut interfaces=std::collections::BTreeMap::new();let _=&mut interfaces;{interfaces}Ok(::organon_preview::evaluation::Value::new(built).with_object(::organon_preview::evaluation::Object{{outputs,interfaces}}))}}}});").unwrap();
            writeln!(
                code,
                "registry.elements.insert({:?}.into(),registry.elements[{name:?}].clone());",
                d.name()
            )
            .unwrap();
        }
    }
    code.push_str("registry}\n");
    code.push_str(
        &organon_cargo::track_files(loaded.paths.iter().chain(sources.tracked_paths()))?
            .to_string(),
    );
    organon_cargo::connect_library_paths(
        &code.replace("::theoria_reactive", "::organon_theoria::reactive"),
        None,
    )
}

// Native values are obtained through the normal Type contract. XML Elements
// already name their generated Rust value type.
fn rust_value(
    loaded: &loader::Loaded,
    paths: &BTreeMap<std::path::PathBuf, String>,
    value: &core_types::ValueType,
) -> Result<String, Error> {
    let id = &loaded.origins[value.kind.spelling()];
    let marker = marker(id, &paths[&id.file])?;
    let definition = loaded
        .file
        .definitions()
        .find(|d| d.name() == value.kind.spelling())
        .unwrap();
    let ty = if matches!(definition, f::Definition::Element(_)) {
        marker
    } else {
        format!("<{marker} as ::organon_theoria::Type>::Value")
    };
    Ok(if value.fallible {
        format!("Result<{ty},&'static str>")
    } else {
        ty
    })
}
