//! Execute checked Organon IR using a native library registry.
//!
//! This is the interpreted path used by previews; compiled code uses Source directly.

mod registry;
use organon_theoria::Source;
pub use registry::{Entry, Object, Outputs, Registry, TypeAdapter, Value};

type Prepared = std::collections::BTreeMap<
    (usize, String),
    organon_theoria::attribute::Attribute<organon_theoria::attribute::Value>,
>;

impl Registry {
    pub fn evaluate(
        &self,
        program: &organon_core::semantic::Program,
        name: &str,
        inputs: std::collections::BTreeMap<String, Value>,
    ) -> Result<std::collections::BTreeMap<String, Result<Value, String>>, String> {
        self.object(program, name, inputs)
            .map(|object| object.outputs)
    }

    fn object(
        &self,
        program: &organon_core::semantic::Program,
        name: &str,
        inputs: std::collections::BTreeMap<String, Value>,
    ) -> Result<Object, String> {
        let element = program
            .elements
            .iter()
            .find(|e| e.name == name)
            .ok_or("unknown element")?;
        for input in &element.inputs {
            if !inputs.contains_key(&input.name) {
                return Err(format!("missing input {:?}", input.name));
            }
        }
        let mut attributes = organon_theoria::attribute::Scope::new(self.attributes.clone());
        for name in inputs
            .keys()
            .chain(element.bindings.iter().map(|b| &b.name))
        {
            attributes.declare(name);
        }
        for input in &element.inputs {
            attributes.declare_contract(&input.name, input.kind.spelling());
        }
        for binding in &element.bindings {
            attributes.declare_contract(&binding.name, binding.value.ty.kind.spelling());
            if binding.value.ty.fallible {
                attributes.declare_fallible(&binding.name);
            }
            if let Some(signature) = program.signatures.get(binding.value.ty.kind.spelling()) {
                attributes.declare_interfaces(&binding.name, &signature.implements);
            }
        }
        for (name, value) in &inputs {
            attributes.set(name, Source::from(value.native.clone()));
        }
        let mut prepared = std::collections::BTreeMap::new();
        for binding in &element.bindings {
            let mut dependencies = vec![];
            self.prepare(
                &binding.value,
                &attributes,
                &mut prepared,
                &mut dependencies,
            )?;
            attributes.dependencies(&binding.name, dependencies);
        }
        let mut scope: std::collections::BTreeMap<_, _> =
            inputs.into_iter().map(|(n, v)| (n, Ok(v))).collect();
        for name in attributes.order()? {
            let binding = element
                .bindings
                .iter()
                .find(|b| b.name == name)
                .ok_or("missing binding")?;
            let value = self.expression(program, &binding.value, &scope, &prepared);
            if binding.value.ty.fallible {
                attributes.set_fallible(
                    &name,
                    Source::from(
                        value
                            .clone()
                            .map(|v| v.native)
                            .map_err(|_| "preview output failed"),
                    ),
                );
            } else {
                attributes.set(&name, Source::from(value.clone()?.native));
            }
            scope.insert(name, value);
        }
        let outputs = element
            .outputs
            .iter()
            .map(|e| {
                (
                    e.name.clone(),
                    scope
                        .get(&e.binding)
                        .cloned()
                        .unwrap_or_else(|| Err("missing output".into())),
                )
            })
            .collect();
        let mut interfaces = std::collections::BTreeMap::new();
        for implementation in &element.implementations {
            let key = self
                .types
                .get(&implementation.interface)
                .ok_or("missing interface registry")?
                .name;
            interfaces.insert(
                key.into(),
                implementation
                    .outputs
                    .iter()
                    .map(|e| {
                        (
                            e.name.clone(),
                            scope
                                .get(&e.binding)
                                .cloned()
                                .unwrap_or_else(|| Err("missing interface output".into())),
                        )
                    })
                    .collect(),
            );
        }
        Ok(Object {
            outputs,
            interfaces,
        })
    }

    fn prepare(
        &self,
        expression: &organon_core::semantic::Expression,
        scope: &organon_theoria::attribute::Scope,
        prepared: &mut Prepared,
        dependencies: &mut Vec<String>,
    ) -> Result<(), String> {
        use organon_core::semantic::{Argument, ExpressionKind};
        match &expression.kind {
            ExpressionKind::Reference(name) => dependencies.push(name.clone()),
            ExpressionKind::Projection { parent, .. } => dependencies.push(parent.clone()),
            ExpressionKind::Construct {
                arguments,
                children,
                ..
            } => {
                for (field, arg) in arguments {
                    match arg {
                        Argument::Reference { name, .. } => dependencies.push(name.clone()),
                        Argument::Attribute { text, expected } => {
                            let interpret = self
                                .types
                                .get(expected.kind.spelling())
                                .map(|t| t.interpret)
                                .ok_or("missing attribute interpreter")?;
                            let attribute = interpret(text, scope, expected.fallible)?;
                            dependencies.extend(attribute.dependencies.clone());
                            prepared.insert(
                                (expression as *const _ as usize, field.clone()),
                                attribute,
                            );
                        }
                        _ => (),
                    }
                }
                for child in children {
                    self.prepare(child, scope, prepared, dependencies)?;
                }
            }
        }
        Ok(())
    }

    fn expression(
        &self,
        program: &organon_core::semantic::Program,
        value: &organon_core::semantic::Expression,
        scope: &std::collections::BTreeMap<String, Result<Value, String>>,
        prepared: &Prepared,
    ) -> Result<Value, String> {
        use organon_core::semantic as ir;
        let read = |name: &str| {
            scope
                .get(name)
                .ok_or_else(|| format!("unknown reference {name:?}"))?
                .clone()
        };
        match &value.kind {
            ir::ExpressionKind::Reference(name) => read(name),
            ir::ExpressionKind::Projection { parent, output, .. } => read(parent)?
                .get::<Object>()?
                .outputs
                .get(output)
                .cloned()
                .ok_or_else(|| format!("unknown output {output:?}"))?,
            ir::ExpressionKind::Construct {
                name,
                arguments,
                children,
            } => {
                let mut values = std::collections::BTreeMap::new();
                for (name, arg) in arguments {
                    let value = match arg {
                        ir::Argument::Missing { .. } => None,
                        ir::Argument::Reference { name, expected } => {
                            let value = read(name)?;
                            Some(if let Some(ty) = self.types.get(expected.kind.spelling()) {
                                value.resolved(ty, &self.attributes)?
                            } else {
                                value
                            })
                        }
                        ir::Argument::Attribute { .. } => {
                            let attribute = &prepared[&(value as *const _ as usize, name.clone())];
                            let reactive = theoria_reactive::Reactive::new();
                            let source = attribute.source.clone();
                            let source = organon_theoria::Calculation::new(move |cx| {
                                source.read(cx).into_cloned()
                            })
                            .bind(&reactive, true)
                            .map_err(|e| e.to_string())?;
                            Some(Value::from_native(
                                source
                                    .as_fixed()
                                    .ok_or("preview input was not fixed")?
                                    .clone(),
                            ))
                        }
                    };
                    values.insert(name.clone(), value);
                }
                if let Some(entry) = self.elements.get(name) {
                    let args = entry
                        .inputs
                        .iter()
                        .map(|n| values.remove(*n).flatten())
                        .collect();
                    let children = children
                        .iter()
                        .map(|c| {
                            let value = self.expression(program, c, scope, prepared)?;
                            if let Some(kind) = entry.children {
                                let ty = self.types.get(kind).ok_or("missing child contract")?;
                                value.resolved(ty, &self.attributes)
                            } else {
                                Ok(value)
                            }
                        })
                        .collect::<Result<_, _>>()?;
                    (entry.evaluate)(args, children)
                } else {
                    if !program.elements.iter().any(|e| e.name == *name) {
                        return Err(format!(
                            "native implementation {name:?} is not registered in the preview host"
                        ));
                    }
                    let inputs = values
                        .into_iter()
                        .map(|(n, v)| {
                            v.map(|v| (n, v))
                                .ok_or_else(|| "missing Element input".to_owned())
                        })
                        .collect::<Result<_, _>>()?;
                    self.object(program, name, inputs)
                        .and_then(|object| self.value(object))
                }
            }
        }
    }
}

pub use organon_core::semantic::Program;
pub fn resolve(source: &str) -> Result<Program, String> {
    let file = organon_core::format::DefinitionFile::from_xml(source).map_err(|e| e.to_string())?;
    organon_core::semantic::resolve(&file).map_err(|e| e.to_string())
}
