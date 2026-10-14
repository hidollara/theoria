//! Lower lexical control flow to executable Rust closures. Native leaves retain their Rust types.
use super::*;
use core_types::{TypeName, ValueType};
type Schema = BTreeMap<String, TypeName>;

pub(super) fn needed(body: &s::Body) -> bool {
    fn conditional(e: &s::Expression) -> bool {
        match &e.kind {
            s::ExpressionKind::Select(_) | s::ExpressionKind::Try { .. } => true,
            s::ExpressionKind::Construct { children, arguments, .. } => {
                arguments.values().any(|arg| matches!(arg,
                    s::Argument::Reference { expected, .. } | s::Argument::Attribute { expected, .. }
                    if expected.fallible()
                )) || children.iter().any(|c| matches!(c,s::Child::Value(e) if conditional(e)))
            }
            _ => false,
        }
    }
    body.bindings.iter().any(|b| conditional(&b.value))
}
pub(super) fn alias(v: &ValueType) -> String {
    let mut name = format!("__flow_{}_{}", v.kind.spelling().len(), v.kind.spelling());
    for e in &v.errors {
        write!(name, "_{}_{}", e.spelling().len(), e.spelling()).unwrap();
    }
    name
}
struct Emit<'a, 'b> {
    program: &'a s::Program,
    ctx: &'a Context<'b>,
    incoming: Option<&'a String>,
}
pub(super) fn value_alias(
    ctx: &Context<'_>,
    t: &ValueType,
    aliases: &mut String,
) -> Result<String, Error> {
    let name = alias(t);
    let declaration = format!("pub type {name}=");
    if !aliases.contains(&declaration) {
        let rust = crate::library::rust_value(ctx.loaded, &ctx.providers, t)?;
        writeln!(aliases, "{declaration}{rust};").unwrap();
    }
    Ok(name)
}
fn ty(n: &TypeName) -> String {
    format!("super::__organon_values::r#{}", n.spelling())
}
impl Emit<'_, '_> {
    fn id(&self, t: &TypeName) -> Result<String, Error> {
        let id = &self.ctx.loaded.origins[t.spelling()];
        organon_cargo::identity(&id.source, &id.name)
    }
    fn unpack(&self, v: &ValueType, expr: &str) -> Result<String, Error> {
        Ok(match v.errors.as_slice() {
            [] => format!("Ok(Value::new({expr}))"),
            [e] => format!(
                "({expr}).map(Value::new).map_err(|e|flow::Error::Failure(flow::Failure{{kind:{:?}.into(),value:Value::new(e)}}))",
                self.id(e)?
            ),
            _ => format!(
                "({expr}).map(Value::new).map_err(|e|flow::Error::Failure(e.into_failure()))"
            ),
        })
    }
    fn convert(&self, v: &ValueType, expr: &str) -> Result<String, Error> {
        let t = ty(&v.kind);
        Ok(match v.errors.as_slice() {
            [] => format!("({expr})?.get::<{t}>()?"),
            [e] => format!(
                "match {expr} {{Ok(v)=>Ok(v.get::<{t}>()?),Err(flow::Error::Failure(e))=>match e.as_contract({:?},&super::__organon_registry())?{{Some(v)=>Err(v.get::<{}>()?),None=>return Err(flow::Error::Failure(e))}},Err(e)=>return Err(e)}}",
                self.id(e)?,
                ty(e)
            ),
            errors => {
                let set = crate::library::error_set(self.ctx.loaded, &self.ctx.providers, errors)?;
                let reasons = errors
                    .iter()
                    .map(|t| self.id(t))
                    .collect::<Result<Vec<_>, _>>()?;
                // Rust error sets are canonicalized for type identity. Interface
                // selection still follows the receiving port's declaration order.
                format!(
                    "match {expr} {{Ok(v)=>Ok(v.get::<{t}>()?),Err(flow::Error::Failure(e))=>{{let registry=super::__organon_registry();let e=e.accept(&{reasons:?},&registry)?;Err(<{set}>::checked(e,&registry)?)}},Err(e)=>return Err(e)}}"
                )
            }
        })
    }
    fn arg(&self, arg: &s::Argument, schema: &Schema) -> Result<String, Error> {
        self.argument(arg, schema, false)
    }
    fn argument(
        &self,
        arg: &s::Argument,
        schema: &Schema,
        as_source: bool,
    ) -> Result<String, Error> {
        Ok(match arg {
            s::Argument::Missing { .. } => {
                return Err(Error("missing required branch value".into()));
            }
            s::Argument::Reference { name, .. } => format!("flow::reference({name:?})"),
            s::Argument::Attribute { text, expected } => {
                let marker = self.ctx.marker(expected.kind.spelling())?;
                let mut code = String::from(
                    "{let mut scope=::organon_theoria::attribute::Scope::new(super::__organon_registry());",
                );
                for (n, t) in schema {
                    let k = t.spelling();
                    write!(code,"scope.declare({n:?},::organon_theoria::attribute::contract({k:?},super::__ORGANON_CONTRACTS));").unwrap();
                }
                code.push_str("const ATTRIBUTE_SCHEMA:&[::organon_theoria::attribute::BindingType<'static>]=&[");
                for (name, t) in schema {
                    let t = t.spelling();
                    write!(code,"::organon_theoria::attribute::BindingType{{name:{name:?},contract: ::organon_theoria::attribute::contract({t:?},super::__ORGANON_CONTRACTS)}},").unwrap();
                }
                let prepare = crate::instance::prepare(&marker, text, "scope");
                let finish = if as_source {
                    "flow::parameter(scope,a,&root_inputs)"
                } else {
                    "flow::map(flow::parsed(scope,a),|v|Ok(Value::new(v)))"
                };
                write!(code, "];let a={prepare};{finish}}}").unwrap();
                code
            }
        })
    }
    fn scope(&self, schema: &Schema, b: &[s::Binding]) -> Schema {
        let mut schema = schema.clone();
        for v in b {
            schema.insert(v.name.clone(), v.value.ty.clone());
        }
        schema
    }
    fn locals(&self, schema: &Schema, b: &[s::Binding]) -> Result<String, Error> {
        self.bindings(schema, b, false)
    }
    fn bindings(&self, schema: &Schema, b: &[s::Binding], root: bool) -> Result<String, Error> {
        let mut code = String::from("{let mut locals=flow::Locals::new();");
        for v in b {
            write!(
                code,
                "locals.insert({:?}.into(),{});",
                v.name,
                self.expr(&v.value, schema, root)?
            )
            .unwrap();
        }
        code.push_str("locals}");
        Ok(code)
    }
    fn branch(
        &self,
        b: &s::Block,
        g: Option<&s::Condition>,
        schema: &Schema,
    ) -> Result<String, Error> {
        let schema = self.scope(schema, &b.bindings);
        let locals = self.locals(&schema, &b.bindings)?;
        let guard = match g {
            Some(c) => format!("Some({})", self.test(c, &schema)?),
            None => "None".into(),
        };
        let mut result = self.arg(&b.result, &schema)?;
        if b.throwing {
            let (s::Argument::Attribute { expected, .. } | s::Argument::Reference { expected, .. }) =
                &b.result
            else {
                unreachable!()
            };
            result = format!("flow::throwing({:?},{result})", self.id(&expected.kind)?);
        }
        Ok(format!(
            "flow::Branch{{locals:{locals},guard:{guard},result:{result}}}"
        ))
    }
    fn expr(&self, e: &s::Expression, schema: &Schema, root: bool) -> Result<String, Error> {
        Ok(match &e.kind {
            s::ExpressionKind::Reference(n) => format!("flow::reference({n:?})"),
            s::ExpressionKind::AsInterface(n) => {
                let source = ty(&schema[n]);
                let target = ty(&e.ty);
                format!(
                    "flow::map(flow::reference({n:?}),|v|Ok(Value::new(::organon_theoria::Provide::<{target}>::provide(&v.get::<{source}>()?))))"
                )
            }
            s::ExpressionKind::Select(cases) => {
                let items = cases
                    .iter()
                    .map(|b| self.branch(&b.body, b.condition.as_ref(), schema))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",");
                // Keep branch expressions visible to structural facade-path rewriting.
                format!("flow::select(::std::vec::Vec::from([{items}]))")
            }
            s::ExpressionKind::Try { body, handlers } => {
                let result = body.result_text();
                let body = self.branch(body, None, schema)?;
                let mut list = vec![];
                for h in handlers {
                    let s::HandlerAction::Catch { name, branch } = &h.action else {
                        let target = if let Some(t) = &h.reason {
                            let id = self.id(t)?;
                            format!("Some({id:?})")
                        } else {
                            "None".into()
                        };
                        list.push(format!(
                            "flow::unreachable({result:?},{target},super::__ORGANON_CONTRACTS)"
                        ));
                        continue;
                    };
                    let test = if let Some(t) = &h.reason {
                        let id = self.id(t)?;
                        format!("|e|e.as_contract({id:?},&super::__organon_registry())")
                    } else {
                        "|e|Ok(Some(e.value.clone()))".into()
                    };
                    let mut local = schema.clone();
                    if let (Some(n), Some(t)) = (name, &h.reason) {
                        local.insert(n.clone(), t.clone());
                    }
                    let name = name
                        .as_ref()
                        .map_or("None".into(), |n| format!("Some({n:?}.into())"));
                    let branch = self.branch(&branch.body, branch.condition.as_ref(), &local)?;
                    list.push(format!(
                        "flow::Handler::Catch{{matches:Box::new({test}),name:{name},branch:{branch}}}"
                    ));
                }
                format!(
                    "flow::attempt({body},::std::vec::Vec::from([{}]))",
                    list.join(",")
                )
            }
            s::ExpressionKind::Projection { parent, output } => {
                let output_type = &self.program.contracts[schema[parent].spelling()]
                    .outputs
                    .iter()
                    .find(|p| p.name == *output)
                    .unwrap()
                    .ty;
                let t = ty(&schema[parent]);
                let read = format!(
                    "f.read({parent:?},cx)?.get::<{t}>()?.r#{output}().read(cx).into_cloned()"
                );
                format!(
                    "Rc::new(move|f:&flow::Frame<'_>,cx:&::organon_theoria::ReadContext<'_,'_>|{{{}}}) as flow::Expr",
                    if self.program.contracts[schema[parent].spelling()].kind
                        == s::DefinitionKind::Element
                    {
                        let id = self.ctx.effects.output(schema[parent].spelling(), output);
                        format!(
                            "<super::__organon_values::__errors_{id} as ::organon_theoria::effects::Port<{}>>::unpack({read})",
                            ty(output_type.kind())
                        )
                    } else {
                        self.unpack(
                            output_type.declared().expect("native output contract"),
                            &read,
                        )?
                    }
                )
            }
            s::ExpressionKind::Construct {
                name,
                arguments,
                children,
            } => {
                let c = &self.program.contracts[name];
                let mut setup = String::new();
                let mut build = format!("super::r#{name}::builder()");
                for (i, p) in c.inputs.iter().enumerate() {
                    let n = p.name.as_str();
                    if matches!(arguments[n], s::Argument::Missing { .. }) {
                        write!(build, ".r#{n}(None)").unwrap();
                        continue;
                    }
                    if root
                        && !p.ty.fallible()
                        && matches!(arguments[n], s::Argument::Attribute { .. })
                    {
                        let arg = self.argument(&arguments[n], schema, true)?;
                        write!(setup, "let arg{i}={arg};").unwrap();
                        let mut source = format!("arg{i}(f,cx)?");
                        if c.optional_inputs.contains(n) {
                            source.push_str(".map(|v|Some(v.clone()))");
                        }
                        write!(build, ".r#{n}({source})").unwrap();
                        continue;
                    }
                    write!(
                        setup,
                        "let arg{i}:flow::Expr={};",
                        self.arg(&arguments[n], schema)?
                    )
                    .unwrap();
                    let mut val = self.convert(&p.ty, &format!("arg{i}(f,cx)"))?;
                    if c.optional_inputs.contains(n) {
                        val = format!("Some({val})");
                    }
                    write!(build, ".r#{n}({val})").unwrap();
                }
                if let Some(contract) = &c.children {
                    let mut parts = String::from("{let mut children=Vec::new();");
                    let t = ty(&contract.kind);
                    for (i, k) in children.iter().enumerate() {
                        match k {
                            s::Child::Value(v) => {
                                write!(
                                    setup,
                                    "let child{i}:flow::Expr={};",
                                    self.expr(v, schema, root)?
                                )
                                .unwrap();
                                let read = format!("child{i}(f,cx)?.get::<{}>()?", ty(&v.ty));
                                if v.ty == contract.kind {
                                    write!(parts, "children.push({read});").unwrap();
                                } else {
                                    write!(parts,"children.push(::organon_theoria::Provide::<{t}>::provide(&{read}));").unwrap();
                                }
                            }
                            s::Child::Incoming { kind } => {
                                let n = self
                                    .incoming
                                    .ok_or_else(|| Error("missing children".into()))?;
                                let vals = format!("f.read({n:?},cx)?.get::<Vec<{}>>()?", ty(kind));
                                if kind == &contract.kind {
                                    write!(parts, "children.extend({vals});").unwrap();
                                } else {
                                    write!(parts,"children.extend({vals}.iter().map(::organon_theoria::Provide::<{t}>::provide));").unwrap();
                                }
                            }
                        }
                    }
                    write!(build, ".__children({parts}children}})").unwrap();
                }
                let method = if c.kind == s::DefinitionKind::Element {
                    "__build"
                } else {
                    "build_in"
                };
                format!(
                    "{{{setup}let reactive=reactive.clone();Rc::new(move|f:&flow::Frame<'_>,cx:&::organon_theoria::ReadContext<'_,'_>|{{let built={build}.{method}(&mut flow::DuringRead(reactive.clone())).map_err(|e|e.to_string())?;Ok(Value::new(built))}}) as flow::Expr}}"
                )
            }
        })
    }
    fn test(&self, c: &s::Condition, schema: &Schema) -> Result<String, Error> {
        Ok(match c {
            s::Condition::All(v) | s::Condition::Any(v) => {
                let op = if matches!(c, s::Condition::All(_)) {
                    "all"
                } else {
                    "any"
                };
                let items = v
                    .iter()
                    .map(|c| self.test(c, schema))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",");
                format!("flow::{op}(::std::vec::Vec::from([{items}]))")
            }
            s::Condition::Not(c) => format!("flow::not({})", self.test(c, schema)?),
            s::Condition::Call {
                name, arguments, ..
            } => {
                let p = &self.program.predicates[name];
                let mut setup = String::new();
                let mut args = vec![];
                for (i, port) in p.inputs.iter().enumerate() {
                    write!(
                        setup,
                        "let a{i}:flow::Expr={};",
                        self.arg(&arguments[port.name.as_str()], schema)?
                    )
                    .unwrap();
                    let argument = if matches!(p.body, s::PredicateBody::Defined { .. }) {
                        let reasons: Result<Vec<_>, _> =
                            port.ty.errors.iter().map(|t| self.id(t)).collect();
                        let reasons = reasons?;
                        let accept = if reasons.is_empty() {
                            "|e|Err(flow::Error::Failure(e))".to_owned()
                        } else {
                            format!("|e|e.accept(&{reasons:?},&super::__organon_registry())")
                        };
                        format!(
                            "({:?}.into(),flow::predicate_argument(a{i},{accept}))",
                            port.name
                        )
                    } else {
                        format!("&({})", self.convert(&port.ty, &format!("a{i}(f,cx)"))?)
                    };
                    args.push(argument);
                }
                let args = args.join(",");
                if let s::PredicateBody::Defined {
                    bindings,
                    condition: test,
                } = &p.body
                {
                    let inner = p
                        .inputs
                        .iter()
                        .map(|p| (p.name.to_string(), p.ty.kind.clone()))
                        .collect();
                    let inner = self.scope(&inner, bindings);
                    format!(
                        "{{{setup}flow::predicate(flow::Locals::from([{args}]),{}, {})}}",
                        self.locals(&inner, bindings)?,
                        self.test(test, &inner)?
                    )
                } else {
                    let marker = self.ctx.marker(name)?;
                    let call = format!("{marker}::__organon_test({args})");
                    let s::PredicateBody::Declared { errors } = &p.body else {
                        unreachable!()
                    };
                    let result = match errors.as_slice() {
                        [] => format!("Ok({call})"),
                        [reason] => format!(
                            "{call}.map_err(|v|flow::Error::Failure(flow::Failure{{kind:{:?}.into(),value:Value::new(v)}}))",
                            self.id(reason)?
                        ),
                        _ => format!("{call}.map_err(|e|flow::Error::Failure(e.into_failure()))"),
                    };
                    format!(
                        "{{{setup}Rc::new(move|f:&flow::Frame<'_>,cx:&::organon_theoria::ReadContext<'_,'_>|{{{result}}}) as flow::Expr<bool>}}"
                    )
                }
            }
        })
    }
}
/// Forward an existing Source when the path to it consists entirely of aliases and projections.
fn forwarded(name: &str, body: &s::Body, ctx: &Context<'_>) -> Option<(String, s::OutputType)> {
    if let Some(p) = ctx.contracts[&body.name]
        .inputs
        .iter()
        .find(|p| p.name == name)
    {
        return Some((
            format!("inputs.r#{name}.clone()"),
            s::OutputType::Declared(p.ty.clone()),
        ));
    }
    let b = body.bindings.iter().find(|b| b.name == name)?;
    match &b.value.kind {
        s::ExpressionKind::Reference(n) => forwarded(n, body, ctx),
        s::ExpressionKind::Projection { parent, output, .. } => {
            if ctx
                .effects
                .fallible_bindings
                .contains(&(body.name.clone(), parent.clone()))
                || ctx.contracts[&body.name]
                    .inputs
                    .iter()
                    .any(|p| p.name == *parent && p.ty.fallible())
            {
                return None;
            }
            let source = forwarded(parent, body, ctx)?;
            let parent_type = source.1.kind().spelling();
            let output_type = ctx.contracts[parent_type]
                .outputs
                .iter()
                .find(|p| p.name == *output)?
                .ty
                .clone();
            let output_type = ctx
                .effects
                .resolved_outputs
                .get(&(parent_type.into(), output.clone()))
                .cloned()
                .map(s::OutputType::Declared)
                .unwrap_or(output_type);
            Some((
                format!("({}).project(|v|v.r#{output}())", source.0),
                output_type,
            ))
        }
        _ => None,
    }
}
pub(super) fn generate(
    body: &s::Body,
    program: &s::Program,
    ctx: &Context<'_>,
    inputs: &[ty::Port],
    aliases: &mut String,
) -> Result<String, Error> {
    let c = &program.contracts[&body.name];
    let emit = Emit {
        program,
        ctx,
        incoming: ctx.children_inputs.get(&body.name),
    };
    let base = c
        .inputs
        .iter()
        .map(|p| (p.name.to_string(), p.ty.kind.clone()))
        .collect();
    let schema = emit.scope(&base, &body.bindings);
    let mut code =
        String::from("use ::organon_theoria::flow::{self,Value};use std::rc::Rc;struct Sources{");
    for p in inputs {
        write!(
            code,
            "r#{}: ::organon_theoria::Source<{}>,",
            p.name,
            crate::element::type_name(&p.ty)
        )
        .unwrap();
    }
    code.push_str("}struct Outputs{");
    let mut outputs = Vec::new();
    for o in &body.outputs {
        outputs.push((o.name.clone(), o, c));
    }
    for (i, imp) in body.implementations.iter().enumerate() {
        for (j, o) in imp.outputs.iter().enumerate() {
            let mut field = format!("__organon_interface_{i}_{j}");
            while outputs.iter().any(|(n, _, _)| *n == field) {
                field.push('_')
            }
            outputs.push((field, o, &program.contracts[&imp.interface]));
        }
    }
    let mut init = String::new();
    let mut sources = String::new();
    let mut shared = BTreeMap::new();
    for (n, o, c) in outputs {
        let t = &c.outputs.iter().find(|p| p.name == o.name).unwrap().ty;
        let mut rt = ty(t.kind());
        if c.kind == s::DefinitionKind::Element {
            rt = format!(
                "super::__organon_values::{}",
                ctx.effects.alias(&body.name, &o.name)
            );
        } else if let Some(ty) = t.declared()
            && ty.fallible()
        {
            let name = value_alias(ctx, ty, aliases)?;
            rt = format!("super::__organon_values::{name}");
        }
        writeln!(code, "r#{n}: ::organon_theoria::Source<{rt}>,").unwrap();
        let next = shared.len();
        let index = *shared.entry((o.binding.clone(), rt)).or_insert(next);
        if next == index {
            if let Some(source) =
                forwarded(&o.binding, body, ctx).filter(|(_, source_type)| match t {
                    s::OutputType::Inferred(_) => true,
                    s::OutputType::Declared(expected) => source_type.declared() == Some(expected),
                })
            {
                writeln!(sources, "let output{index}={};", source.0).unwrap();
            } else {
                let convert = if c.kind == s::DefinitionKind::Element {
                    let id = ctx.effects.output(&body.name, &o.name);
                    format!(
                        "<super::__organon_values::__errors_{id} as ::organon_theoria::effects::Port<{}>>::pack(result,super::__organon_registry)",
                        ty(t.kind())
                    )
                } else {
                    format!(
                        "Ok({})",
                        emit.convert(t.declared().expect("interface output contract"), "result")?
                    )
                };
                writeln!(sources,"let output{index}=flow::output(locals.clone(),{:?},|result|{convert},context)?;",o.binding).unwrap();
            }
        }
        writeln!(init, "r#{n}:output{index}.clone(),").unwrap();
    }
    code.push_str("}#[allow(unused_variables,unused_mut)] fn bind_graph(context:&mut impl ::organon_theoria::BindingContext,inputs:&Sources)->Result<Outputs,::organon_theoria::BuildError>{let reactive=context.reactive();");
    code.push_str("let mut root_inputs=std::collections::BTreeMap::<String,::organon_theoria::Source<Value>>::new();");
    for p in c.inputs.iter().filter(|p| !p.ty.fallible()) {
        writeln!(
            code,
            "root_inputs.insert({:?}.into(),inputs.r#{}.clone().map(|v|Value::new(v.clone())));",
            p.name, p.name
        )
        .unwrap();
    }
    write!(
        code,
        "let mut locals=flow::roots({});",
        emit.bindings(&schema, &body.bindings, true)?
    )
    .unwrap();
    for p in inputs {
        if let Some(port) = c
            .inputs
            .iter()
            .find(|i| i.name == p.name && i.ty.fallible())
        {
            let value = emit.unpack(&port.ty, "source.read(cx).into_cloned()")?;
            writeln!(code, "{{let source=inputs.r#{}.clone();locals.insert({:?}.into(),Rc::new(move|_,cx|{value}));}}",p.name,p.name).unwrap();
            continue;
        }
        write!(
            code,
            "locals.insert({:?}.into(),flow::input(inputs.r#{}.clone()));",
            p.name, p.name
        )
        .unwrap();
    }
    write!(
        code,
        "let locals=Rc::new(locals);{sources}Ok(Outputs{{{init}}})}}"
    )
    .unwrap();
    Ok(code)
}
