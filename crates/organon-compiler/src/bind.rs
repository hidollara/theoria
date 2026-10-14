//! Persistent value graphs. Fallibility and fixedness are independent properties.
use crate::element::type_name;
use organon_format::{Input, InputType};
use std::{collections::BTreeSet, fmt::Write};

pub(crate) enum Construction {
    Bound(String),
    Source(String),
}

pub(crate) struct Binding {
    pub name: String,
    pub export: bool,
    pub inline: bool,
    pub kind: InputType,
    pub statements: String,
    pub expression: String,
    pub references: Vec<String>,
    pub fallible: bool,
    /// An existing instance or output Source, without a second construction.
    pub constructed: Option<Construction>,
}

impl Binding {
    pub fn result_type(&self) -> String {
        let ty = type_name(&self.kind);
        if self.fallible {
            format!("Result<{ty}, &'static str>")
        } else {
            ty.to_owned()
        }
    }
}

pub(crate) fn generate(
    inputs: &[Input],
    plans: &[Binding],
    exports: &[Input],
    output: &mut String,
) {
    output.push_str("struct Sources {\n");
    for input in inputs {
        writeln!(
            output,
            "r#{}: ::organon::Source<{}>,",
            input.name,
            type_name(&input.kind)
        )
        .unwrap();
    }
    output.push_str("}\nstruct Exports {\n");
    for export in exports {
        let plan = plans.iter().find(|p| p.name == export.name).unwrap();
        writeln!(
            output,
            "r#{}: ::organon::Bound<{}>,",
            export.name,
            plan.result_type()
        )
        .unwrap();
    }
    output.push_str("}\n#[allow(clippy::let_and_return)]\nfn bind_graph(context: &mut impl ::organon::BindingContext, inputs: &Sources) -> Result<Exports, ::organon::BuildError> {\nlet _ = (&context, inputs);\n");
    for input in inputs {
        writeln!(
            output,
            "let source_{} = inputs.r#{}.clone();\nlet _ = &source_{};",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    for plan in plans {
        if let Some(construction) = &plan.constructed {
            let name = &plan.name;
            match construction {
                Construction::Bound(code) => {
                    output.push_str(code);
                    writeln!(output, "let source_{name} = bound_{name}.source();").unwrap();
                }
                Construction::Source(expression) => {
                    writeln!(output, "let source_{name} = {expression};").unwrap();
                }
            }
            writeln!(output, "let _ = &source_{name};").unwrap();
            continue;
        }
        if plan.inline && !plan.export {
            continue;
        }
        let name = &plan.name;
        let dependencies = dependencies(plan, plans);
        writeln!(output, "let bound_{name} = {{").unwrap();
        for dependency in dependencies.iter().filter(|name| !is_inline(name, plans)) {
            writeln!(
                output,
                "let source_{dependency} = source_{dependency}.clone();"
            )
            .unwrap();
        }
        writeln!(
            output,
            "::organon::output(move |cx| -> {} {{\nlet _ = cx;",
            plan.result_type()
        )
        .unwrap();
        evaluation(plan, plans, &dependencies, output);
        writeln!(output, "\n}}).bind_in(context, {})?\n}};", plan.inline).unwrap();
        if !plan.inline {
            writeln!(
                output,
                "let source_{name} = bound_{name}.source();\nlet _ = &source_{name};"
            )
            .unwrap();
        }
    }
    output.push_str("Ok(Exports {\n");
    for export in exports {
        writeln!(output, "r#{}: bound_{},", export.name, export.name).unwrap();
    }
    output.push_str("})\n}\n");
}

fn is_inline(name: &str, plans: &[Binding]) -> bool {
    plans
        .iter()
        .any(|p| p.name == name && p.inline && p.constructed.is_none())
}

pub(crate) fn source(name: &str, plans: &[Binding]) -> String {
    if !is_inline(name, plans) {
        return format!("source_{name}.clone()");
    }
    let plan = plans.iter().find(|p| p.name == name).unwrap();
    let dependencies = dependencies(plan, plans);
    let mut code = String::from("{\n");
    for dependency in dependencies.iter().filter(|n| !is_inline(n, plans)) {
        writeln!(
            code,
            "let source_{dependency} = source_{dependency}.clone();"
        )
        .unwrap();
    }
    writeln!(
        code,
        "::organon::output(move |cx| -> {} {{ let _ = cx;",
        plan.result_type()
    )
    .unwrap();
    evaluation(plan, plans, &dependencies, &mut code);
    code.push_str("}).into_inline()\n}");
    code
}

fn dependencies(plan: &Binding, plans: &[Binding]) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut pending = plan.references.clone();
    while let Some(name) = pending.pop() {
        if names.insert(name.clone())
            && let Some(binding) = plans
                .iter()
                .find(|p| p.name == name && p.inline && p.constructed.is_none())
        {
            pending.extend(binding.references.clone());
        }
    }
    names
}

fn binding(name: &str, plans: &[Binding], output: &mut String) {
    if plans.iter().any(|p| p.name == name && p.fallible) {
        writeln!(
            output,
            "let binding_{name} = value_{name}.as_ref().map_err(|e| *e)?;"
        )
        .unwrap();
    } else {
        writeln!(output, "let binding_{name} = value_{name};").unwrap();
    }
}

fn begin(plan: &Binding, output: &mut String) {
    if plan.fallible {
        writeln!(output, "(|| -> {} {{", plan.result_type()).unwrap();
    } else {
        output.push_str("{\n");
    }
}

fn end(plan: &Binding, output: &mut String) {
    output.push_str(&plan.statements);
    if plan.fallible {
        writeln!(output, "Ok({})\n}})()", plan.expression).unwrap();
    } else {
        writeln!(output, "{}\n}}", plan.expression).unwrap();
    }
}

fn evaluation(
    plan: &Binding,
    plans: &[Binding],
    dependencies: &BTreeSet<String>,
    output: &mut String,
) {
    begin(plan, output);
    for name in dependencies.iter().filter(|name| !is_inline(name, plans)) {
        writeln!(output, "let value_{name} = &*source_{name}.read(cx);").unwrap();
        binding(name, plans, output);
    }
    for inline in plans
        .iter()
        .filter(|p| p.inline && p.constructed.is_none() && dependencies.contains(&p.name))
    {
        let name = &inline.name;
        writeln!(output, "let inline_{name} = ").unwrap();
        begin(inline, output);
        end(inline, output);
        writeln!(output, ";\nlet value_{name} = &inline_{name};").unwrap();
        binding(name, plans, output);
    }
    end(plan, output);
}
