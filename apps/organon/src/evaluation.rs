//! Prepare checked IR for evaluation in the preview host's native registry.
mod registry;
pub use organon_core::semantic::Program;
use organon_core::semantic::{self as ir, Type};
use organon_theoria::{
    ReadContext, attribute,
    flow::{self, Expr, Locals},
};
pub use registry::{Entry, Object, Outputs, Registry, TypeAdapter, Value};
use std::{collections::BTreeMap, rc::Rc};

type Schema = BTreeMap<String, Type>;
fn unpack(v: attribute::Value) -> Value {
    v.get::<Value>().unwrap_or_else(|_| Value::from_native(v))
}
fn pack(v: Value) -> attribute::Value {
    attribute::Value::new(v)
}
pub fn resolve(source: &str) -> Result<Program, String> {
    let file = organon_core::xml::parse(source).map_err(|e| e.to_string())?;
    organon_core::resolver::resolve_module(&file).map_err(|e| e.to_string())
}
impl Registry {
    pub fn evaluate(
        &self,
        program: &Program,
        name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<BTreeMap<String, Result<Value, String>>, String> {
        Ok(self
            .evaluate_typed(program, name, inputs)?
            .into_iter()
            .map(|(n, v)| {
                (
                    n,
                    v.map_err(|e| match e {
                        flow::Error::Failure(f) => f
                            .value
                            .get::<String>()
                            .or_else(|_| f.value.get::<&'static str>().map(str::to_owned))
                            .unwrap_or(f.kind),
                        e => e.to_string(),
                    }),
                )
            })
            .collect())
    }
    pub fn evaluate_typed(
        &self,
        program: &Program,
        name: &str,
        inputs: BTreeMap<String, Value>,
    ) -> Result<Outputs, String> {
        self.evaluate_with_inputs(
            program,
            name,
            inputs.into_iter().map(|(n, v)| (n, Ok(v))).collect(),
        )
    }
    pub fn evaluate_with_inputs(
        &self,
        program: &Program,
        name: &str,
        inputs: BTreeMap<String, flow::Outcome<Value>>,
    ) -> Result<Outputs, String> {
        let host = Planner {
            registry: Rc::new(self.clone()),
            program: Rc::new(program.clone()),
            children: Vec::new(),
        };
        host.object(name, inputs)
            .map(|o| o.outputs)
            .map_err(|e| e.to_string())
    }
}
#[derive(Clone)]
struct Planner {
    registry: Rc<Registry>,
    program: Rc<Program>,
    children: Vec<Value>,
}
impl Planner {
    fn id(&self, n: &str) -> String {
        self.registry.types.get(n).map_or(n, |t| t.name).to_owned()
    }
    fn input(
        &self,
        allowed: &[Type],
        value: flow::Outcome<Value>,
    ) -> flow::Outcome<flow::Outcome<Value>> {
        match value {
            Ok(v) => Ok(Ok(v)),
            Err(flow::Error::Failure(f)) => {
                Ok(Err(flow::Error::Failure(self.accept_failure(allowed, f)?)))
            }
            Err(e) => Err(e),
        }
    }
    fn accept_failure(
        &self,
        allowed: &[Type],
        failure: flow::Failure,
    ) -> flow::Outcome<flow::Failure> {
        let reasons: Vec<_> = allowed.iter().map(|t| self.id(t.name())).collect();
        failure.accept(&reasons, &self.registry.attributes)
    }
    fn object(
        &self,
        name: &str,
        inputs: BTreeMap<String, flow::Outcome<Value>>,
    ) -> flow::Outcome<Object> {
        let body = self
            .program
            .bodies
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| format!("unknown element {name:?}"))?;
        let c = &self.program.contracts[name];
        if let Some(c) = &c.children {
            if !c.accepts(self.children.len()) {
                return Err("invalid child count".into());
            }
        } else if !self.children.is_empty() {
            return Err("element does not accept children".into());
        }
        let mut schema = Schema::new();
        let mut values = Locals::new();
        for p in &c.inputs {
            let input = inputs
                .get(p.name.as_str())
                .ok_or_else(|| format!("missing input {:?}", p.name))?
                .clone();
            let v = self.input(&p.ty.errors, input)?.map(pack);
            schema.insert(p.name.to_string(), p.ty.kind.clone());
            values.insert(p.name.to_string(), Rc::new(move |_, _| v.clone()));
        }
        let (locals, _) = self.locals(&body.bindings, &schema)?;
        values.extend(locals);
        let outputs = body.outputs.clone();
        let impls = body.implementations.clone();
        let ids = impls
            .iter()
            .map(|i| self.id(&i.interface))
            .collect::<Vec<_>>();
        let reactive = theoria_reactive::Reactive::new();
        let evaluated = reactive.create_computed(move |native| -> flow::Outcome<Object> {
            let cx = ReadContext::new(native);
            let f = flow::Frame::new(&values, None);
            let project = |ports: &[ir::OutputBinding]| -> Result<Outputs, String> {
                ports
                    .iter()
                    .map(|p| {
                        let v = match f.read(&p.binding, &cx) {
                            Err(flow::Error::Invalid(e)) => return Err(e),
                            v => v.map(unpack),
                        };
                        Ok((p.name.to_string(), v))
                    })
                    .collect()
            };
            Ok(Object {
                outputs: project(&outputs)?,
                interfaces: impls
                    .iter()
                    .zip(&ids)
                    .map(|(i, id)| Ok((id.clone(), project(&i.outputs)?)))
                    .collect::<Result<_, String>>()?,
            })
        });
        evaluated.snapshot().map_err(|e| e.to_string())?.cloned()
    }
    fn locals(&self, values: &[ir::Binding], outer: &Schema) -> Result<(Locals, Schema), String> {
        let mut schema = outer.clone();
        for b in values {
            schema.insert(b.name.clone(), b.value.ty.clone());
        }
        let locals = values
            .iter()
            .map(|b| Ok((b.name.clone(), self.expr(&b.value, &schema)?)))
            .collect::<Result<_, String>>()?;
        Ok((locals, schema))
    }
    fn arg(&self, arg: &ir::Argument, schema: &Schema) -> Result<Expr, String> {
        match arg {
            ir::Argument::Missing { .. } => Err("missing required argument".into()),
            ir::Argument::Reference { name, .. } => Ok(flow::reference(name)),
            ir::Argument::Attribute { text, expected } => {
                let adapter = self
                    .registry
                    .types
                    .get(expected.kind.name())
                    .ok_or_else(|| format!("missing parser for {}", expected.kind.name()))?;
                let mut scope = attribute::Scope::new(self.registry.attributes.clone());
                for (n, t) in schema {
                    let name = self.id(t.name());
                    let interfaces: Vec<_> = self
                        .program
                        .contracts
                        .get(t.name())
                        .into_iter()
                        .flat_map(|c| &c.implements)
                        .map(|i| self.id(i))
                        .collect();
                    let interfaces: Vec<_> = interfaces.iter().map(String::as_str).collect();
                    scope.declare(
                        n,
                        attribute::Contract {
                            name: &name,
                            interfaces: &interfaces,
                        },
                    );
                }
                let parsed = (adapter.interpret)(text, &scope)?;
                Ok(flow::parsed_with(scope, parsed, |v| Ok(unpack(v).native)))
            }
        }
    }
    fn block(
        &self,
        body: &ir::Block,
        condition: Option<&ir::Condition>,
        outer: &Schema,
    ) -> Result<flow::Branch, String> {
        let (locals, schema) = self.locals(&body.bindings, outer)?;
        let guard = condition.map(|c| self.test(c, &schema)).transpose()?;
        let mut result = self.arg(&body.result, &schema)?;
        if body.throwing {
            let (ir::Argument::Attribute { expected, .. }
            | ir::Argument::Reference { expected, .. }) = &body.result
            else {
                unreachable!()
            };
            result = flow::throwing(
                &self.id(expected.kind.name()),
                flow::map(result, |v| Ok(unpack(v).native)),
            );
        }
        Ok(flow::Branch {
            locals,
            guard,
            result,
        })
    }
    fn test(&self, test: &ir::Condition, schema: &Schema) -> Result<Expr<bool>, String> {
        Ok(match test {
            ir::Condition::All(v) => flow::all(
                v.iter()
                    .map(|p| self.test(p, schema))
                    .collect::<Result<_, _>>()?,
            ),
            ir::Condition::Any(v) => flow::any(
                v.iter()
                    .map(|p| self.test(p, schema))
                    .collect::<Result<_, _>>()?,
            ),
            ir::Condition::Not(v) => flow::not(self.test(v, schema)?),
            ir::Condition::Call {
                name, arguments, ..
            } => {
                let p = &self.program.predicates[name];
                let mut args = Vec::new();
                for port in &p.inputs {
                    args.push(self.arg(&arguments[port.name.as_str()], schema)?);
                }
                if let ir::PredicateBody::Defined {
                    bindings: locals,
                    condition: body,
                } = &p.body
                {
                    let schema = p
                        .inputs
                        .iter()
                        .map(|p| (p.name.to_string(), p.ty.kind.clone()))
                        .collect();
                    let (locals, schema) = self.locals(locals, &schema)?;
                    let params = p
                        .inputs
                        .iter()
                        .zip(args)
                        .map(|(port, arg)| {
                            let reasons: Vec<_> =
                                port.ty.errors.iter().map(|t| self.id(t.name())).collect();
                            let registry = self.registry.clone();
                            (
                                port.name.to_string(),
                                flow::predicate_argument(arg, move |e| {
                                    e.accept(&reasons, &registry.attributes)
                                }),
                            )
                        })
                        .collect();
                    flow::predicate(params, locals, self.test(body, &schema)?)
                } else {
                    let native = *self
                        .registry
                        .predicates
                        .get(name)
                        .ok_or_else(|| format!("no native predicate {name:?}"))?;
                    let ports: Vec<_> = p.inputs.iter().map(|p| p.ty.errors.clone()).collect();
                    let host = self.clone();
                    Rc::new(move |f, c| {
                        let mut values = Vec::with_capacity(args.len());
                        for (arg, allowed) in args.iter().zip(&ports) {
                            values.push(host.input(allowed, arg(f, c).map(unpack))?);
                        }
                        native(values, &host.registry.attributes)
                    })
                }
            }
        })
    }
    fn expr(&self, e: &ir::Expression, schema: &Schema) -> Result<Expr, String> {
        Ok(match &e.kind {
            ir::ExpressionKind::Select(cases) => flow::select(
                cases
                    .iter()
                    .map(|c| self.block(&c.body, c.condition.as_ref(), schema))
                    .collect::<Result<_, _>>()?,
            ),
            ir::ExpressionKind::Try { body, handlers } => {
                let mut list = Vec::new();
                for h in handlers {
                    let ir::HandlerAction::Catch { name, branch } = &h.action else {
                        let target = h.reason.clone();
                        let program = self.program.clone();
                        let registry = self.registry.clone();
                        list.push(flow::Handler::Unreachable {
                            result: body.result_text().into(),
                            match_reason: Box::new(move |kind| {
                                for (name, contract) in &program.contracts {
                                    let id =
                                        registry.types.get(name).map_or(name.as_str(), |t| t.name);
                                    if id == kind {
                                        return target
                                            .as_ref()
                                            .is_none_or(|t| {
                                                name == t.name()
                                                    || contract
                                                        .implements
                                                        .iter()
                                                        .any(|i| i == t.name())
                                            })
                                            .then(|| name.clone());
                                    }
                                }
                                target.is_none().then(|| kind.into())
                            }),
                        });
                        continue;
                    };
                    let mut local = schema.clone();
                    if let (Some(n), Some(t)) = (name, &h.reason) {
                        local.insert(n.clone(), t.clone());
                    }
                    let target = h.reason.as_ref().map(|t| self.id(t.name()));
                    let implementations: Vec<_> = self
                        .program
                        .contracts
                        .iter()
                        .filter(|(_, c)| {
                            h.reason
                                .as_ref()
                                .is_some_and(|r| c.implements.iter().any(|i| i == r.name()))
                        })
                        .map(|(n, _)| self.id(n))
                        .collect();
                    let reg = self.registry.clone();
                    list.push(flow::Handler::Catch {
                        name: name.clone(),
                        branch: self.block(&branch.body, branch.condition.as_ref(), &local)?,
                        matches: Box::new(move |e| {
                            if let Some(id) = &target
                                && id != &e.kind
                            {
                                return if implementations.contains(&e.kind) {
                                    Ok(Some(
                                        unpack(e.value.clone())
                                            .native
                                            .with_contract(&reg.attributes, &e.kind)
                                            .provide(id)?,
                                    ))
                                } else {
                                    Ok(None)
                                };
                            }
                            Ok(Some(e.value.clone()))
                        }),
                    });
                }
                flow::attempt(self.block(body, None, schema)?, list)
            }
            ir::ExpressionKind::Reference(n) => flow::reference(n),
            ir::ExpressionKind::AsInterface(n) => {
                let target = self.id(e.ty.name());
                let source = self.id(schema[n].name());
                let registry = self.registry.clone();
                flow::map(flow::reference(n), move |v| {
                    let value = unpack(v)
                        .native
                        .with_contract(&registry.attributes, &source);
                    Ok(pack(Value::from_native(value.provide(&target)?)))
                })
            }
            ir::ExpressionKind::Projection { parent, output, .. } => {
                let output = output.clone();
                let projection = self
                    .registry
                    .projections
                    .get(&(self.id(schema[parent].name()), output.clone()))
                    .copied();
                flow::map(flow::reference(parent), move |v| {
                    let v = unpack(v);
                    if let Some(get) = projection {
                        return get(v).map(pack);
                    }
                    v.get::<Object>()?
                        .outputs
                        .get(&output)
                        .ok_or_else(|| format!("missing output {output:?}"))?
                        .clone()
                        .map(pack)
                })
            }
            ir::ExpressionKind::Construct {
                name,
                arguments,
                children,
            } => {
                let c = &self.program.contracts[name];
                let args = c
                    .inputs
                    .iter()
                    .map(|p| {
                        let arg = &arguments[p.name.as_str()];
                        Ok((
                            p.name.to_string(),
                            p.ty.errors.clone(),
                            if matches!(arg, ir::Argument::Missing { .. }) {
                                None
                            } else {
                                Some(self.arg(arg, schema)?)
                            },
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let children = children
                    .iter()
                    .map(|child| match child {
                        ir::Child::Incoming { kind } => Ok((None, self.id(kind.name()))),
                        ir::Child::Value(v) => self
                            .expr(v, schema)
                            .map(|expr| (Some(expr), self.id(v.ty.name()))),
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                let adapter = c
                    .children
                    .as_ref()
                    .and_then(|c| self.registry.types.get(c.kind.name()))
                    .cloned();
                let host = self.clone();
                let name = name.clone();
                Rc::new(move |f, cx| {
                    let mut supplied = Vec::new();
                    for (_, allowed, arg) in &args {
                        supplied.push(
                            arg.as_ref()
                                .map(|a| host.input(allowed, a(f, cx).map(unpack)))
                                .transpose()?,
                        );
                    }
                    let mut nested = Vec::new();
                    for (child, source) in &children {
                        let values = if let Some(expr) = child {
                            vec![unpack(expr(f, cx)?)]
                        } else {
                            host.children.clone()
                        };
                        for value in values {
                            nested.push(if let Some(a) = &adapter {
                                value.resolved(a, source, &host.registry.attributes)?
                            } else {
                                value
                            });
                        }
                    }
                    if let Some(entry) = host.registry.elements.get(&name) {
                        return Ok(pack((entry.evaluate)(
                            supplied,
                            nested,
                            &host.registry.attributes,
                        )?));
                    }
                    let child = Planner {
                        children: nested,
                        ..host.clone()
                    };
                    let inputs = args
                        .iter()
                        .zip(supplied)
                        .filter_map(|((n, _, _), v)| v.map(|v| (n.clone(), v)))
                        .collect();
                    Ok(pack(host.registry.value(child.object(&name, inputs)?)?))
                })
            }
        })
    }
}
