use crate::ir::{Argument, Arguments};
use crate::types::{Port, Type};
use crate::{
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
    pub native: bool,
}

/// Both linear and branching graphs call the receiving library's parser in the same phase.
pub(super) fn prepare(marker: &str, text: &str, scope: &str) -> String {
    format!(
        r#"{{
    const DEPENDENCIES:[bool;ATTRIBUTE_SCHEMA.len()]={{
        use ::organon_theoria::DefaultParse as _;
        {marker}::__ORGANON_PARSER.dependencies({text:?},ATTRIBUTE_SCHEMA)
    }};
    let build = const {{
        #[allow(unused_imports)]
        use ::organon_theoria::DefaultParse as _;
        const SIZE: usize = match {marker}::__ORGANON_PARSER.measure(
            {text:?},
            ::organon_theoria::attribute::ParseContext::new(ATTRIBUTE_SCHEMA),
        ) {{
            Ok(size) => size,
            Err(error) => panic!("{{}}", error.message()),
        }};
        {marker}::__ORGANON_PARSER.prepare::<SIZE>(
            {text:?},
            ::organon_theoria::attribute::ParseContext::new(ATTRIBUTE_SCHEMA),
        )
    }};
let attribute=build(&{scope})?;attribute.check_requirements(ATTRIBUTE_SCHEMA, &DEPENDENCIES)?;attribute}}"#
    )
}

pub(crate) fn construct(
    name: &str,
    element: &str,
    signature: &Signature,
    arguments: &Arguments,
    plans: &mut Vec<Binding>,
) {
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
            let attribute = prepare(marker, text, "attributes");
            writeln!(setup,"let {variable}={attribute};attributes.dependencies({name:?},{variable}.dependencies.clone());").unwrap();
            let expression = "source.read(cx).into_cloned()";
            let expression = if *optional {
                format!("Some({expression})")
            } else {
                expression.into()
            };
            format!(
                "{{let source={variable}.source.clone();::organon_theoria::Calculation::new(move|cx|{expression}).bind_in(context)?}}"
            )
        } else {
            argument(value, &port.ty)
        };
        write!(builder, ".r#{}({source})", port.name).unwrap();
    }
    if signature.native {
        builder.push_str(".build_in(context)?");
    } else {
        builder.push_str(".__build(context)?");
    }
    let kind = Type(element.to_owned());
    plans.push(Binding {
        name: name.to_owned(),
        kind,
        references,
        construction: Construction::Instance {
            prepare: setup,
            build: format!(
            "let instance_{name} = {builder};\nlet source_{name} = ::organon_theoria::Source::from(instance_{name});\n"
            ),
        },
    });
}

fn argument(value: &Argument, ty: &Type) -> String {
    if let Argument::Reference { name } = value {
        return bind::source(name);
    }
    let mut captures = String::new();
    let mut reads = String::new();
    let mut seen = HashSet::new();
    for name in value.references() {
        if !seen.insert(name.clone()) {
            continue;
        }
        writeln!(captures, "let source_{name} = {};", bind::source(&name)).unwrap();
        writeln!(reads, "let binding_{name} = &*source_{name}.read(cx);").unwrap();
    }
    let expression = match value {
        Argument::Interpreted { .. } => unreachable!(),
        Argument::Native { expression, .. } => expression.clone(),
        Argument::Reference { .. } => unreachable!(),
    };
    let result_type = type_name(ty);
    format!(
        "{{ {captures} ::organon_theoria::Calculation::new(move |cx| -> {result_type} {{ let _ = cx; {reads} {expression} }}).bind_in(context)? }}"
    )
}

/// Select one explicitly named output for Bind. Keeping Sources here preserves a child's
/// fixedness and observes replacement of an Element-valued parent.
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
        output_type: &Type,
        plans: &mut Vec<Binding>,
    ) -> String {
        let key = (parent.to_owned(), field.to_owned());
        if let Some(name) = self.names.get(&key) {
            return name.clone();
        }
        let selected = format!("value.r#{field}()");
        let expression = format!("{}.project(|value| {{ {selected} }})", bind::source(parent));
        let mut name = format!("__organon_output_{}", self.used.len());
        while !self.used.insert(name.clone()) {
            name.push('_');
        }
        plans.push(Binding {
            name: name.to_owned(),
            kind: output_type.clone(),
            references: vec![parent.to_owned()],
            construction: Construction::Source(expression),
        });
        self.names.insert(key, name.clone());
        name
    }
}
