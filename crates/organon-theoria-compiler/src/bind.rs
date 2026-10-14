//! Persistent value graphs for straight-line, infallible construction.
use crate::element::type_name;
use crate::types::{Port, Type};
use std::{collections::BTreeSet, fmt::Write};

pub(crate) enum Construction {
    Expression(String),
    Instance { prepare: String, build: String },
    Source(String),
}

pub(crate) struct OutputBinding {
    pub name: String,
    pub binding: String,
    pub kind: Type,
}
impl OutputBinding {
    pub fn result_type(&self) -> String {
        type_name(&self.kind)
    }
}

pub(crate) struct Binding {
    pub name: String,
    pub kind: Type,
    pub references: Vec<String>,
    pub construction: Construction,
}

impl Binding {
    pub fn result_type(&self) -> String {
        type_name(&self.kind)
    }
}

pub(crate) fn generate(
    inputs: &[Port],
    plans: &[Binding],
    outputs: &[OutputBinding],
    output: &mut String,
) {
    declarations(inputs, outputs, output);
    if plans.iter().any(
        |p| matches!(p.construction,Construction::Instance{ref prepare,..} if !prepare.is_empty()),
    ) {
        generate_with_attributes(inputs, plans, outputs, output);
        return;
    }
    output.push_str("#[allow(clippy::let_and_return, clippy::needless_question_mark)]\nfn bind_graph(context: &mut impl ::organon_theoria::BindingContext, inputs: &Sources) -> Result<Outputs, ::organon_theoria::BuildError> {\nlet _ = (&context, inputs);\n");
    for input in inputs {
        writeln!(
            output,
            "let source_{} = inputs.r#{}.clone();\nlet _ = &source_{};",
            input.name, input.name, input.name
        )
        .unwrap();
    }
    for plan in plans {
        emit_plan(plan, output);
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

fn declarations(inputs: &[Port], outputs: &[OutputBinding], output: &mut String) {
    output.push_str("struct Sources {\n");
    for input in inputs {
        writeln!(
            output,
            "r#{}: ::organon_theoria::Source<{}>,",
            input.name,
            type_name(&input.ty)
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
    output.push_str("}\n");
}

pub(crate) fn source(name: &str) -> String {
    format!("source_{name}.clone()")
}

fn dependencies(plan: &Binding) -> BTreeSet<String> {
    plan.references.iter().cloned().collect()
}

fn evaluation(plan: &Binding, dependencies: &BTreeSet<String>, output: &mut String) {
    output.push_str("{\n");
    for name in dependencies {
        writeln!(output, "let value_{name} = &*source_{name}.read(cx);").unwrap();
        writeln!(output, "let binding_{name} = value_{name};").unwrap();
    }
    let Construction::Expression(expression) = &plan.construction else {
        unreachable!()
    };
    writeln!(output, "{expression}\n}}").unwrap();
}

fn emit_plan(plan: &Binding, output: &mut String) {
    let name = &plan.name;
    match &plan.construction {
        Construction::Expression(_) => {}
        Construction::Instance { build, .. } => {
            output.push_str(build);
            writeln!(output, "let _ = &source_{name};").unwrap();
            return;
        }
        Construction::Source(expression) => {
            writeln!(
                output,
                "let source_{name} = {expression};\nlet _ = &source_{name};"
            )
            .unwrap();
            return;
        }
    }
    let dependencies = dependencies(plan);
    writeln!(output, "let source_{name} = {{").unwrap();
    for dependency in &dependencies {
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
    evaluation(plan, &dependencies, output);
    writeln!(output, "\n}}).bind_in(context)?\n}};").unwrap();
}

fn generate_with_attributes(
    inputs: &[Port],
    plans: &[Binding],
    outputs: &[OutputBinding],
    out: &mut String,
) {
    out.push_str("#[allow(unused_variables,unused_mut,clippy::let_and_return,clippy::needless_question_mark)]\nfn bind_graph(context:&mut impl ::organon_theoria::BindingContext,inputs:&Sources)->Result<Outputs,::organon_theoria::BuildError>{let mut attributes=::organon_theoria::attribute::Scope::new(super::__organon_registry());\n");
    out.push_str(
        "const ATTRIBUTE_SCHEMA: &[::organon_theoria::attribute::BindingType<'static>] = &[\n",
    );
    for (name, kind) in inputs
        .iter()
        .map(|i| (&i.name, &i.ty))
        .chain(plans.iter().map(|p| (&p.name, &p.kind)))
    {
        let Type(kind) = kind;
        writeln!(out,"::organon_theoria::attribute::BindingType{{name:{name:?},contract: ::organon_theoria::attribute::contract({kind:?},super::__ORGANON_CONTRACTS)}},").unwrap();
    }
    out.push_str("];\n");
    for (name, kind) in inputs
        .iter()
        .map(|i| (&i.name, &i.ty))
        .chain(plans.iter().map(|p| (&p.name, &p.kind)))
    {
        let Type(kind) = kind;
        writeln!(out, "attributes.declare({name:?}, ::organon_theoria::attribute::contract({kind:?},super::__ORGANON_CONTRACTS));").unwrap();
    }
    for i in inputs {
        let n = &i.name;
        writeln!(out,"let source_{n}=inputs.r#{n}.clone();attributes.set({n:?},{{let source=source_{n}.clone();::organon_theoria::Calculation::new(move|cx|::organon_theoria::attribute::Value::new(source.read(cx).into_cloned())).into_deferred()}});").unwrap();
    }
    for p in plans {
        let n = &p.name;
        let ty = p.result_type();
        writeln!(
            out,
            "let mut slot_{n}: Option<::organon_theoria::Source<{ty}>>=None;"
        )
        .unwrap();
        writeln!(
            out,
            "attributes.dependencies({n:?},vec!{:?}.into_iter().map(str::to_owned));",
            p.references
        )
        .unwrap();
    }
    for p in plans {
        if let Construction::Instance { prepare, .. } = &p.construction {
            out.push_str(prepare);
        }
    }
    out.push_str("for name in attributes.order()? {match name.as_str(){\n");
    for p in plans {
        let n = &p.name;
        writeln!(out, "{n:?}=>{{").unwrap();
        // Dependency order guarantees that these slots already hold constructed Sources.
        for dependency in dependencies(p) {
            if plans.iter().any(|p| p.name == dependency) {
                writeln!(out, "let source_{dependency}=slot_{dependency}.as_ref().expect(\"dependency initialized\").clone();").unwrap();
            }
        }
        emit_plan(p, out);
        writeln!(out, "slot_{n}=Some(source_{n}.clone());").unwrap();
        writeln!(out,"attributes.set({n:?},{{let source=source_{n}.clone();::organon_theoria::Calculation::new(move|cx|{{let value=source.read(cx).into_cloned();::organon_theoria::attribute::Value::new(value)}}).into_deferred()}});}}").unwrap();
    }
    out.push_str("_=>unreachable!(),}}\n");
    out.push_str("Ok(Outputs{\n");
    for output in outputs {
        let name = &output.binding;
        let value = if inputs.iter().any(|input| input.name == *name) {
            source(name)
        } else {
            format!("slot_{name}.as_ref().expect(\"provided initialized\").clone()")
        };
        writeln!(out, "r#{}:{value},", output.name).unwrap();
    }
    out.push_str("})}\n");
}
