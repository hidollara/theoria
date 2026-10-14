//! Ask each native const parser for requirements, then select concrete Rust port types.
use super::*;
use s::effects::Effect;

#[derive(Default)]
pub(super) struct Effects {
    pub code: String,
    pub aliases: String,
    pub outputs: BTreeMap<(String, String), usize>,
    /// Complete contracts only where native dependencies cannot change the error set.
    pub resolved_outputs: BTreeMap<(String, String), s::TypeContract>,
    pub fallible_bindings: std::collections::BTreeSet<(String, String)>,
    pub fallible_bodies: std::collections::BTreeSet<String>,
}
impl Effects {
    pub fn output(&self, element: &str, output: &str) -> usize {
        self.outputs[&(element.to_owned(), output.to_owned())]
    }
    pub fn alias(&self, element: &str, output: &str) -> String {
        format!("__result_{}", self.output(element, output))
    }
}

pub(super) fn generate(program: &s::Program, ctx: &Context<'_>) -> Result<Effects, Error> {
    let graph = s::effects::graph(program);
    let mut code = String::new();
    for (i, a) in graph.attributes.iter().enumerate() {
        let marker = ctx.marker(a.kind.name())?;
        let size = a.bindings.len();
        write!(code,"const __ATTRIBUTE_DEPS_{i}:[bool;{size}]={{use ::organon_theoria::DefaultParse as _; {marker}::__ORGANON_PARSER.dependencies::<{size}>({:?},&[",a.text).unwrap();
        for (name, (kind, _)) in &a.bindings {
            write!(code,"::organon_theoria::attribute::BindingType{{name:{name:?},contract: ::organon_theoria::attribute::contract({:?},__ORGANON_CONTRACTS)}},",kind.name()).unwrap();
        }
        code.push_str("]) };\n");
    }
    write!(
        code,
        "const __ORGANON_EFFECTS:[[bool;{}];{}]=::organon_theoria::effects::evaluate(&[",
        graph.reasons.len(),
        graph.nodes.len()
    )
    .unwrap();
    for n in &graph.nodes {
        let node = match n {
            Effect::Union(v) => format!("Union(&{v:?})"),
            Effect::Reasons(v) => format!("Reasons(&{v:?})"),
            Effect::Attribute(i) => format!(
                "References{{selected:&__ATTRIBUTE_DEPS_{i},sources:&{:?}}}",
                graph.attributes[*i]
                    .bindings
                    .values()
                    .map(|(_, n)| *n)
                    .collect::<Vec<_>>()
            ),
            Effect::Filter { source, reasons } => {
                format!("Filter{{source:{source},reasons:&{reasons:?}}}")
            }
            Effect::When { condition, value } => {
                format!("When{{condition:{condition},value:{value}}}")
            }
        };
        write!(code, "::organon_theoria::effects::Node::{node},").unwrap();
    }
    code.push_str("]);\n");
    for check in &graph.checks {
        for i in 0..graph.reasons.len() {
            if !check.allowed.contains(&i) {
                writeln!(
                    code,
                    "const _:()=assert!(!__ORGANON_EFFECTS[{}][{i}],{:?});",
                    check.source, check.name
                )
                .unwrap();
            }
        }
    }
    let mut aliases = String::new();
    let mut outputs = BTreeMap::new();
    let mut emitted = BTreeMap::new();
    for body in &program.bodies {
        for port in &program.contracts[&body.name].outputs {
            let key = (body.name.clone(), port.name.to_string());
            let mut id = graph.outputs[&key];
            // Multiple public names for the same binding keep the same Source.
            let mut seen = std::collections::BTreeSet::new();
            while seen.insert(id) {
                match &graph.nodes[id] {
                    Effect::Union(v) if v.len() == 1 => id = v[0],
                    _ => break,
                }
            }
            // Distinct success types may have the same effect node (e.g. an empty set).
            let alias_key = (id, port.ty.kind().name());
            if let Some(alias) = emitted.get(&alias_key) {
                outputs.insert(key, *alias);
                continue;
            }
            let alias = emitted.len();
            emitted.insert(alias_key, alias);
            let mut reasons = BTreeMap::new();
            for (i, r) in graph.reasons.iter().enumerate() {
                let origin = &ctx.loaded.origins[r.name()];
                reasons.insert(
                    organon_cargo::identity(&origin.source, &origin.name)?,
                    (i, ctx.marker(r.name())?),
                );
            }
            let set = reasons.into_values().rev().fold("()".to_owned(),|tail,(i,m)|format!("::organon_theoria::effects::Include<{{super::__ORGANON_EFFECTS[{id}][{i}]}},{m},{tail}>"));
            writeln!(aliases,"pub type __errors_{alias}=<{set} as ::organon_theoria::effects::Normalize>::Set;\npub type __result_{alias}=<__errors_{alias} as ::organon_theoria::effects::Port<r#{}>>::Value;",port.ty.kind().name()).unwrap();
            outputs.insert(key, alias);
        }
    }
    let select_all = |selected| {
        graph
            .attributes
            .iter()
            .map(|a| vec![selected; a.bindings.len()])
            .collect::<Vec<_>>()
    };
    let lower = graph.evaluate(&select_all(false));
    let upper = graph.evaluate(&select_all(true));
    let mut resolved_outputs = BTreeMap::new();
    for (name, contract) in &program.contracts {
        for port in &contract.outputs {
            let key = (name.clone(), port.name.to_string());
            let id = graph.outputs[&key];
            if lower[id] == upper[id] {
                resolved_outputs.insert(
                    key,
                    s::TypeContract {
                        kind: port.ty.kind().clone(),
                        errors: graph
                            .reasons
                            .iter()
                            .zip(&lower[id])
                            .filter(|(_, present)| **present)
                            .map(|(r, _)| r.clone())
                            .collect(),
                    },
                );
            }
        }
    }
    let potential = upper
        .iter()
        .map(|set| set.iter().any(|v| *v))
        .collect::<Vec<_>>();
    let fallible_bindings = program
        .bodies
        .iter()
        .flat_map(|b| {
            b.bindings
                .iter()
                .zip(&graph.bindings[&b.name])
                .filter(|(_, id)| potential[**id])
                .map(|(v, _)| (b.name.clone(), v.name.clone()))
        })
        .collect();
    let fallible_bodies = graph
        .bindings
        .iter()
        .filter(|(_, nodes)| nodes.iter().any(|n| potential[*n]))
        .map(|(name, _)| name.clone())
        .collect();
    Ok(Effects {
        code,
        aliases,
        outputs,
        resolved_outputs,
        fallible_bindings,
        fallible_bodies,
    })
}
