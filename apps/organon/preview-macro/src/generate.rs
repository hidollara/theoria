//! Generate app-owned adapters from the same native contracts used by compiled code.
//! Nothing here is emitted by library!: the preview host explicitly requests it.
use organon_cargo::declaration_path as marker;
use organon_cargo::{CargoSources, identity as symbol};
use organon_core::Error;
use organon_core::{resolver, semantic as s};
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
    let program = &loaded.program;
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
        .origins
        .iter()
        .filter(|(key, _)| {
            !program
                .contracts
                .get(*key)
                .is_some_and(|c| c.kind == s::DefinitionKind::Element)
        })
        .map(|(_, id)| paths[&id.source].clone())
        .collect();
    for path in providers {
        writeln!(
            code,
            "registry.attributes.extend({path}::__organon::attribute_types());"
        )
        .unwrap();
    }
    for (key, p) in &program.predicates {
        let s::PredicateBody::Declared { errors } = &p.body else {
            continue;
        };
        let id = &loaded.origins[key];
        let name = &names[key];
        let target = marker(id, &paths[&id.source])?;
        let mut args = Vec::new();
        for input in p.inputs.iter() {
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
        let result = erase_errors(&loaded, errors, &call, "|v|v")?;
        writeln!(code,"registry.predicates.insert({name:?}.into(),|values,_contracts|{{let mut values=values.into_iter();{result}}});registry.predicates.insert({:?}.into(),registry.predicates[{name:?}]);",key).unwrap();
    }
    for (key, e) in &program.contracts {
        if e.kind == s::DefinitionKind::Element {
            continue;
        }
        let id = &loaded.origins[key];
        let name = &names[key];
        let target = marker(id, &paths[&id.source])?;
        writeln!(
            code,
            "registry.attributes.alias({:?},{name:?}).expect(\"registered type\");",
            key
        )
        .unwrap();
        writeln!(code,"registry.types.insert({name:?}.into(),::organon_preview::evaluation::TypeAdapter::of::<{target}>({name:?}));registry.types.insert({:?}.into(),registry.types[{name:?}].clone());",key).unwrap();
        if e.kind == s::DefinitionKind::Interface {
            let mut fields = String::new();
            for p in &e.outputs {
                let ty = p.ty.declared().expect("interface output");
                let field = &p.name;
                let read = format!(
                    "v.get::<{target}>()?.r#{field}_fixed().ok_or(\"expected a fixed interface output\")?.clone()"
                );
                let projected = erase_errors(
                    &loaded,
                    &ty.errors,
                    &read,
                    "::organon_preview::evaluation::Value::new",
                )?;
                writeln!(code,"registry.projections.insert(({name:?}.into(),{field:?}.into()),|v|{{{projected}}});").unwrap();
                let get = native_input(
                    &loaded,
                    &paths,
                    ty,
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
        if e.kind == (s::DefinitionKind::DeclareElement { tag: true }) {
            let mut build = format!("{target}::builder()");
            for p in e.inputs.iter() {
                let read = native_input(&loaded, &paths, &p.ty, "v")?;
                let value = if e.optional_inputs.contains(&p.name) {
                    format!("match values.next().flatten(){{Some(v)=>Some({read}),None=>None}}")
                } else {
                    format!("{{let v=values.next().flatten().ok_or(\"missing input\")?;{read}}}")
                };
                write!(build, ".r#{}({value})", p.name).unwrap();
            }
            if let Some(c) = e.children.as_ref() {
                let ty = value_type(&loaded, &paths, c.kind.name())?;
                write!(build,".__children(children.into_iter().map(|v|v.get::<{ty}>()).collect::<Result<Vec<_>,_>>()?)").unwrap();
            }
            build.push_str(".build(&reactive).map_err(|e|e.to_string())?");
            let mut values = String::new();
            for o in e.outputs.iter() {
                let get = format!(
                    "built.r#{}_fixed().ok_or(\"expected fixed preview\")?.clone()",
                    o.name
                );
                let value = erase_errors(
                    &loaded,
                    &o.ty.declared().expect("native output").errors,
                    &get,
                    "::organon_preview::evaluation::Value::new",
                )?;
                write!(values, "outputs.insert({:?}.into(),{value});", o.name).unwrap();
            }
            let mut interfaces = String::new();
            for i in e.implements.iter().map(String::as_str) {
                let sig = &program.contracts[i];
                let id = &loaded.origins[i];
                let interface = marker(id, &paths[&id.source])?;
                write!(interfaces,
                    "{{let provided: {interface}=::organon_theoria::Provide::provide(&built);interfaces.insert({:?}.into(),std::collections::BTreeMap::from([",
                    names[i]
                ).unwrap();
                for p in &sig.outputs {
                    let ty = p.ty.declared().expect("interface output");
                    let get = format!(
                        "provided.r#{}.as_fixed().ok_or(\"expected fixed preview\")?.clone()",
                        p.name
                    );
                    let value = erase_errors(
                        &loaded,
                        &ty.errors,
                        &get,
                        "::organon_preview::evaluation::Value::new",
                    )?;
                    write!(interfaces, "({:?}.into(),{value}),", p.name).unwrap();
                }
                interfaces.push_str("]));}");
            }
            let inputs = e
                .inputs
                .iter()
                .map(|p| format!("{:?}", p.name))
                .collect::<Vec<_>>()
                .join(",");
            writeln!(code,"registry.elements.insert({name:?}.into(),::organon_preview::evaluation::Entry{{inputs:vec![{inputs}],evaluate:|values,children,_contracts|{{let reactive=::organon_theoria::reactive::Reactive::new();let mut values=values.into_iter();let _=(&mut values,&children);let built={build};let mut outputs=std::collections::BTreeMap::new();{values}let mut interfaces=std::collections::BTreeMap::new();let _=&mut interfaces;{interfaces}Ok(::organon_preview::evaluation::Value::new(built).with_object(::organon_preview::evaluation::Object{{outputs,interfaces}}))}}}});").unwrap();
            writeln!(
                code,
                "registry.elements.insert({:?}.into(),registry.elements[{name:?}].clone());",
                key
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
    loaded: &resolver::Resolved<std::path::PathBuf>,
    paths: &BTreeMap<std::path::PathBuf, String>,
    v: &s::TypeContract,
    expr: &str,
) -> Result<String, Error> {
    let t = value_type(loaded, paths, v.kind.name())?;
    Ok(match v.errors.as_slice() {
        [] => format!("({expr}).map_err(|e|e.to_string())?.get::<{t}>()?"),
        [reason] => {
            let reason_ty = value_type(loaded, paths, reason.name())?;
            let id = &loaded.origins[reason.name()];
            let id = symbol(&id.source, &id.name)?;
            format!(
                "match {expr}{{Ok(v)=>Ok(v.get::<{t}>()?),Err(::organon_theoria::flow::Error::Failure(e))=>Err(e.as_contract({id:?},_contracts).map_err(|e|e.to_string())?.ok_or_else(||format!(\"undeclared Organon failure {{}}\", e.kind))?.get::<{reason_ty}>()?),Err(e)=>return Err(e.to_string().into())}}"
            )
        }
        errors => {
            let set = organon_theoria_compiler::library::error_set(
                loaded,
                paths,
                errors.iter().map(|t| t.name()),
            )?;
            let reasons = errors
                .iter()
                .map(|t| {
                    let id = &loaded.origins[t.name()];
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
    loaded: &resolver::Resolved<std::path::PathBuf>,
    errors: &[s::Type],
    expr: &str,
    map: &str,
) -> Result<String, Error> {
    Ok(match errors {
        [] => format!("Ok(({map})({expr}))"),
        [reason] => {
            let id = &loaded.origins[reason.name()];
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
