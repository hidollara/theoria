//! Preserve native attribute requirements until their parsers can run.
use super::super::*;
use std::collections::BTreeMap;

pub enum Effect {
    Union(Vec<usize>),
    Reasons(Vec<usize>),
    Attribute(usize),
    Filter { source: usize, reasons: Vec<usize> },
    When { condition: usize, value: usize },
}

pub struct Requirement {
    pub text: String,
    pub kind: Type,
    pub bindings: BTreeMap<String, (Type, usize)>,
}

pub struct Check {
    pub name: String,
    pub source: usize,
    pub allowed: Vec<usize>,
}

pub struct Graph {
    pub reasons: Vec<Type>,
    pub nodes: Vec<Effect>,
    pub attributes: Vec<Requirement>,
    pub outputs: BTreeMap<(String, String), usize>,
    pub checks: Vec<Check>,
    pub bindings: BTreeMap<String, Vec<usize>>,
    pub predicates: BTreeMap<String, usize>,
}
impl Graph {
    /// Evaluate using dependencies selected by each attribute parser.
    pub fn evaluate(&self, selected: &[Vec<bool>]) -> Vec<Vec<bool>> {
        assert_eq!(selected.len(), self.attributes.len());
        let sources: Vec<Vec<usize>> = self
            .attributes
            .iter()
            .map(|a| a.bindings.values().map(|(_, n)| *n).collect())
            .collect();
        let nodes: Vec<_> = self
            .nodes
            .iter()
            .map(|node| match node {
                Effect::Union(v) => super::Node::Union(v),
                Effect::Reasons(v) => super::Node::Reasons(v),
                Effect::Attribute(i) => super::Node::References {
                    selected: &selected[*i],
                    sources: &sources[*i],
                },
                Effect::Filter { source, reasons } => super::Node::Filter {
                    source: *source,
                    reasons,
                },
                Effect::When { condition, value } => super::Node::When {
                    condition: *condition,
                    value: *value,
                },
            })
            .collect();
        super::evaluate::evaluate_dynamic(&nodes, self.reasons.len())
    }
}
type Scope = BTreeMap<String, (Type, usize)>;

struct Build<'a> {
    program: &'a Program,
    graph: Graph,
}

/// No grammar is assumed for attributes, including attributes returning a block's result.
pub fn graph(program: &Program) -> Graph {
    let mut reasons = std::collections::BTreeSet::new();
    for c in program.contracts.values() {
        for p in &c.inputs {
            reasons.extend(p.ty.errors.iter().cloned());
        }
        for p in &c.outputs {
            if let Some(ty) = p.ty.declared() {
                reasons.extend(ty.errors.iter().cloned());
            }
        }
    }
    for p in program.predicates.values() {
        if let PredicateBody::Declared { errors } = &p.body {
            reasons.extend(errors.iter().cloned());
        }
        for i in &p.inputs {
            reasons.extend(i.ty.errors.iter().cloned());
        }
    }
    // Throw-only reasons may not appear in an exported port (e.g. a fully caught failure).
    fn throws(e: &Expression, reasons: &mut std::collections::BTreeSet<Type>) {
        fn block(b: &Block, reasons: &mut std::collections::BTreeSet<Type>) {
            if b.throwing
                && let Argument::Attribute { expected, .. } | Argument::Reference { expected, .. } =
                    &b.result
            {
                reasons.insert(expected.kind.clone());
            }
            for v in &b.bindings {
                throws(&v.value, reasons);
            }
        }
        match &e.kind {
            ExpressionKind::Select(v) => {
                for b in v {
                    block(&b.body, reasons);
                }
            }
            ExpressionKind::Try { body, handlers } => {
                block(body, reasons);
                for h in handlers {
                    if let HandlerAction::Catch { branch, .. } = &h.action {
                        block(&branch.body, reasons);
                    }
                }
            }
            ExpressionKind::Construct { children, .. } => {
                for c in children {
                    if let Child::Value(e) = c {
                        throws(e, reasons);
                    }
                }
            }
            _ => (),
        }
    }
    for b in &program.bodies {
        for v in &b.bindings {
            throws(&v.value, &mut reasons);
        }
    }
    for p in program.predicates.values() {
        if let PredicateBody::Defined { bindings: v, .. } = &p.body {
            for b in v {
                throws(&b.value, &mut reasons);
            }
        }
    }
    let mut b = Build {
        program,
        graph: Graph {
            reasons: reasons.into_iter().collect(),
            nodes: vec![],
            attributes: vec![],
            outputs: BTreeMap::new(),
            checks: vec![],
            bindings: BTreeMap::new(),
            predicates: BTreeMap::new(),
        },
    };
    for (name, c) in &program.contracts {
        for p in &c.outputs {
            let id = match &p.ty {
                OutputType::Inferred(_) => b.node(Effect::Union(vec![])),
                OutputType::Declared(ty) => b.reasons(&ty.errors),
            };
            b.graph
                .outputs
                .insert((name.clone(), p.name.to_string()), id);
        }
    }
    for name in program.predicates.keys() {
        let id = b.node(Effect::Union(vec![]));
        b.graph.predicates.insert(name.clone(), id);
    }
    for (name, p) in &program.predicates {
        let node = match &p.body {
            PredicateBody::Defined {
                bindings: locals,
                condition,
            } => {
                let env = b.inputs(&p.inputs);
                let env = b.locals(locals, &env);
                b.condition(condition, &env)
            }
            PredicateBody::Declared { errors } => b.reasons(errors),
        };
        b.graph.nodes[b.graph.predicates[name]] = Effect::Union(vec![node]);
    }
    for body in &program.bodies {
        let env = b.inputs(&program.contracts[&body.name].inputs);
        let env = b.locals(&body.bindings, &env);
        b.graph.bindings.insert(
            body.name.clone(),
            body.bindings.iter().map(|v| env[&v.name].1).collect(),
        );
        for o in &body.outputs {
            let target = b.graph.outputs[&(body.name.clone(), o.name.clone())];
            b.graph.nodes[target] = Effect::Union(vec![env[&o.binding].1]);
        }
        for imp in &body.implementations {
            for o in &imp.outputs {
                let port = program.contracts[&imp.interface]
                    .outputs
                    .iter()
                    .find(|p| p.name == o.name)
                    .unwrap();
                let allowed = b.covered(
                    &port
                        .ty
                        .declared()
                        .expect("interface output contract")
                        .errors,
                );
                b.graph.checks.push(Check {
                    name: format!("{}.{} must satisfy {}", body.name, o.name, imp.interface),
                    source: env[&o.binding].1,
                    allowed,
                });
            }
        }
    }
    b.graph
}

impl Build<'_> {
    fn node(&mut self, e: Effect) -> usize {
        let id = self.graph.nodes.len();
        self.graph.nodes.push(e);
        id
    }
    fn reasons(&mut self, reasons: &[Type]) -> usize {
        let ids = reasons
            .iter()
            .map(|r| self.graph.reasons.iter().position(|e| e == r).unwrap())
            .collect();
        self.node(Effect::Reasons(ids))
    }
    fn covers(&self, actual: &Type, allowed: &Type) -> bool {
        actual == allowed
            || self.program.contracts[actual.name()]
                .implements
                .iter()
                .any(|i| i == allowed.name())
    }
    fn covered(&self, allowed: &[Type]) -> Vec<usize> {
        self.graph
            .reasons
            .iter()
            .enumerate()
            .filter_map(|(i, r)| allowed.iter().any(|t| self.covers(r, t)).then_some(i))
            .collect()
    }
    fn inputs(&mut self, inputs: &[Port]) -> Scope {
        inputs
            .iter()
            .map(|p| {
                (
                    p.name.to_string(),
                    (p.ty.kind.clone(), self.reasons(&p.ty.errors)),
                )
            })
            .collect()
    }
    fn locals(&mut self, values: &[Binding], outer: &Scope) -> Scope {
        let mut env = outer.clone();
        for v in values {
            let id = self.node(Effect::Union(vec![]));
            env.insert(v.name.clone(), (v.value.ty.clone(), id));
        }
        for v in values {
            let id = self.expr(&v.value, &env);
            self.graph.nodes[env[&v.name].1] = Effect::Union(vec![id]);
        }
        env
    }
    fn argument(&mut self, arg: &Argument, env: &Scope) -> usize {
        match arg {
            Argument::Missing { .. } => self.node(Effect::Union(vec![])),
            Argument::Reference { name, .. } => env[name].1,
            Argument::Attribute { text, expected, .. } => {
                let index = self.graph.attributes.len();
                self.graph.attributes.push(Requirement {
                    text: text.clone(),
                    kind: expected.kind.clone(),
                    bindings: env.clone(),
                });
                self.node(Effect::Attribute(index))
            }
        }
    }
    fn escaping(&mut self, arg: &Argument, env: &Scope) -> usize {
        let source = self.argument(arg, env);
        let (Argument::Reference { expected, .. } | Argument::Attribute { expected, .. }) = arg
        else {
            return source;
        };
        let covered = self.covered(&expected.errors);
        let reasons = (0..self.graph.reasons.len())
            .filter(|i| !covered.contains(i))
            .collect();
        self.node(Effect::Filter { source, reasons })
    }
    fn condition(&mut self, c: &Condition, env: &Scope) -> usize {
        let nodes = match c {
            Condition::All(v) | Condition::Any(v) => {
                v.iter().map(|c| self.condition(c, env)).collect()
            }
            Condition::Not(c) => vec![self.condition(c, env)],
            Condition::Call {
                name, arguments, ..
            } => {
                let mut nodes = vec![self.graph.predicates[name]];
                for a in arguments.values() {
                    nodes.push(self.escaping(a, env));
                }
                nodes
            }
        };
        self.node(Effect::Union(nodes))
    }
    fn block(&mut self, b: &Block, guard: Option<&Condition>, env: &Scope) -> usize {
        let env = self.locals(&b.bindings, env);
        let mut nodes = vec![self.argument(&b.result, &env)];
        if b.throwing {
            let (Argument::Reference {
                expected: TypeContract { kind, .. },
                ..
            }
            | Argument::Attribute {
                expected: TypeContract { kind, .. },
                ..
            }) = &b.result
            else {
                unreachable!()
            };
            nodes.push(self.reasons(std::slice::from_ref(kind)));
        }
        if let Some(c) = guard {
            nodes.push(self.condition(c, &env));
        }
        self.node(Effect::Union(nodes))
    }
    fn expr(&mut self, e: &Expression, env: &Scope) -> usize {
        let nodes = match &e.kind {
            ExpressionKind::Reference(n) | ExpressionKind::AsInterface(n) => vec![env[n].1],
            ExpressionKind::Projection { parent, output, .. } => vec![
                env[parent].1,
                self.graph.outputs[&(env[parent].0.name().to_owned(), output.clone())],
            ],
            ExpressionKind::Construct {
                arguments,
                children,
                ..
            } => {
                let mut nodes: Vec<_> = arguments.values().map(|a| self.escaping(a, env)).collect();
                for c in children {
                    if let Child::Value(e) = c {
                        nodes.push(self.expr(e, env));
                    }
                }
                nodes
            }
            ExpressionKind::Select(cases) => cases
                .iter()
                .map(|b| self.block(&b.body, b.condition.as_ref(), env))
                .collect(),
            ExpressionKind::Try { body, handlers } => {
                let mut uncaught = self.block(body, None, env);
                let mut nodes = vec![];
                for h in handlers {
                    let matching = self
                        .graph
                        .reasons
                        .iter()
                        .enumerate()
                        .filter_map(|(i, r)| {
                            h.reason
                                .as_ref()
                                // An interface failure exposes that contract, not its
                                // original implementation's concrete type.
                                .is_none_or(|t| self.covers(r, t))
                                .then_some(i)
                        })
                        .collect();
                    let condition = self.node(Effect::Filter {
                        source: uncaught,
                        reasons: matching,
                    });
                    let unconditional = match &h.action {
                        HandlerAction::Unreachable => true,
                        HandlerAction::Catch { name, branch } => {
                            let mut env = env.clone();
                            if let (Some(n), Some(t)) = (name, &h.reason) {
                                let id = self.node(Effect::Union(vec![]));
                                env.insert(n.clone(), (t.clone(), id));
                            }
                            let value = self.block(&branch.body, branch.condition.as_ref(), &env);
                            nodes.push(self.node(Effect::When { condition, value }));
                            branch.condition.is_none()
                        }
                    };
                    if unconditional {
                        let remaining = self
                            .graph
                            .reasons
                            .iter()
                            .enumerate()
                            .filter_map(|(i, r)| {
                                h.reason
                                    .as_ref()
                                    .is_some_and(|t| !self.covers(r, t))
                                    .then_some(i)
                            })
                            .collect();
                        uncaught = self.node(Effect::Filter {
                            source: uncaught,
                            reasons: remaining,
                        });
                    }
                }
                nodes.push(uncaught);
                nodes
            }
        };
        self.node(Effect::Union(nodes))
    }
}
