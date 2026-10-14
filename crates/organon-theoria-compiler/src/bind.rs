//! Persistent value graphs. Fallibility and fixedness are independent properties.
use crate::element::type_name;
use crate::types::{Input, Type};
use std::{collections::BTreeSet, fmt::Write};

pub(crate) enum Construction {
    Deferred { setup: String, code: String },
    Source(String),
}

pub(crate) struct OutputBinding {
    pub name: String,
    pub binding: String,
    pub kind: Type,
    pub fallible: bool,
}
impl OutputBinding {
    pub fn result_type(&self) -> String {
        let ty = type_name(&self.kind);
        if self.fallible {
            format!("Result<{ty}, &'static str>")
        } else {
            ty
        }
    }
}

pub(crate) struct Binding {
    pub name: String,
    pub provided: bool,
    pub inline: bool,
    pub kind: Type,
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
    outputs: &[OutputBinding],
    output: &mut String,
) {
    if plans.iter().any(
        |p| matches!(p.constructed,Some(Construction::Deferred{ref setup,..}) if !setup.is_empty()),
    ) {
        generate_deferred(inputs, plans, outputs, output);
        return;
    }
    output.push_str("struct Sources {\n");
    for input in inputs {
        writeln!(
            output,
            "r#{}: ::organon_theoria::Source<{}>,",
            input.name,
            type_name(&input.kind)
        )
        .unwrap();
    }
    output.push_str("}\nstruct Outputs {\n");
    for provided in outputs {
        writeln!(
            output,
            "r#{}: ::organon_theoria::Source<{}>,",
            provided.name,
            provided.result_type()
        )
        .unwrap();
    }
    output.push_str("}\n#[allow(clippy::let_and_return, clippy::needless_question_mark)]\nfn bind_graph(context: &mut impl ::organon_theoria::BindingContext, inputs: &Sources) -> Result<Outputs, ::organon_theoria::BuildError> {\nlet _ = (&context, inputs);\n");
    for input in inputs {
        writeln!(
            output,
            "let source_{} = inputs.r#{}.clone();\nlet _ = &source_{};",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    for plan in plans {
        emit_plan(plan, plans, output);
    }
    output.push_str("Ok(Outputs {\n");
    for provided in outputs {
        writeln!(
            output,
            "r#{}: source_{}.clone(),",
            provided.name, provided.binding
        )
        .unwrap();
    }
    output.push_str("})\n}\n");
}

fn is_inline(name: &str, plans: &[Binding]) -> bool {
    plans
        .iter()
        .any(|p| p.name == name && p.inline && !p.provided && p.constructed.is_none())
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
        "::organon_theoria::Calculation::new(move |cx| -> {} {{ let _ = cx;",
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
                .find(|p| p.name == name && p.inline && !p.provided && p.constructed.is_none())
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
    for inline in plans.iter().filter(|p| {
        p.inline && !p.provided && p.constructed.is_none() && dependencies.contains(&p.name)
    }) {
        let name = &inline.name;
        writeln!(output, "let inline_{name} = ").unwrap();
        begin(inline, output);
        end(inline, output);
        writeln!(output, ";\nlet value_{name} = &inline_{name};").unwrap();
        binding(name, plans, output);
    }
    end(plan, output);
}

fn emit_plan(plan: &Binding, plans: &[Binding], output: &mut String) {
    if let Some(construction) = &plan.constructed {
        let name = &plan.name;
        match construction {
            Construction::Deferred { code, .. } => {
                output.push_str(code);
            }
            Construction::Source(expression) => {
                writeln!(output, "let source_{name} = {expression};").unwrap();
            }
        }
        writeln!(output, "let _ = &source_{name};").unwrap();
        return;
    }
    if plan.inline && !plan.provided {
        return;
    }
    let name = &plan.name;
    let dependencies = dependencies(plan, plans);
    writeln!(output, "let source_{name} = {{").unwrap();
    for dependency in dependencies.iter().filter(|name| !is_inline(name, plans)) {
        writeln!(
            output,
            "let source_{dependency} = source_{dependency}.clone();"
        )
        .unwrap();
    }
    writeln!(
        output,
        "::organon_theoria::Calculation::new(move |cx| -> {} {{\nlet _ = cx;",
        plan.result_type()
    )
    .unwrap();
    evaluation(plan, plans, &dependencies, output);
    writeln!(output, "\n}}).bind_in(context, {})?\n}};", plan.inline).unwrap();
}

fn generate_deferred(
    inputs: &[Input],
    plans: &[Binding],
    outputs: &[OutputBinding],
    out: &mut String,
) {
    out.push_str("struct Sources {\n");
    for i in inputs {
        writeln!(
            out,
            "r#{}: ::organon_theoria::Source<{}>,",
            i.name,
            type_name(&i.kind)
        )
        .unwrap();
    }
    out.push_str("}\nstruct Outputs {\n");
    for e in outputs {
        writeln!(
            out,
            "r#{}: ::organon_theoria::Source<{}>,",
            e.name,
            e.result_type()
        )
        .unwrap();
    }
    out.push_str("}\n#[allow(unused_variables,unused_mut,clippy::let_and_return,clippy::needless_question_mark)]\nfn bind_graph(context:&mut impl ::organon_theoria::BindingContext,inputs:&Sources)->Result<Outputs,::organon_theoria::BuildError>{let mut attributes=::organon_theoria::attribute::Scope::new(super::__organon_registry());\n");
    for (name, ty) in inputs
        .iter()
        .map(|i| (&i.name, type_name(&i.kind)))
        .chain(plans.iter().map(|p| (&p.name, type_name(&p.kind))))
    {
        writeln!(out, "attributes.declare_typed::<{ty}>({name:?});").unwrap();
    }
    for p in plans.iter().filter(|p| p.fallible) {
        writeln!(out, "attributes.declare_fallible({:?});", p.name).unwrap();
    }
    for i in inputs {
        let n = &i.name;
        writeln!(out,"let source_{n}=inputs.r#{n}.clone();attributes.set({n:?},{{let source=source_{n}.clone();::organon_theoria::Calculation::new(move|cx|::organon_theoria::attribute::Value::new(source.read(cx).into_cloned())).into_inline()}});").unwrap();
    }
    for p in plans {
        let n = &p.name;
        let ty = p.result_type();
        writeln!(out,"let slot_{n}: ::std::rc::Rc<::std::cell::RefCell<Option<::organon_theoria::Source<{ty}>>>>=Default::default();let source_{n}={{let slot=slot_{n}.clone();::organon_theoria::Calculation::new(move|cx|{{let source=slot.borrow().as_ref().expect(\"binding initialized\").clone();source.read(cx).into_cloned()}}).into_inline()}};").unwrap();
        writeln!(
            out,
            "attributes.dependencies({n:?},vec!{:?}.into_iter().map(str::to_owned));",
            p.references
        )
        .unwrap();
    }
    for p in plans {
        if let Some(Construction::Deferred { setup, .. }) = &p.constructed {
            out.push_str(setup);
        }
    }
    let published: BTreeSet<_> = outputs.iter().map(|e| e.binding.as_str()).collect();
    for name in &published {
        let ty = outputs
            .iter()
            .find(|e| e.binding == *name)
            .unwrap()
            .result_type();
        writeln!(
            out,
            "let mut result_{name}: Option<::organon_theoria::Source<{ty}>>=None;"
        )
        .unwrap();
    }
    out.push_str("for name in attributes.order()? {match name.as_str(){\n");
    for p in plans {
        let n = &p.name;
        writeln!(out, "{n:?}=>{{").unwrap();
        // Construction order has initialized dependencies. Use their actual Sources
        // instead of the forward-reference placeholders when projecting outputs.
        for dependency in dependencies(p, plans) {
            if plans.iter().any(|p| p.name == dependency) {
                writeln!(out, "let source_{dependency}=slot_{dependency}.borrow().as_ref().expect(\"dependency initialized\").clone();").unwrap();
            }
        }
        if is_inline(n, plans) {
            writeln!(out, "let source_{n}={};", source(n, plans)).unwrap();
        } else {
            emit_plan(p, plans, out);
        }
        if published.contains(n.as_str()) {
            writeln!(out, "result_{n}=Some(source_{n}.clone());").unwrap();
        }
        writeln!(out, "*slot_{n}.borrow_mut()=Some(source_{n}.clone());").unwrap();
        let (setter, value) = if p.fallible {
            (
                "set_fallible",
                "value.map(::organon_theoria::attribute::Value::new)",
            )
        } else {
            ("set", "::organon_theoria::attribute::Value::new(value)")
        };
        writeln!(out,"attributes.{setter}({n:?},{{let source=source_{n}.clone();::organon_theoria::Calculation::new(move|cx|{{let value=source.read(cx).into_cloned();{value}}}).into_inline()}});}}").unwrap();
    }
    out.push_str("_=>unreachable!(),}}\n");
    for i in inputs {
        let n = &i.name;
        if published.contains(n.as_str()) {
            writeln!(out, "result_{n}=Some(source_{n});").unwrap();
        }
    }
    out.push_str("Ok(Outputs{\n");
    for e in outputs {
        writeln!(
            out,
            "r#{}:result_{}.as_ref().expect(\"provided initialized\").clone(),",
            e.name, e.binding
        )
        .unwrap();
    }
    out.push_str("})}\n");
}
