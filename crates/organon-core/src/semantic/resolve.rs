use super::*;
use crate::{
    syntax,
    types::{Port, TypeName, ValueType},
};
use std::collections::{BTreeMap, HashSet};

pub fn resolve(module: &syntax::Module) -> Result<Program, Error> {
    module.validate()?;
    if module.sources().next().is_some() {
        return Err(Error("resolve imports before semantic analysis".into()));
    }
    let mut resolver = Resolver {
        definitions: BTreeMap::new(),
        contracts: BTreeMap::new(),
        active: HashSet::new(),
        predicates: BTreeMap::new(),
        bodies: vec![],
    };
    for d in module.definitions() {
        if resolver.definitions.insert(d.name().into(), d).is_some() {
            return Err(Error(format!("duplicate definition {:?}", d.name())));
        }
    }
    for d in module.definitions() {
        match d {
            syntax::Definition::DeclareElement(v) => {
                if let Some(c) = v.children.as_ref() {
                    resolver.check_type(&c.kind)?;
                }
                let inputs: Vec<_> = v.inputs().cloned().collect();
                let outputs: Vec<_> = v.outputs().cloned().collect();
                resolver.check_ports(&inputs)?;
                resolver.check_ports(&outputs)?;
                let mut seen = HashSet::new();
                for name in v.implements() {
                    if !seen.insert(name)
                        || !matches!(
                            resolver.definitions.get(name),
                            Some(syntax::Definition::Interface(_))
                        )
                    {
                        return Err(Error(format!("invalid implements {name:?}")));
                    }
                }
                resolver.contracts.insert(
                    v.name.to_string(),
                    Contract {
                        kind: DefinitionKind::DeclareElement { tag: v.tag },
                        inputs,
                        optional_inputs: v
                            .inputs()
                            .filter(|p| v.is_optional_input(&p.name))
                            .map(|p| p.name.to_string())
                            .collect(),
                        children: v.children.as_ref().cloned(),
                        outputs: outputs
                            .into_iter()
                            .map(|p| Port {
                                name: p.name,
                                ty: OutputType::Declared(p.ty),
                            })
                            .collect(),
                        implements: v.implements().map(str::to_owned).collect(),
                    },
                );
            }
            syntax::Definition::Interface(v) => {
                resolver.check_ports(&v.outputs)?;
                resolver.contracts.insert(
                    v.name.to_string(),
                    Contract {
                        kind: DefinitionKind::Interface,
                        inputs: vec![],
                        optional_inputs: Default::default(),
                        children: None,
                        outputs: v
                            .outputs
                            .iter()
                            .map(|p| Port {
                                name: p.name.clone(),
                                ty: OutputType::Declared(p.ty.clone()),
                            })
                            .collect(),
                        implements: vec![],
                    },
                );
            }
            _ => (),
        }
    }
    for d in module.definitions() {
        match d {
            syntax::Definition::Predicate(_) | syntax::Definition::DeclarePredicate(_) => {
                resolver.ensure_predicate(d.name())?
            }
            _ => resolver.ensure(d.name())?,
        }
    }
    let program = Program {
        predicates: resolver.predicates,
        contracts: resolver.contracts,
        bodies: resolver.bodies,
    };
    super::effects::validate_known(&program)?;
    Ok(program)
}
struct Resolver<'a> {
    definitions: BTreeMap<String, &'a syntax::Definition>,
    contracts: BTreeMap<String, Contract>,
    active: HashSet<String>,
    bodies: Vec<Body>,
    predicates: BTreeMap<String, Predicate>,
}
impl Resolver<'_> {
    fn check_type(&self, ty: &TypeName) -> Result<(), Error> {
        if matches!(
            self.definitions.get(ty.spelling()),
            Some(
                syntax::Definition::Element(_)
                    | syntax::Definition::DeclareElement(_)
                    | syntax::Definition::Interface(_)
            )
        ) {
            Ok(())
        } else {
            Err(Error(format!("unknown type {:?}", ty.spelling())))
        }
    }
    fn check_ports(&self, ports: &[Port]) -> Result<(), Error> {
        let mut seen = HashSet::new();
        for p in ports {
            self.check_type(&p.ty.kind)?;
            let mut errors = HashSet::new();
            for reason in &p.ty.errors {
                self.check_type(reason)?;
                if !errors.insert(reason.spelling()) {
                    return Err(Error(format!("duplicate error {}", reason.spelling())));
                }
            }
            if !seen.insert(&p.name) {
                return Err(Error(format!("duplicate port {:?}", p.name)));
            }
        }
        Ok(())
    }
    fn ensure(&mut self, name: &str) -> Result<(), Error> {
        if self.contracts.contains_key(name) {
            return Ok(());
        }
        let Some(syntax::Definition::Element(e)) = self.definitions.get(name).copied() else {
            return Err(Error(format!("unknown element {name:?}")));
        };
        if !self.active.insert(name.into()) {
            return Err(Error(format!("cyclic element dependency {name:?}")));
        }
        let mut scope = Scope::new(e)?;
        if let Some(children) = &e.children {
            self.check_type(&children.kind)?;
        }
        let inputs: Vec<_> = e.inputs().cloned().collect();
        self.check_ports(&inputs)?;
        for name in scope.declarations.keys().cloned().collect::<Vec<_>>() {
            self.binding(&name, &mut scope)?;
        }
        let mut outputs = vec![];
        let mut output_contracts = vec![];
        let mut names = HashSet::new();
        for provided in e.provides() {
            if !names.insert(provided.name.as_str()) {
                return Err(Error("duplicate output".into()));
            }
            let ty = self.binding(&provided.reference, &mut scope)?;
            output_contracts.push(Port {
                name: provided.name.clone(),
                ty: OutputType::Inferred(ty),
            });
            outputs.push(OutputBinding {
                name: provided.name.to_string(),
                binding: provided.reference.to_string(),
            });
        }
        let mut implementations = vec![];
        let mut seen = HashSet::new();
        for (origin, member) in e.members.iter().enumerate() {
            let syntax::ElementMember::Impl(imp) = member else {
                continue;
            };
            self.ensure(&imp.interface)?;
            let contract = self.contracts[imp.interface.as_str()].clone();
            if !matches!(
                self.definitions.get(imp.interface.as_str()),
                Some(syntax::Definition::Interface(_))
            ) || !seen.insert(&imp.interface)
            {
                return Err(Error(format!("invalid impl {:?}", imp.interface)));
            }
            let mut outputs = vec![];
            let mut provided = HashSet::new();
            for p in &imp.provides {
                let expected = contract
                    .outputs
                    .iter()
                    .find(|o| o.name == p.name)
                    .ok_or_else(|| Error(format!("unknown interface output {:?}", p.name)))?;
                let value = self.reference(&p.reference, Some(expected.ty.kind()), &mut scope)?;
                if !provided.insert(&p.name) {
                    return Err(Error(format!("invalid provide {:?}", p.name)));
                }
                let binding = if matches!(value.kind, ExpressionKind::AsInterface(_)) {
                    let mut name = format!("__organon_provide_{origin}_{}", p.name);
                    while scope.contains(&name) {
                        name.push('_');
                    }
                    scope.bindings.push(Binding {
                        name: name.clone(),
                        origin,
                        value,
                    });
                    name
                } else {
                    p.reference.to_string()
                };
                outputs.push(OutputBinding {
                    name: p.name.to_string(),
                    binding,
                });
            }
            if outputs.len() != contract.outputs.len() {
                return Err(Error("incomplete interface implementation".into()));
            }
            implementations.push(Implementation {
                interface: imp.interface.to_string(),
                outputs,
            });
        }
        if outputs.is_empty() && implementations.is_empty() {
            return Err(Error("element requires a provide or impl".into()));
        }
        self.contracts.insert(
            name.into(),
            Contract {
                kind: DefinitionKind::Element,
                optional_inputs: Default::default(),
                children: e.children.clone(),
                inputs,
                outputs: output_contracts,
                implements: implementations
                    .iter()
                    .map(|i| i.interface.clone())
                    .collect(),
            },
        );
        self.bodies.push(Body {
            name: name.into(),
            bindings: scope.bindings,
            outputs,
            implementations,
        });
        self.active.remove(name);
        Ok(())
    }
    fn binding(&mut self, name: &str, scope: &mut Scope) -> Result<TypeName, Error> {
        if let Some(ty) = scope.types.get(name) {
            return Ok(ty.clone());
        }
        if !scope.declarations.contains_key(name)
            && let Some(outer) = &mut scope.outer
        {
            return self.binding(name, outer);
        }
        let (origin, decl) = scope
            .declarations
            .get(name)
            .cloned()
            .ok_or_else(|| Error(format!("unknown binding {name:?}")))?;
        if !scope.active.insert(name.into()) {
            return Err(Error(format!("cyclic binding {name:?}")));
        }
        let value = match decl {
            syntax::LocalBinding::Let(v) => self.expression(&v.value, scope)?,
            syntax::LocalBinding::Bind(v) => {
                // Anonymous construction is materialized once, shared by every projection.
                let parent =
                    if let syntax::ValueExpression::Reference { name: n, ty: None } = &v.value {
                        n.to_string()
                    } else {
                        if let Some(n) = scope.parents.get(&origin) {
                            n.clone()
                        } else {
                            let expression = self.expression(&v.value, scope)?;
                            let mut n = format!("__organon_bind_{origin}");
                            while scope.contains(&n) {
                                n.push('_');
                            }
                            scope.types.insert(n.clone(), expression.ty.clone());
                            scope.parents.insert(origin, n.clone());
                            scope.bindings.push(Binding {
                                name: n.clone(),
                                origin,
                                value: expression,
                            });
                            n
                        }
                    };
                let ty = self.binding(&parent, scope)?;
                self.ensure(ty.spelling())?;
                let output = v
                    .outputs
                    .iter()
                    .find(|(_, n)| n.as_str() == name)
                    .unwrap()
                    .0;
                let result = self.contracts[ty.spelling()]
                    .outputs
                    .iter()
                    .find(|o| &o.name == output)
                    .ok_or_else(|| Error(format!("unknown output {output:?}")))?
                    .ty
                    .kind()
                    .clone();
                Expression {
                    ty: result,
                    kind: ExpressionKind::Projection {
                        parent,
                        output: output.to_string(),
                    },
                }
            }
        };
        let ty = value.ty.clone();
        scope.types.insert(name.into(), ty.clone());
        scope.active.remove(name);
        scope.bindings.push(Binding {
            name: name.into(),
            origin,
            value,
        });
        Ok(ty)
    }
    fn satisfies(&self, actual: &TypeName, target: &TypeName) -> bool {
        actual == target
            || match self.definitions.get(actual.spelling()) {
                Some(syntax::Definition::Element(e)) => e
                    .implementations()
                    .any(|i| i.interface == target.spelling()),
                Some(syntax::Definition::DeclareElement(d)) => {
                    d.implements().any(|i| i == target.spelling())
                }
                _ => false,
            }
    }

    fn reference(
        &mut self,
        name: &str,
        target: Option<&TypeName>,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        let mut ty = self.binding(name, scope)?;
        let Some(target) = target.filter(|target| *target != &ty) else {
            return Ok(Expression {
                ty,
                kind: ExpressionKind::Reference(name.into()),
            });
        };
        self.check_type(target)?;
        if !self.satisfies(&ty, target) {
            return Err(Error(format!(
                "{} does not implement {}",
                ty.spelling(),
                target.spelling()
            )));
        }
        ty = target.clone();
        Ok(Expression {
            ty,
            kind: ExpressionKind::AsInterface(name.into()),
        })
    }
    fn expression(
        &mut self,
        value: &syntax::ValueExpression,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        match value {
            syntax::ValueExpression::Select(select) => return self.select(select, scope),
            syntax::ValueExpression::Try(attempt) => return self.attempt(attempt, scope),
            _ => (),
        }
        if matches!(value, syntax::ValueExpression::Children) {
            return Err(Error(
                "children is a sequence and can only appear in a construction's children".into(),
            ));
        }
        let syntax::ValueExpression::Construct(i) = value else {
            let syntax::ValueExpression::Reference { name, ty } = value else {
                unreachable!()
            };
            return self.reference(name, ty.as_ref(), scope);
        };
        self.ensure(&i.element)?;
        let element_contract = self.contracts[i.element.as_str()].clone();
        if !element_contract.constructible() {
            return Err(Error(format!(
                "{} cannot be constructed as a tag",
                i.element
            )));
        }
        let inputs = element_contract.inputs;
        let result = TypeName::parse(&i.element)?;
        if i.arguments
            .keys()
            .any(|n| !inputs.iter().any(|p| &p.name == n))
        {
            return Err(Error(format!("{}: unknown input", i.element)));
        }
        let mut arguments = BTreeMap::new();
        for port in inputs {
            let text = match i.arguments.get(&port.name) {
                Some(text) => text,
                None if element_contract
                    .optional_inputs
                    .contains(port.name.as_str()) =>
                {
                    arguments.insert(
                        port.name.into_string(),
                        Argument::Missing { expected: port.ty },
                    );
                    continue;
                }
                None => return Err(Error(format!("missing input {:?}", port.name))),
            };
            arguments.insert(
                port.name.into_string(),
                self.argument(text, &port.ty, scope)?,
            );
        }
        let mut children = vec![];
        let mut minimum = 0usize;
        let mut maximum = Some(0usize);
        for child in &i.children {
            let contract = element_contract
                .children
                .as_ref()
                .ok_or_else(|| Error(format!("{} does not accept children", i.element)))?;
            if matches!(child, syntax::ValueExpression::Children) {
                let incoming = scope.children.as_ref().ok_or_else(|| {
                    Error("children requires a child contract on the enclosing element".into())
                })?;
                if !self.satisfies(&incoming.kind, &contract.kind) {
                    return Err(Error("incompatible incoming child type".into()));
                }
                minimum = minimum
                    .checked_add(incoming.minimum)
                    .ok_or_else(|| Error("child count overflow".into()))?;
                maximum = maximum
                    .zip(incoming.maximum)
                    .and_then(|(a, b)| a.checked_add(b));
                children.push(Child::Incoming {
                    kind: incoming.kind.clone(),
                });
                continue;
            }
            let child = self.expression(child, scope)?;
            if !self.satisfies(&child.ty, &contract.kind) {
                return Err(Error("incompatible child type".into()));
            }

            minimum = minimum
                .checked_add(1)
                .ok_or_else(|| Error("child count overflow".into()))?;
            maximum = maximum.and_then(|n| n.checked_add(1));
            children.push(Child::Value(child));
        }
        if let Some(contract) = &element_contract.children
            && (minimum < contract.minimum
                || contract
                    .maximum
                    .is_some_and(|limit| maximum.is_none_or(|n| n > limit)))
        {
            return Err(Error(format!("{}: invalid child count range", i.element)));
        }
        Ok(Expression {
            ty: result,
            kind: ExpressionKind::Construct {
                name: i.element.to_string(),
                arguments,
                children,
            },
        })
    }
}
#[derive(Default)]
struct Scope {
    outer: Option<Box<Scope>>,
    children: Option<crate::types::ChildrenType>,
    declarations: BTreeMap<String, (usize, syntax::LocalBinding)>,
    types: BTreeMap<String, TypeName>,
    bindings: Vec<Binding>,
    active: HashSet<String>,
    parents: BTreeMap<usize, String>,
}
impl Scope {
    fn contains(&self, name: &str) -> bool {
        self.types.contains_key(name)
            || self.declarations.contains_key(name)
            || self.outer.as_ref().is_some_and(|s| s.contains(name))
    }

    fn local(&mut self, origin: usize, local: &syntax::LocalBinding) -> Result<(), Error> {
        let mut insert = |name: &crate::identifier::Identifier| {
            if self.contains(name) {
                return Err(Error(format!("duplicate or shadowed binding {name:?}")));
            }
            self.declarations
                .insert(name.to_string(), (origin, local.clone()));
            Ok(())
        };
        match local {
            syntax::LocalBinding::Let(v) => insert(&v.name),
            syntax::LocalBinding::Bind(v) => v.outputs.values().try_for_each(insert),
        }
    }

    fn new(e: &syntax::Element) -> Result<Self, Error> {
        let mut result = Self {
            children: e.children.clone(),
            ..Self::default()
        };
        for (index, member) in e.members.iter().enumerate() {
            match member {
                syntax::ElementMember::Input(input) => {
                    if result.contains(&input.name) {
                        return Err(Error(format!("duplicate binding {:?}", input.name)));
                    }
                    result
                        .types
                        .insert(input.name.to_string(), input.ty.kind.clone());
                }
                syntax::ElementMember::Let(value) => {
                    result.local(index, &syntax::LocalBinding::Let(value.clone()))?;
                }
                syntax::ElementMember::Bind(value) => {
                    result.local(index, &syntax::LocalBinding::Bind(value.clone()))?;
                }
                syntax::ElementMember::Provide(_) | syntax::ElementMember::Impl(_) => {}
            }
        }
        Ok(result)
    }
}

#[path = "control.rs"]
mod control;
