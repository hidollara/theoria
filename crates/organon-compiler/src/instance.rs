use crate::{
    Error,
    bind::{self, Binding, Construction},
    element::type_name,
    generate, lower,
};
use organon_format::{Input, InputType, Instance, Let, Port, PortType, Segment, Template};
use std::{
    collections::{HashMap, HashSet},
    fmt::Write,
};

#[derive(Clone)]
pub(crate) struct Signature {
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub interface: bool,
    pub implemented: bool,
}

pub(crate) fn construct(
    definition: &Let,
    instance: &Instance,
    signature: &Signature,
    bindings: &mut Vec<Input>,
    plans: &mut Vec<Binding>,
) -> Result<(), Error> {
    if signature.interface && !signature.implemented {
        return Err(Error(format!(
            "interface {:?} needs an implementation binding in organon::import!",
            instance.element
        )));
    }
    for (name, _) in &instance.inputs {
        if !signature.inputs.iter().any(|p| p.name == *name) {
            return Err(Error(format!(
                "{}: unknown input {name:?}",
                instance.element
            )));
        }
    }
    let mut builder = format!("super::r#{}::builder()", instance.element);
    for port in &signature.inputs {
        let value = instance
            .inputs
            .iter()
            .find(|(n, _)| *n == port.name)
            .ok_or_else(|| {
                Error(format!(
                    "{}: missing input {:?}",
                    instance.element, port.name
                ))
            })?;
        let source = argument(&value.1, &port.ty, bindings, plans)?;
        write!(builder, ".r#{}({source})", port.name).unwrap();
    }
    if signature.interface {
        write!(
            builder,
            ".build_in(context, &super::__organon_impl_{}())?",
            instance.element
        )
        .unwrap();
    } else {
        builder.push_str(".__build(context)?");
    }
    let name = &definition.name;
    let kind = InputType::Element(instance.element.clone());
    bindings.push(Input {
        name: name.clone(),
        kind: kind.clone(),
    });
    plans.push(Binding {
        name: name.clone(),
        export: definition.export,
        inline: false,
        kind,
        fallible: false,
        statements: String::new(),
        expression: String::new(),
        references: vec![],
        constructed: Some(Construction::Bound(format!(
            "let instance_{name} = {builder};\nlet bound_{name} = ::organon::output(move |_| instance_{name}.clone()).bind_in(context, false)?;\n"
        ))),
    });
    Ok(())
}

fn argument(
    value: &str,
    ty: &PortType,
    bindings: &[Input],
    plans: &[Binding],
) -> Result<String, Error> {
    let template = Template::parse(value).map_err(|e| Error(e.to_string()))?;
    if let Some(name) = template.reference() {
        let binding = bindings
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| Error(format!("unknown argument {name:?}")))?;
        let fallible = plans.iter().any(|p| p.name == name && p.fallible);
        if binding.kind != ty.kind
            && (matches!(binding.kind, InputType::Element(_))
                || matches!(ty.kind, InputType::Element(_)))
        {
            return Err(Error(format!(
                "argument {name:?}: expected {}, got {}",
                ty.kind.spelling(),
                binding.kind.spelling()
            )));
        }
        if binding.kind == ty.kind {
            let source = bind::source(name, plans);
            if fallible == ty.fallible {
                return Ok(source);
            }
            if fallible {
                return Err(Error(format!(
                    "argument {name:?} is Result<{}>; this input requires {} (explicit error handling is required)",
                    type_name(&ty.kind),
                    type_name(&ty.kind)
                )));
            }
            return Ok(format!(
                "{{ let source = {source}; ::organon::output(move |cx| -> Result<{}, &'static str> {{ Ok(source.read(cx).into_cloned()) }}).into_inline() }}",
                type_name(&ty.kind)
            ));
        }
    }
    let mut captures = String::new();
    let mut reads = String::new();
    let mut seen = HashSet::new();
    for segment in template.segments() {
        if let Segment::Reference(name) = segment {
            if !seen.insert(name) {
                continue;
            }
            let input = bindings
                .iter()
                .find(|b| b.name == *name)
                .ok_or_else(|| Error(format!("unknown argument reference {name:?}")))?;
            if !matches!(input.kind, InputType::Scalar | InputType::String) {
                return Err(Error(format!("cannot interpolate resource {name:?}")));
            }
            if plans.iter().any(|p| p.name == *name && p.fallible) {
                return Err(Error(format!(
                    "argument reference {name:?} requires explicit Result handling"
                )));
            }
            writeln!(
                captures,
                "let source_{name} = {};",
                bind::source(name, plans)
            )
            .unwrap();
            writeln!(reads, "let binding_{name} = &*source_{name}.read(cx);").unwrap();
        }
    }
    let (expression, can_fail) = match &ty.kind {
        InputType::String => (generate::expand(&template), false),
        InputType::Scalar => {
            let value = lower::scalar(value, bindings)?;
            if let crate::ir::Value::Literal(v) = &value
                && !v.is_finite()
            {
                return Err(Error("scalar argument must be finite".into()));
            }
            let fails = matches!(value, crate::ir::Value::Template(_));
            (generate::scalar(&value), fails)
        }
        InputType::Color | InputType::ColorField => {
            let value = lower::color(value, bindings)?;
            let fails = matches!(value, crate::ir::Value::Template(_));
            let color = generate::color(&value);
            (
                if ty.kind == InputType::ColorField {
                    format!("::theoria_canvas::ColorField::Solid({color})")
                } else {
                    color
                },
                fails,
            )
        }
        _ => {
            return Err(Error(format!(
                "{} input requires a single matching resource reference",
                type_name(&ty.kind)
            )));
        }
    };
    if can_fail && !ty.fallible {
        return Err(Error(
            "fallible argument conversion requires a Result input".into(),
        ));
    }
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
        "{{ {captures} ::organon::output(move |cx| -> {result_type} {{ let _ = cx; {reads} {expression} }}).into_inline() }}"
    ))
}

/// Resolve an output chain lazily. Keeping Sources here preserves a child's
/// inline policy and observes replacement of an Element-valued parent.
pub(crate) struct Projections {
    names: HashMap<String, String>,
    used: HashSet<String>,
}

impl Projections {
    pub fn new(used: HashSet<String>) -> Self {
        Self {
            names: HashMap::new(),
            used,
        }
    }

    pub fn resolve(
        &mut self,
        path: &str,
        signatures: &HashMap<String, Signature>,
        bindings: &mut Vec<Input>,
        plans: &mut Vec<Binding>,
    ) -> Result<String, Error> {
        let mut segments = path.split('.');
        let mut parent = segments.next().unwrap().to_owned();
        let mut prefix = parent.clone();
        for field in segments {
            prefix.push('.');
            prefix.push_str(field);
            if let Some(name) = self.names.get(&prefix) {
                parent = name.clone();
                continue;
            }
            let input = bindings
                .iter()
                .find(|b| b.name == parent)
                .ok_or_else(|| Error(format!("unknown reference {parent:?}")))?;
            let InputType::Element(element) = &input.kind else {
                return Err(Error(format!(
                    "get requires an element; {parent:?} has type {}",
                    input.kind.spelling()
                )));
            };
            let signature = signatures
                .get(element)
                .ok_or_else(|| Error(format!("unknown element type {element:?}")))?;
            let port = signature
                .outputs
                .iter()
                .find(|p| p.name == field)
                .ok_or_else(|| Error(format!("{element}: unknown or private output {field:?}")))?;
            let parent_fallible = plans.iter().any(|p| p.name == parent && p.fallible);
            let fallible = parent_fallible || port.ty.fallible;
            let child = format!("value.r#{field}_source()");
            let selected = if parent_fallible {
                let child = if port.ty.fallible {
                    child
                } else {
                    format!("{child}.project(|value| ::organon::Source::from(Ok(value.clone())))")
                };
                format!(
                    "match value {{ Ok(value) => {child}, Err(error) => ::organon::Source::from(Err(*error)) }}"
                )
            } else {
                child
            };
            let expression = format!(
                "{}.project(|value| {{ {selected} }})",
                bind::source(&parent, plans)
            );
            let mut name = format!("__organon_output_{}", self.used.len());
            while !self.used.insert(name.clone()) {
                name.push('_');
            }
            bindings.push(Input {
                name: name.clone(),
                kind: port.ty.kind.clone(),
            });
            plans.push(Binding {
                name: name.clone(),
                export: false,
                inline: false,
                kind: port.ty.kind.clone(),
                fallible,
                statements: String::new(),
                expression: String::new(),
                references: vec![],
                constructed: Some(Construction::Source(expression)),
            });
            self.names.insert(prefix.clone(), name.clone());
            parent = name;
        }
        Ok(parent)
    }
}
