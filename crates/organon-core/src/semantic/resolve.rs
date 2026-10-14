use super::*;
use crate::{
    format as f,
    types::{Port, Type, ValueType},
};
use std::collections::{BTreeMap, HashSet};

pub fn resolve(file: &f::DefinitionFile) -> Result<Program, Error> {
    file.validate()?;
    if file.sources().next().is_some() {
        return Err(Error("resolve imports before semantic analysis".into()));
    }
    let mut resolver = Resolver {
        definitions: BTreeMap::new(),
        signatures: BTreeMap::new(),
        active: HashSet::new(),
        elements: vec![],
    };
    for d in file.definitions() {
        if resolver.definitions.insert(d.name().into(), d).is_some() {
            return Err(Error(format!("duplicate definition {:?}", d.name())));
        }
    }
    for d in file.definitions() {
        match d {
            f::Definition::External(v) => {
                if let Some(c) = v.children() {
                    resolver.check_type(&c.kind)?;
                }
                let inputs: Vec<_> = v.inputs().cloned().collect();
                let outputs: Vec<_> = v.outputs().map(|o| o.port.clone()).collect();
                resolver.check_ports(&inputs)?;
                resolver.check_ports(&outputs)?;
                let mut seen = HashSet::new();
                for name in v.implements() {
                    if !seen.insert(name)
                        || !matches!(
                            resolver.definitions.get(name),
                            Some(f::Definition::Interface(_))
                        )
                    {
                        return Err(Error(format!("invalid implements {name:?}")));
                    }
                }
                resolver.signatures.insert(
                    v.name.clone(),
                    Signature {
                        inline_outputs: v
                            .outputs()
                            .filter(|o| o.inline)
                            .map(|o| o.port.name.clone())
                            .collect(),
                        kind: DefinitionKind::External { tag: v.tag },
                        inputs,
                        optional_inputs: v
                            .inputs()
                            .filter(|p| v.optional(&p.name))
                            .map(|p| p.name.clone())
                            .collect(),
                        children: v.children().cloned(),
                        outputs,
                        implements: v.implements().map(str::to_owned).collect(),
                    },
                );
            }
            f::Definition::Interface(v) => {
                resolver.check_ports(&v.outputs)?;
                resolver.signatures.insert(
                    v.name.clone(),
                    Signature {
                        inline_outputs: Default::default(),
                        kind: DefinitionKind::Interface,
                        inputs: vec![],
                        optional_inputs: Default::default(),
                        children: None,
                        outputs: v.outputs.clone(),
                        implements: vec![],
                    },
                );
            }
            _ => (),
        }
    }
    for d in file.definitions() {
        if let f::Definition::External(e) = d {
            for name in e.implements() {
                let expected = &resolver.signatures[name].outputs;
                if let Some(mapping) = e.implementation(name) {
                    let mut seen = HashSet::new();
                    for p in &mapping.provides {
                        if !seen.insert(&p.name) || !expected.iter().any(|o| o.name == p.name) {
                            return Err(Error("invalid interface provision".into()));
                        }
                    }
                    if seen.len() != expected.len() {
                        return Err(Error("missing interface provision".into()));
                    }
                    // A bare implements declaration is supplied by the native implementation.
                    for output in expected {
                        let source = &mapping
                            .provides
                            .iter()
                            .find(|p| p.name == output.name)
                            .unwrap()
                            .reference;
                        let actual = e
                            .outputs()
                            .find(|o| o.port.name == *source)
                            .ok_or_else(|| Error("unknown provided output".into()))?;
                        if actual.port.ty != output.ty {
                            return Err(Error("incompatible interface output".into()));
                        }
                    }
                }
            }
        }
    }
    for d in file.definitions() {
        resolver.ensure(d.name())?;
    }
    Ok(Program {
        signatures: resolver.signatures,
        elements: resolver.elements,
    })
}
struct Resolver<'a> {
    definitions: BTreeMap<String, &'a f::Definition>,
    signatures: BTreeMap<String, Signature>,
    active: HashSet<String>,
    elements: Vec<Element>,
}
impl Resolver<'_> {
    fn check_type(&self, ty: &Type) -> Result<(), Error> {
        if self.definitions.contains_key(ty.spelling()) {
            Ok(())
        } else {
            Err(Error(format!("unknown type {:?}", ty.spelling())))
        }
    }
    fn check_ports(&self, ports: &[Port]) -> Result<(), Error> {
        let mut seen = HashSet::new();
        for p in ports {
            self.check_type(&p.ty.kind)?;
            if !seen.insert(&p.name) {
                return Err(Error(format!("duplicate port {:?}", p.name)));
            }
        }
        Ok(())
    }
    fn ensure(&mut self, name: &str) -> Result<(), Error> {
        if self.signatures.contains_key(name) {
            return Ok(());
        }
        let Some(f::Definition::Element(e)) = self.definitions.get(name).copied() else {
            return Err(Error(format!("unknown element {name:?}")));
        };
        if !self.active.insert(name.into()) {
            return Err(Error(format!("cyclic element dependency {name:?}")));
        }
        let mut scope = Scope::new(e)?;
        for input in e.inputs() {
            self.check_type(&input.kind)?;
        }
        for name in scope.declarations.keys().cloned().collect::<Vec<_>>() {
            self.binding(&name, &mut scope)?;
        }
        let mut outputs = vec![];
        let mut names = HashSet::new();
        for provided in e.provides() {
            if !names.insert(provided.name.as_str()) {
                return Err(Error("duplicate output".into()));
            }
            let ty = self.binding(&provided.reference, &mut scope)?;
            outputs.push(Output {
                name: provided.name.clone(),
                binding: provided.reference.clone(),
                ty,
            });
        }
        let mut implementations = vec![];
        let mut seen = HashSet::new();
        for imp in e.implementations() {
            self.ensure(&imp.interface)?;
            let contract = self.signatures[&imp.interface].clone();
            if !matches!(
                self.definitions.get(&imp.interface),
                Some(f::Definition::Interface(_))
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
                let ty = self.binding(&p.reference, &mut scope)?;
                if ty != expected.ty || !provided.insert(&p.name) {
                    return Err(Error(format!("invalid provide {:?}", p.name)));
                }
                outputs.push(Output {
                    name: p.name.clone(),
                    binding: p.reference.clone(),
                    ty,
                });
            }
            if outputs.len() != contract.outputs.len() {
                return Err(Error("incomplete interface implementation".into()));
            }
            implementations.push(Implementation {
                interface: imp.interface.clone(),
                outputs,
            });
        }
        if outputs.is_empty() && implementations.is_empty() {
            return Err(Error("element requires a provide or impl".into()));
        }
        self.signatures.insert(
            name.into(),
            Signature {
                inline_outputs: Default::default(),
                kind: DefinitionKind::Element,
                optional_inputs: Default::default(),
                children: None,
                inputs: e
                    .inputs()
                    .map(|i| Port {
                        name: i.name.clone(),
                        ty: ValueType {
                            kind: i.kind.clone(),
                            fallible: false,
                        },
                    })
                    .collect(),
                outputs: outputs
                    .iter()
                    .map(|o| Port {
                        name: o.name.clone(),
                        ty: o.ty.clone(),
                    })
                    .collect(),
                implements: implementations
                    .iter()
                    .map(|i| i.interface.clone())
                    .collect(),
            },
        );
        self.elements.push(Element {
            name: name.into(),
            inputs: e.inputs().cloned().collect(),
            bindings: scope.bindings,
            outputs,
            implementations,
        });
        self.active.remove(name);
        Ok(())
    }
    fn binding(&mut self, name: &str, scope: &mut Scope) -> Result<ValueType, Error> {
        if let Some(ty) = scope.types.get(name) {
            return Ok(ty.clone());
        }
        let (origin, decl) = scope
            .declarations
            .get(name)
            .cloned()
            .ok_or_else(|| Error(format!("unknown binding {name:?}")))?;
        if !scope.active.insert(name.into()) {
            return Err(Error(format!("cyclic binding {name:?}")));
        }
        let (value, inline) = match decl {
            f::Declaration::Let(v) => (self.expression(&v.value, scope)?, v.inline),
            f::Declaration::Bind(v) => {
                // Anonymous construction is materialized once, shared by every projection.
                let parent = if let f::ValueExpr::Reference(n) = &v.value {
                    n.clone()
                } else {
                    if let Some(n) = scope.parents.get(&origin) {
                        n.clone()
                    } else {
                        let expression = self.expression(&v.value, scope)?;
                        let mut n = format!("__organon_bind_{origin}");
                        while scope.declarations.contains_key(&n) || scope.types.contains_key(&n) {
                            n.push('_');
                        }
                        scope.types.insert(n.clone(), expression.ty.clone());
                        scope.parents.insert(origin, n.clone());
                        scope.bindings.push(Binding {
                            name: n.clone(),
                            origin,
                            inline: false,
                            value: expression,
                        });
                        n
                    }
                };
                let ty = self.binding(&parent, scope)?;
                self.ensure(ty.kind.spelling())?;
                let output = v
                    .outputs
                    .iter()
                    .find(|(_, n)| n.as_str() == name)
                    .unwrap()
                    .0;
                let mut result = self.signatures[ty.kind.spelling()]
                    .outputs
                    .iter()
                    .find(|o| &o.name == output)
                    .ok_or_else(|| Error(format!("unknown output {output:?}")))?
                    .ty
                    .clone();
                let output_type = result.clone();
                result.fallible |= ty.fallible;
                (
                    Expression {
                        ty: result,
                        kind: ExpressionKind::Projection {
                            parent,
                            output: output.clone(),
                            output_type,
                            parent_fallible: ty.fallible,
                        },
                    },
                    false,
                )
            }
            _ => unreachable!(),
        };
        let ty = value.ty.clone();
        scope.types.insert(name.into(), ty.clone());
        scope.active.remove(name);
        scope.bindings.push(Binding {
            name: name.into(),
            origin,
            inline,
            value,
        });
        Ok(ty)
    }
    fn compatible(&mut self, actual: &ValueType, expected: &ValueType) -> Result<bool, Error> {
        if actual.fallible && !expected.fallible {
            return Ok(false);
        }
        if actual.kind == expected.kind {
            return Ok(true);
        }
        if self.definitions.contains_key(actual.kind.spelling()) {
            self.ensure(actual.kind.spelling())?;
        }
        Ok(self
            .signatures
            .get(actual.kind.spelling())
            .is_some_and(|s| s.implements.iter().any(|i| i == expected.kind.spelling())))
    }
    fn expression(&mut self, value: &f::ValueExpr, scope: &mut Scope) -> Result<Expression, Error> {
        let f::ValueExpr::Instance(i) = value else {
            let f::ValueExpr::Reference(name) = value else {
                unreachable!()
            };
            return Ok(Expression {
                ty: self.binding(name, scope)?,
                kind: ExpressionKind::Reference(name.clone()),
            });
        };
        self.ensure(&i.element)?;
        let signature = self.signatures[&i.element].clone();
        if !signature.constructible() {
            return Err(Error(format!(
                "{} cannot be constructed as a tag",
                i.element
            )));
        }
        let inputs = signature.inputs;
        let result = ValueType {
            kind: Type::parse(&i.element)?,
            fallible: false,
        };
        if i.inputs
            .keys()
            .any(|n| !inputs.iter().any(|p| &p.name == n))
        {
            return Err(Error(format!("{}: unknown input", i.element)));
        }
        let mut arguments = BTreeMap::new();
        for port in inputs {
            let text = match i.inputs.get(&port.name) {
                Some(text) => text,
                None if signature.optional_inputs.contains(&port.name) => {
                    arguments.insert(port.name, Argument::Missing { expected: port.ty });
                    continue;
                }
                None => return Err(Error(format!("missing input {:?}", port.name))),
            };
            if matches!(
                self.definitions.get(port.ty.kind.spelling()),
                Some(f::Definition::External(_)) | Some(f::Definition::Interface(_))
            ) {
                arguments.insert(
                    port.name,
                    Argument::Attribute {
                        text: text.clone(),
                        expected: port.ty,
                    },
                );
                continue;
            }
            let arg = {
                let name: crate::names::BindingName = text.parse()?;
                let actual = self.binding(name.as_str(), scope)?;
                if !self.compatible(&actual, &port.ty)? {
                    return Err(Error(format!(
                        "expected {}, got {}",
                        port.ty.spelling(),
                        actual.spelling()
                    )));
                }
                Argument::Reference {
                    name: name.as_str().into(),
                    expected: port.ty.clone(),
                }
            };
            arguments.insert(port.name, arg);
        }
        if let Some(c) = &signature.children
            && (i.children.len() < c.minimum || c.maximum.is_some_and(|n| i.children.len() > n))
        {
            return Err(Error(format!("{}: invalid child count", i.element)));
        }
        let mut children = vec![];
        for child in &i.children {
            let contract = signature
                .children
                .as_ref()
                .ok_or_else(|| Error(format!("{} does not accept children", i.element)))?;
            let child = self.expression(child, scope)?;
            if !self.compatible(
                &child.ty,
                &ValueType {
                    kind: contract.kind.clone(),
                    fallible: child.ty.fallible,
                },
            )? {
                return Err(Error("incompatible child type".into()));
            }

            children.push(child);
        }
        Ok(Expression {
            ty: result,
            kind: ExpressionKind::Construct {
                name: i.element.clone(),
                arguments,
                children,
            },
        })
    }
}
struct Scope {
    declarations: BTreeMap<String, (usize, f::Declaration)>,
    types: BTreeMap<String, ValueType>,
    bindings: Vec<Binding>,
    active: HashSet<String>,
    parents: BTreeMap<usize, String>,
}
impl Scope {
    fn new(e: &f::ElementDefinition) -> Result<Self, Error> {
        let mut result = Self {
            declarations: BTreeMap::new(),
            types: BTreeMap::new(),
            bindings: vec![],
            active: HashSet::new(),
            parents: BTreeMap::new(),
        };
        let mut names = HashSet::new();
        for (index, d) in e.declarations.iter().enumerate() {
            let keys = match d {
                f::Declaration::Input(i) => {
                    result.types.insert(
                        i.name.clone(),
                        ValueType {
                            kind: i.kind.clone(),
                            fallible: false,
                        },
                    );
                    vec![&i.name]
                }
                f::Declaration::Let(v) => vec![&v.name],
                f::Declaration::Bind(v) => v.outputs.values().collect(),
                _ => vec![],
            };
            for name in keys {
                if !names.insert(name) {
                    return Err(Error(format!("duplicate binding {name:?}")));
                }
                if !matches!(d, f::Declaration::Input(_)) {
                    result.declarations.insert(name.clone(), (index, d.clone()));
                }
            }
        }
        Ok(result)
    }
}
