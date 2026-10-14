use crate::syntax;
use crate::{Error, semantic::*};
use std::collections::{BTreeMap, HashSet};

pub(super) struct Definition<'a> {
    pub syntax: &'a syntax::Definition,
    pub names: &'a BTreeMap<String, String>,
    pub source: &'a dyn std::fmt::Debug,
}
pub(super) fn resolve(definitions: BTreeMap<String, Definition<'_>>) -> Result<Program, Error> {
    let mut resolver = Resolver {
        definitions,
        current: String::new(),
        contracts: BTreeMap::new(),
        active: HashSet::new(),
        predicates: BTreeMap::new(),
        bodies: vec![],
    };
    let keys = resolver.definitions.keys().cloned().collect::<Vec<_>>();
    for key in &keys {
        resolver.current = key.clone();
        let d = resolver.definitions[key].syntax;
        let result = (|| {
            match d {
                syntax::Definition::DeclareElement(v) => {
                    let children = resolver.resolve_children(v.children.as_ref())?;
                    let inputs = resolver.resolve_ports(v.inputs())?;
                    let outputs = resolver.resolve_ports(v.outputs())?;
                    let mut seen = HashSet::new();
                    let mut implements = Vec::new();
                    for name in v.implements() {
                        let name = resolver.lookup(name)?;
                        if !seen.insert(name.clone())
                            || !matches!(
                                resolver.definitions.get(&name).map(|d| d.syntax),
                                Some(syntax::Definition::Interface(_))
                            )
                        {
                            return Err(Error(format!("invalid implements {name:?}")));
                        }
                        implements.push(name);
                    }
                    resolver.contracts.insert(
                        key.clone(),
                        Contract {
                            kind: DefinitionKind::DeclareElement { tag: v.tag },
                            inputs,
                            optional_inputs: v
                                .inputs()
                                .filter(|p| v.is_optional_input(&p.name))
                                .map(|p| p.name.to_string())
                                .collect(),
                            children,
                            outputs: outputs
                                .into_iter()
                                .map(|p| Port {
                                    name: p.name,
                                    ty: OutputType::Declared(p.ty),
                                })
                                .collect(),
                            implements,
                        },
                    );
                }
                syntax::Definition::Interface(v) => {
                    let outputs = resolver.resolve_ports(&v.outputs)?;
                    resolver.contracts.insert(
                        key.clone(),
                        Contract {
                            kind: DefinitionKind::Interface,
                            inputs: vec![],
                            optional_inputs: Default::default(),
                            children: None,
                            outputs: outputs
                                .into_iter()
                                .map(|p| Port {
                                    name: p.name,
                                    ty: OutputType::Declared(p.ty),
                                })
                                .collect(),
                            implements: vec![],
                        },
                    );
                }
                _ => (),
            }
            Ok(())
        })();
        result.map_err(|e| resolver.contextualize(e))?;
    }
    for key in &keys {
        let result = match resolver.definitions[key].syntax {
            syntax::Definition::Predicate(_) | syntax::Definition::DeclarePredicate(_) => {
                resolver.ensure_predicate(key)
            }
            _ => resolver.ensure(key),
        };
        result.map_err(|e| resolver.contextualize(e))?;
    }
    let program = Program {
        predicates: resolver.predicates,
        contracts: resolver.contracts,
        bodies: resolver.bodies,
    };
    crate::semantic::effects::validate_known(&program)?;
    Ok(program)
}
struct Resolver<'a> {
    definitions: BTreeMap<String, Definition<'a>>,
    current: String,
    contracts: BTreeMap<String, Contract>,
    active: HashSet<String>,
    bodies: Vec<Body>,
    predicates: BTreeMap<String, Predicate>,
}
impl Resolver<'_> {
    fn contextualize(&self, error: Error) -> Error {
        let definition = &self.definitions[&self.current];
        Error(format!(
            "{:?}: {}: {error}",
            definition.source,
            definition.syntax.name()
        ))
    }
    fn lookup(&self, name: &str) -> Result<String, Error> {
        self.definitions[&self.current]
            .names
            .get(name)
            .cloned()
            .ok_or_else(|| Error(format!("unknown definition {name:?}; import it before use")))
    }

    fn resolve_type(&self, ty: &syntax::TypeName) -> Result<Type, Error> {
        let key = self
            .lookup(ty.spelling())
            .map_err(|e| Error(format!("unknown type {:?}: {e}", ty.spelling())))?;
        if matches!(
            self.definitions.get(&key).map(|d| d.syntax),
            Some(
                syntax::Definition::Element(_)
                    | syntax::Definition::DeclareElement(_)
                    | syntax::Definition::Interface(_)
            )
        ) {
            Ok(Type::named(key))
        } else {
            Err(Error(format!("unknown type {:?}", ty.spelling())))
        }
    }
    fn resolve_ports<'a>(
        &self,
        ports: impl IntoIterator<Item = &'a syntax::Port>,
    ) -> Result<Vec<Port>, Error> {
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for p in ports {
            let kind = self.resolve_type(&p.ty.kind)?;
            let mut errors = HashSet::new();
            let mut reasons = Vec::new();
            for reason in &p.ty.errors {
                let resolved = self.resolve_type(reason)?;
                if !errors.insert(resolved.clone()) {
                    return Err(Error(format!("duplicate error {}", reason.spelling())));
                }
                reasons.push(resolved);
            }
            if !seen.insert(&p.name) {
                return Err(Error(format!("duplicate port {:?}", p.name)));
            }
            result.push(Port {
                name: p.name.to_string(),
                ty: TypeContract {
                    kind,
                    errors: reasons,
                },
            });
        }
        Ok(result)
    }

    fn resolve_children(
        &self,
        children: Option<&syntax::ChildrenType>,
    ) -> Result<Option<ChildrenContract>, Error> {
        children
            .map(|children| {
                let kind = self.resolve_type(&children.kind)?;
                let (minimum, maximum) = match children.count {
                    syntax::ChildCount::One => (1, Some(1)),
                    syntax::ChildCount::Optional => (0, Some(1)),
                    syntax::ChildCount::Many => (0, None),
                };
                Ok(ChildrenContract {
                    kind,
                    minimum,
                    maximum,
                })
            })
            .transpose()
    }
    fn ensure(&mut self, name: &str) -> Result<(), Error> {
        if self.contracts.contains_key(name) {
            return Ok(());
        }
        let Some(syntax::Definition::Element(e)) = self.definitions.get(name).map(|d| d.syntax)
        else {
            return Err(Error(format!("unknown element {name:?}")));
        };
        if !self.active.insert(name.into()) {
            return Err(Error(format!("cyclic element dependency {name:?}")));
        }
        let previous = std::mem::replace(&mut self.current, name.to_owned());
        let children = self.resolve_children(e.children.as_ref())?;
        let inputs = self.resolve_ports(e.inputs())?;
        let mut scope = Scope::new(e, &inputs, children.clone())?;
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
                name: provided.name.to_string(),
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
            let interface = self.lookup(&imp.interface)?;
            self.ensure(&interface)?;
            let contract = self.contracts[&interface].clone();
            if !matches!(
                self.definitions.get(&interface).map(|d| d.syntax),
                Some(syntax::Definition::Interface(_))
            ) || !seen.insert(interface.clone())
            {
                return Err(Error(format!("invalid impl {:?}", imp.interface)));
            }
            let mut outputs = vec![];
            let mut provided = HashSet::new();
            for p in &imp.provides {
                let expected = contract
                    .outputs
                    .iter()
                    .find(|o| o.name == p.name.as_str())
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
            implementations.push(Implementation { interface, outputs });
        }
        if outputs.is_empty() && implementations.is_empty() {
            return Err(Error("element requires a provide or impl".into()));
        }
        self.contracts.insert(
            name.into(),
            Contract {
                kind: DefinitionKind::Element,
                optional_inputs: Default::default(),
                children,
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
        self.current = previous;
        Ok(())
    }
    fn binding(&mut self, name: &str, scope: &mut Scope) -> Result<Type, Error> {
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
            syntax::Binding::Let(v) => self.expression(&v.element, scope)?,
            syntax::Binding::Bind(v) => {
                // Anonymous construction is materialized once, shared by every projection.
                let parent = if let syntax::ElementExpression::Reference { name: n, ty: None } =
                    &v.element
                {
                    n.to_string()
                } else {
                    if let Some(n) = scope.parents.get(&origin) {
                        n.clone()
                    } else {
                        let expression = self.expression(&v.element, scope)?;
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
                self.ensure(ty.name())?;
                let output = v
                    .outputs
                    .iter()
                    .find(|(_, n)| n.as_str() == name)
                    .unwrap()
                    .0;
                let result = self.contracts[ty.name()]
                    .outputs
                    .iter()
                    .find(|o| o.name == output.as_str())
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
    fn satisfies(&self, actual: &Type, target: &Type) -> bool {
        if actual == target {
            return true;
        }
        let definition = &self.definitions[actual.name()];
        match definition.syntax {
            syntax::Definition::Element(e) => e.implementations().any(|i| {
                definition
                    .names
                    .get(i.interface.as_str())
                    .is_some_and(|name| name == target.name())
            }),
            syntax::Definition::DeclareElement(d) => d.implements().any(|i| {
                definition
                    .names
                    .get(i)
                    .is_some_and(|name| name == target.name())
            }),
            _ => false,
        }
    }

    fn reference(
        &mut self,
        name: &str,
        target: Option<&Type>,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        let mut ty = self.binding(name, scope)?;
        let Some(target) = target.filter(|target| *target != &ty) else {
            return Ok(Expression {
                ty,
                kind: ExpressionKind::Reference(name.into()),
            });
        };

        if !self.satisfies(&ty, target) {
            return Err(Error(format!(
                "{} does not implement {}",
                ty.name(),
                target.name()
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
        value: &syntax::ElementExpression,
        scope: &mut Scope,
    ) -> Result<Expression, Error> {
        match value {
            syntax::ElementExpression::Select(select) => return self.select(select, scope),
            syntax::ElementExpression::Try(attempt) => return self.attempt(attempt, scope),
            _ => (),
        }
        if matches!(value, syntax::ElementExpression::Children) {
            return Err(Error(
                "children is a sequence and can only appear in a construction's children".into(),
            ));
        }
        let syntax::ElementExpression::Construct(i) = value else {
            let syntax::ElementExpression::Reference { name, ty } = value else {
                unreachable!()
            };
            let ty = ty.as_ref().map(|ty| self.resolve_type(ty)).transpose()?;
            return self.reference(name, ty.as_ref(), scope);
        };
        let element = self
            .lookup(&i.element)
            .map_err(|e| Error(format!("unknown element {:?}: {e}", i.element)))?;
        self.ensure(&element)?;
        let element_contract = self.contracts[&element].clone();
        if !element_contract.constructible() {
            return Err(Error(format!(
                "{} cannot be constructed as a tag",
                i.element
            )));
        }
        let inputs = element_contract.inputs;
        let result = Type::named(element.clone());
        if i.arguments
            .keys()
            .any(|n| !inputs.iter().any(|p| p.name == n.as_str()))
        {
            return Err(Error(format!("{}: unknown input", i.element)));
        }
        let mut arguments = BTreeMap::new();
        for port in inputs {
            let text = match i.arguments.get(port.name.as_str()) {
                Some(text) => text,
                None if element_contract
                    .optional_inputs
                    .contains(port.name.as_str()) =>
                {
                    arguments.insert(port.name, Argument::Missing { expected: port.ty });
                    continue;
                }
                None => return Err(Error(format!("missing input {:?}", port.name))),
            };
            arguments.insert(port.name, self.argument(text, &port.ty, scope)?);
        }
        let mut children = vec![];
        let mut minimum = 0usize;
        let mut maximum = Some(0usize);
        for child in &i.children {
            let contract = element_contract
                .children
                .as_ref()
                .ok_or_else(|| Error(format!("{} does not accept children", i.element)))?;
            if matches!(child, syntax::ElementExpression::Children) {
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
            && !contract.accepts_range(minimum, maximum)
        {
            return Err(Error(format!("{}: invalid child count range", i.element)));
        }
        Ok(Expression {
            ty: result,
            kind: ExpressionKind::Construct {
                name: element,
                arguments,
                children,
            },
        })
    }
}
#[derive(Default)]
struct Scope {
    outer: Option<Box<Scope>>,
    children: Option<ChildrenContract>,
    declarations: BTreeMap<String, (usize, syntax::Binding)>,
    types: BTreeMap<String, Type>,
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

    fn local(&mut self, origin: usize, local: &syntax::Binding) -> Result<(), Error> {
        let mut insert = |name: &syntax::Identifier| {
            if self.contains(name) {
                return Err(Error(format!("duplicate or shadowed binding {name:?}")));
            }
            self.declarations
                .insert(name.to_string(), (origin, local.clone()));
            Ok(())
        };
        match local {
            syntax::Binding::Let(v) => insert(&v.name),
            syntax::Binding::Bind(v) => v.outputs.values().try_for_each(insert),
        }
    }

    fn new(
        e: &syntax::Element,
        inputs: &[Port],
        children: Option<ChildrenContract>,
    ) -> Result<Self, Error> {
        let mut result = Self {
            children,
            types: inputs
                .iter()
                .map(|p| (p.name.clone(), p.ty.kind.clone()))
                .collect(),
            ..Self::default()
        };
        for (index, member) in e.members.iter().enumerate() {
            if let syntax::ElementMember::Binding(binding) = member {
                result.local(index, binding)?;
            }
        }
        Ok(result)
    }
}

#[path = "control.rs"]
mod control;
