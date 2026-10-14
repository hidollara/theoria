use crate::ir::{Argument, Arguments};
use crate::types::{Input, Port, Type, ValueType};
use crate::{
    Error,
    bind::{self, Binding, Construction},
    element::type_name,
};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

#[derive(Clone)]
pub(crate) struct Signature {
    pub inputs: Vec<Port>,
    pub external: bool,
}

pub(crate) fn construct(
    name: &str,
    element: &str,
    signature: &Signature,
    arguments: &Arguments,
    bindings: &mut Vec<Input>,
    plans: &mut Vec<Binding>,
) -> Result<(), Error> {
    let mut setup = String::new();
    let mut references = vec![];
    let mut builder = format!("super::r#{}::builder()", element);
    for port in &signature.inputs {
        let value = &arguments[&port.name];
        references.extend(value.references());
        let source = if let Argument::Interpreted {
            marker,
            text,
            optional,
        } = value
        {
            let variable = format!("attribute_{name}_{}", port.name);
            let interpret = if port.ty.fallible {
                "interpret_fallible"
            } else {
                "interpret"
            };
            writeln!(setup,"let {variable}=<{marker} as ::organon_theoria::Type>::{interpret}({text:?},&attributes)?;attributes.dependencies({name:?},{variable}.dependencies.clone());").unwrap();
            let expression = "source.read(cx).into_cloned()";
            let expression = if *optional {
                format!("Some({expression})")
            } else {
                expression.into()
            };
            format!(
                "{{let source={variable}.source.clone();::organon_theoria::Calculation::new(move|cx|{expression}).bind_in(context,true)?}}"
            )
        } else {
            argument(value, &port.ty, bindings, plans)
                .map_err(|e| Error(format!("{} input {:?}: {e}", element, port.name)))?
        };
        write!(builder, ".r#{}({source})", port.name).unwrap();
    }
    if signature.external {
        builder.push_str(".build_in(context)?");
    } else {
        builder.push_str(".__build(context)?");
    }
    let kind = Type::Element(element.to_owned());
    bindings.push(Input {
        name: name.to_owned(),
        kind: kind.clone(),
    });
    plans.push(Binding {
        name: name.to_owned(),
        provided: false,
        inline: false,
        kind,
        fallible: false,
        expression: String::new(),
        references,
        constructed: Some(Construction::Deferred {setup,code:format!(
            "let instance_{name} = {builder};\nlet source_{name} = ::organon_theoria::Calculation::new(move |_| instance_{name}.clone()).bind_in(context, false)?;\n"
        )}),
    });
    Ok(())
}

fn argument(
    value: &Argument,
    ty: &ValueType,
    _bindings: &[Input],
    plans: &[Binding],
) -> Result<String, Error> {
    if let Argument::Coerce {
        name,
        interface,
        source_fallible,
        target_fallible,
    } = value
    {
        let source = bind::source(name, plans);
        let provided = if *source_fallible {
            "::organon_theoria::Provide::provide(value.as_ref().map_err(|e| *e)?)"
        } else {
            "::organon_theoria::Provide::provide(&*value)"
        };
        let expression = if *target_fallible {
            format!("Ok({provided})")
        } else {
            provided.into()
        };
        let target = type_name(&Type::Element(interface.clone()));
        let target = if *target_fallible {
            format!("Result<{target}, &'static str>")
        } else {
            target
        };
        return Ok(format!(
            "{{ let source = {source}; ::organon_theoria::Calculation::new(move |cx| -> {target} {{ let value = source.read(cx); {expression} }}).bind_in(context,true)? }}"
        ));
    }
    if let Argument::Reference { name, lift_result } = value {
        let source = bind::source(name, plans);
        if !lift_result {
            return Ok(source);
        }
        return Ok(format!(
            "{{ let source = {source}; ::organon_theoria::Calculation::new(move |cx| -> Result<{}, &'static str> {{ Ok(source.read(cx).into_cloned()) }}).bind_in(context,true)? }}",
            type_name(&ty.kind)
        ));
    }
    let mut captures = String::new();
    let mut reads = String::new();
    let mut seen = HashSet::new();
    for name in value.references() {
        if !seen.insert(name.clone()) {
            continue;
        }
        writeln!(
            captures,
            "let source_{name} = {};",
            bind::source(&name, plans)
        )
        .unwrap();
        writeln!(reads, "let binding_{name} = &*source_{name}.read(cx);").unwrap();
    }
    let expression = match value {
        Argument::Interpreted { .. } => unreachable!(),
        Argument::Native { expression, .. } => expression.clone(),
        Argument::Reference { .. } | Argument::Coerce { .. } => unreachable!(),
    };
    let result_type = if ty.fallible {
        format!("Result<{}, &'static str>", type_name(&ty.kind))
    } else {
        type_name(&ty.kind)
    };
    let expression = if ty.fallible {
        format!("Ok({expression})")
    } else {
        expression
    };
    Ok(format!(
        "{{ {captures} ::organon_theoria::Calculation::new(move |cx| -> {result_type} {{ let _ = cx; {reads} {expression} }}).bind_in(context,true)? }}"
    ))
}

/// Select one explicitly named output for Bind. Keeping Sources here preserves a child's
/// inline policy and observes replacement of an Element-valued parent.
pub(crate) struct Projections {
    names: HashMap<(String, String), String>,
    used: HashSet<String>,
}

impl Projections {
    pub fn new(used: HashSet<String>) -> Self {
        Self {
            names: HashMap::new(),
            used,
        }
    }

    pub fn emit(
        &mut self,
        parent: &str,
        field: &str,
        output_type: &ValueType,
        parent_fallible: bool,
        bindings: &mut Vec<Input>,
        plans: &mut Vec<Binding>,
    ) -> String {
        let key = (parent.to_owned(), field.to_owned());
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        let fallible = parent_fallible || output_type.fallible;
        let child = format!("value.r#{field}()");
        let selected = if parent_fallible {
            let child = if output_type.fallible {
                child
            } else {
                format!(
                    "{child}.project(|value| ::organon_theoria::Source::from(Ok(value.clone())))"
                )
            };
            format!(
                "match value {{ Ok(value) => {child}, Err(error) => ::organon_theoria::Source::from(Err(*error)) }}"
            )
        } else {
            child
        };
        let expression = format!(
            "{}.project(|value| {{ {selected} }})",
            bind::source(parent, plans)
        );
        let mut name = format!("__organon_output_{}", self.used.len());
        while !self.used.insert(name.clone()) {
            name.push('_');
        }
        bindings.push(Input {
            name: name.to_owned(),
            kind: output_type.kind.clone(),
        });
        plans.push(Binding {
            name: name.to_owned(),
            provided: false,
            inline: false,
            kind: output_type.kind.clone(),
            fallible,
            expression: String::new(),
            references: vec![parent.to_owned()],
            constructed: Some(Construction::Source(expression)),
        });
        self.names.insert(key, name.clone());
        name
    }
}
