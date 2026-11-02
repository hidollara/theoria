//! Generate app-owned adapters from the same native contracts used by compiled code.
//! Nothing here is emitted by library!: the preview host explicitly requests it.
use organon_cargo::declaration_path as marker;
use organon_cargo::{CargoSources, identity as symbol};
use organon_core::Error;
use organon_core::types as core_types;
use organon_core::{loader, syntax};
use organon_theoria_compiler::library::value_type;
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write,
    path::Path,
};

pub fn compile(entry: &Path, consumer: &Path) -> Result<String, Error> {
    let consumer = std::fs::canonicalize(consumer).map_err(|e| Error(e.to_string()))?;
    let consumer = consumer.as_path();
    let mut sources = CargoSources::default();
    let loaded = sources.load(entry)?;
    organon_core::semantic::resolve(&loaded.definitions)?;
    let paths = sources.import_provider_paths(consumer, &loaded)?;
    let names: BTreeMap<_, _> = loaded
        .origins
        .iter()
        .map(|(n, id)| Ok((n.clone(), symbol(&id.source, &id.name)?)))
        .collect::<Result<_, Error>>()?;
    let mut code = String::from(
        "pub fn registry()->::organon_preview::evaluation::Registry {let mut registry=::organon_preview::evaluation::Registry::default();\n",
    );
    let providers: BTreeSet<_> = loaded
        .definitions
        .definitions()
        .filter(|d| !matches!(d, syntax::Definition::Element(_)))
        .map(|d| paths[&loaded.origins[d.name()].source].clone())
        .collect();
    for path in providers {
        writeln!(
            code,
            "registry.attributes.extend({path}::__organon::attribute_types());"
        )
        .unwrap();
    }
    for d in loaded.definitions.definitions() {
        let id = &loaded.origins[d.name()];
        let name = &names[d.name()];
        if matches!(
            d,
            syntax::Definition::Element(_) | syntax::Definition::Predicate(_)
        ) {
            continue;
        }
        let target = marker(id, &paths[&id.source])?;
        if let syntax::Definition::DeclarePredicate(p) = d {
            let mut args = Vec::new();
            for input in p.inputs() {
                args.push(format!(
                    "&({})",
                    native_input(
                        &loaded,
                        &paths,
                        &input.ty,
                        "values.next().expect(\"predicate argument\")"
                    )?
                ));
            }
            let call = format!("{target}::__organon_test({})", args.join(","));
            let result = erase_errors(
                &loaded,
                &p.errors().cloned().collect::<Vec<_>>(),
                &call,
                "|v|v",
            )?;
            writeln!(code,"registry.predicates.insert({name:?}.into(),|values,_contracts|{{let mut values=values.into_iter();{result}}});registry.predicates.insert({:?}.into(),registry.predicates[{name:?}]);",d.name()).unwrap();
            continue;
        }
        writeln!(
            code,
            "registry.attributes.alias({:?},{name:?}).expect(\"registered type\");",
            d.name()
        )
        .unwrap();
        writeln!(code,"registry.types.insert({name:?}.into(),::organon_preview::evaluation::TypeAdapter::of::<{target}>({name:?}));registry.types.insert({:?}.into(),registry.types[{name:?}].clone());",d.name()).unwrap();
        if let syntax::Definition::Interface(i) = d {
            let mut fields = String::new();
            for p in &i.outputs {
                let field = &p.name;
                let read = format!(
                    "v.get::<{target}>()?.r#{field}_fixed().ok_or(\"expected a fixed interface output\")?.clone()"
                );
                let projected = erase_errors(
                    &loaded,
                    &p.ty.errors,
                    &read,
                    "::organon_preview::evaluation::Value::new",
                )?;
                writeln!(code,"registry.projections.insert(({name:?}.into(),{field:?}.into()),|v|{{{projected}}});").unwrap();
                let get = native_input(
                    &loaded,
                    &paths,
                    &p.ty,
                    &format!(
                        "values.get({:?}).ok_or(\"missing interface output\")?.clone()",
                        p.name
                    ),
                )?;
                write!(
                    fields,
                    "r#{}: ::organon_theoria::Source::from({get}),",
                    p.name
                )
                .unwrap();
            }

            writeln!(code,"registry.interfaces.insert({name:?}.into(),|values,_contracts|Ok(::organon_theoria::attribute::Value::new({target}{{{fields}}}))); ").unwrap();
        }
        if let syntax::Definition::DeclareElement(e) = d
            && e.tag
        {
            let mut build = format!("{target}::builder()");
            for p in e.inputs() {
                let read = native_input(&loaded, &paths, &p.ty, "v")?;
                let value = if e.is_optional_input(&p.name) {
                    format!("match values.next().flatten(){{Some(v)=>Some({read}),None=>None}}")
                } else {
                    format!("{{let v=values.next().flatten().ok_or(\"missing input\")?;{read}}}")
                };
                write!(build, ".r#{}({value})", p.name).unwrap();
            }
            if let Some(c) = e.children.as_ref() {
                let ty = value_type(&loaded, &paths, &c.kind)?;
                write!(build,".__children(children.into_iter().map(|v|v.get::<{ty}>()).collect::<Result<Vec<_>,_>>()?)").unwrap();
            }
            build.push_str(".build(&reactive).map_err(|e|e.to_string())?");
            let mut values = String::new();
            for o in e.outputs() {
                let get = format!(
                    "built.r#{}_fixed().ok_or(\"expected fixed preview\")?.clone()",
                    o.name
                );
                let value = erase_errors(
                    &loaded,
                    &o.ty.errors,
                    &get,
                    "::organon_preview::evaluation::Value::new",
                )?;
                write!(values, "outputs.insert({:?}.into(),{value});", o.name).unwrap();
            }
            let mut interfaces = String::new();
            for i in e.implements() {
                let sig = loaded
                    .definitions
                    .definitions()
                    .find_map(|d| match d {
                        syntax::Definition::Interface(v) if v.name == i => Some(v),
                        _ => None,
                    })
                    .unwrap();
                let id = &loaded.origins[i];
                let interface = marker(id, &paths[&id.source])?;
                write!(interfaces,
                    "{{let provided: {interface}=::organon_theoria::Provide::provide(&built);interfaces.insert({:?}.into(),std::collections::BTreeMap::from([",
                    names[i]
                ).unwrap();
                for p in &sig.outputs {
                    let get = format!(
                        "provided.r#{}.as_fixed().ok_or(\"expected fixed preview\")?.clone()",
                        p.name
                    );
                    let value = erase_errors(
                        &loaded,
                        &p.ty.errors,
                        &get,
                        "::organon_preview::evaluation::Value::new",
                    )?;
                    write!(interfaces, "({:?}.into(),{value}),", p.name).unwrap();
                }
                interfaces.push_str("]));}");
            }
            let inputs = e
                .inputs()
                .map(|p| format!("{:?}", p.name))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(code,"registry.elements.insert({name:?}.into(),::organon_preview::evaluation::Entry{{inputs:vec![{inputs}],evaluate:|values,children,_contracts|{{let reactive=::organon_theoria::reactive::Reactive::new();let mut values=values.into_iter();let _=(&mut values,&children);let built={build};let mut outputs=std::collections::BTreeMap::new();{values}let mut interfaces=std::collections::BTreeMap::new();let _=&mut interfaces;{interfaces}Ok(::organon_preview::evaluation::Value::new(built).with_object(::organon_preview::evaluation::Object{{outputs,interfaces}}))}}}});").unwrap();
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
        &organon_cargo::track_files(loaded.source_order.iter().chain(sources.tracked_paths()))?
            .to_string(),
    );
    organon_cargo::connect_library_paths(&code, None)
}

fn native_input(
    loaded: &loader::Loaded<std::path::PathBuf>,
    paths: &BTreeMap<std::path::PathBuf, String>,
    v: &core_types::ValueType,
    expr: &str,
) -> Result<String, Error> {
    let t = value_type(loaded, paths, &v.kind)?;
    Ok(match v.errors.as_slice() {
        [] => format!("({expr}).map_err(|e|e.to_string())?.get::<{t}>()?"),
        [reason] => {
            let reason_ty = value_type(loaded, paths, reason)?;
            let id = &loaded.origins[reason.spelling()];
            let id = symbol(&id.source, &id.name)?;
            format!(
                "match {expr}{{Ok(v)=>Ok(v.get::<{t}>()?),Err(::organon_theoria::flow::Error::Failure(e))=>Err(e.as_contract({id:?},_contracts).map_err(|e|e.to_string())?.ok_or_else(||format!(\"undeclared Organon failure {{}}\", e.kind))?.get::<{reason_ty}>()?),Err(e)=>return Err(e.to_string().into())}}"
            )
        }
        errors => {
            let set = organon_theoria_compiler::library::error_set(loaded, paths, errors)?;
            let reasons = errors
                .iter()
                .map(|t| {
                    let id = &loaded.origins[t.spelling()];
                    symbol(&id.source, &id.name)
                })
                .collect::<Result<Vec<_>, _>>()?;
            format!(
                "match {expr}{{Ok(v)=>Ok(v.get::<{t}>()?),Err(::organon_theoria::flow::Error::Failure(e))=>{{let e=e.accept(&{reasons:?},_contracts).map_err(|e|e.to_string())?;Err(<{set}>::checked(e,_contracts).map_err(|e|e.to_string())?)}},Err(e)=>return Err(e.to_string().into())}}"
            )
        }
    })
}
fn erase_errors(
    loaded: &loader::Loaded<std::path::PathBuf>,
    errors: &[core_types::TypeName],
    expr: &str,
    map: &str,
) -> Result<String, Error> {
    Ok(match errors {
        [] => format!("Ok(({map})({expr}))"),
        [reason] => {
            let id = &loaded.origins[reason.spelling()];
            let id = symbol(&id.source, &id.name)?;
            format!(
                "({expr}).map({map}).map_err(|v|::organon_theoria::flow::Error::Failure(::organon_theoria::flow::Failure{{kind:{id:?}.into(),value: ::organon_theoria::attribute::Value::new(v)}}))"
            )
        }
        _ => format!(
            "({expr}).map({map}).map_err(|e|::organon_theoria::flow::Error::Failure(e.into_failure()))"
        ),
    })
}
