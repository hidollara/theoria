//! Lexical evaluation of conditional calculations. A frame lives for one read.
pub use crate::source::DuringRead;
use crate::{Calculation, ReadContext, Source, attribute};
pub use attribute::Value;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fmt,
    rc::Rc,
};

/// A declared Organon failure. Identity is nominal even if two declarations share a Rust type.
#[derive(Clone)]
pub struct Failure {
    pub kind: String,
    pub value: Value,
}
impl Failure {
    /// Raise a value under its Organon declaration, including across import aliases.
    pub fn new<M: crate::Type>(payload: M::Value) -> Self {
        Self {
            kind: M::ID.to_owned(),
            value: Value::new(payload),
        }
    }
    pub fn downcast<M: crate::Type>(&self) -> Option<M::Value> {
        (self.kind == M::ID)
            .then(|| self.value.get::<M::Value>().ok())
            .flatten()
    }
    /// Admit the exact declared reason first, then the first matching interface.
    pub fn accept(
        self,
        allowed: &[impl AsRef<str>],
        registry: &attribute::Registry,
    ) -> Outcome<Self> {
        if allowed.iter().any(|kind| kind.as_ref() == self.kind) {
            return Ok(self);
        }
        for kind in allowed {
            let kind = kind.as_ref();
            if let Some(value) = self.as_contract(kind, registry)? {
                return Ok(Self {
                    kind: kind.into(),
                    value,
                });
            }
        }
        Err(Error::Failure(self))
    }
    /// Use the declared Organon identity: different providers can share the Rust payload type.
    pub fn as_contract(
        &self,
        target: &str,
        registry: &attribute::Registry,
    ) -> Outcome<Option<Value>> {
        if self.kind == target {
            return Ok(Some(self.value.clone()));
        }
        let Some(entry) = registry
            .types
            .values()
            .find(|entry| entry.contract.name == self.kind)
        else {
            return Ok(None);
        };
        if !entry.contract.interfaces.contains(&target) {
            return Ok(None);
        }
        Ok(Some((entry.provide)(&self.value, target)?))
    }
}
impl fmt::Debug for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("Failure").field(&self.kind).finish()
    }
}
#[derive(Clone, Debug)]
pub enum Error {
    Failure(Failure),
    Invalid(String),
    /// An argument failed in the caller before it satisfied the predicate's input contract.
    #[doc(hidden)]
    Caller(Box<Error>),
}
impl From<String> for Error {
    fn from(s: String) -> Self {
        Self::Invalid(s)
    }
}
impl From<&str> for Error {
    fn from(s: &str) -> Self {
        Self::Invalid(s.into())
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(s) => f.write_str(s),
            Self::Caller(e) => fmt::Display::fmt(e, f),
            Self::Failure(e) => write!(f, "unhandled {}", e.kind),
        }
    }
}
pub type Outcome<T> = Result<T, Error>;
pub type Expr<T = Value> = Rc<dyn Fn(&Frame<'_>, &ReadContext<'_, '_>) -> Outcome<T>>;
pub type Locals = BTreeMap<String, Expr>;
pub struct Frame<'a> {
    locals: &'a Locals,
    outer: Option<&'a Frame<'a>>,
    arguments_from: Option<&'a Frame<'a>>,
    values: RefCell<BTreeMap<String, (Outcome<Value>, bool)>>,
    active: RefCell<BTreeSet<String>>,
}
impl<'a> Frame<'a> {
    pub fn new(locals: &'a Locals, outer: Option<&'a Frame<'a>>) -> Self {
        Self {
            locals,
            outer,
            arguments_from: None,
            values: Default::default(),
            active: Default::default(),
        }
    }
    pub fn read(&self, name: &str, cx: &ReadContext<'_, '_>) -> Outcome<Value> {
        if let Some((v, dynamic)) = self.values.borrow().get(name) {
            cx.track(*dynamic);
            return v.clone();
        }
        let Some(expr) = self.locals.get(name) else {
            return self
                .outer
                .ok_or_else(|| format!("unknown binding {name:?}"))?
                .read(name, cx);
        };
        if !self.active.borrow_mut().insert(name.into()) {
            return Err(format!("cyclic binding {name:?}").into());
        }
        let (value, dynamic) = cx.observe(|cx| expr(self.arguments_from.unwrap_or(self), cx));
        self.active.borrow_mut().remove(name);
        self.values
            .borrow_mut()
            .insert(name.into(), (value.clone(), dynamic));
        value
    }
}
/// Root bindings share immutable results across independently requested outputs.
/// A branch-local frame has different arguments on each visit and only caches for that visit.
pub fn roots(mut locals: Locals) -> Locals {
    for expr in locals.values_mut() {
        let original = expr.clone();
        let fixed = std::cell::OnceCell::<Outcome<Value>>::new();
        *expr = Rc::new(move |frame, cx| {
            if let Some(v) = fixed.get() {
                return v.clone();
            }
            let (v, dynamic) = cx.observe(|cx| original(frame, cx));
            if !dynamic {
                let _ = fixed.set(v.clone());
            }
            v
        });
    }
    locals
}
pub fn reference(name: &str) -> Expr {
    let name = name.to_owned();
    Rc::new(move |frame, cx| frame.read(&name, cx))
}
pub fn map<T: 'static, U: 'static>(
    item: Expr<T>,
    f: impl Fn(T) -> Outcome<U> + 'static,
) -> Expr<U> {
    Rc::new(move |frame, cx| f(item(frame, cx)?))
}
pub fn input<T: Clone + 'static>(source: Source<T>) -> Expr {
    Rc::new(move |_, cx| Ok(Value::new(source.read(cx).into_cloned())))
}
pub fn throwing(kind: &str, value: Expr) -> Expr {
    let kind = kind.to_owned();
    Rc::new(move |f, cx| {
        Err(Error::Failure(Failure {
            kind: kind.clone(),
            value: value(f, cx)?,
        }))
    })
}
pub struct Branch {
    pub locals: Locals,
    pub guard: Option<Expr<bool>>,
    pub result: Expr,
}
impl Branch {
    fn visit(
        &self,
        parent: &Frame<'_>,
        cx: &ReadContext<'_, '_>,
        binding: Option<(&str, Value)>,
    ) -> Outcome<Option<Value>> {
        let f = Frame::new(&self.locals, Some(parent));
        if let Some((n, v)) = binding {
            f.values.borrow_mut().insert(n.into(), (Ok(v), false));
        }
        if let Some(test) = &self.guard
            && !test(&f, cx)?
        {
            return Ok(None);
        }
        (self.result)(&f, cx).map(Some)
    }
}
pub fn select(branches: Vec<Branch>) -> Expr {
    Rc::new(move |f, cx| {
        for b in &branches {
            if let Some(v) = b.visit(f, cx, None)? {
                return Ok(v);
            }
        }
        Err("no matching Organon case".into())
    })
}
pub enum Handler {
    Catch {
        matches: Box<dyn Fn(&Failure) -> Outcome<Option<Value>>>,
        name: Option<String>,
        branch: Branch,
    },
    Unreachable {
        result: String,
        /// Return the matching failure's Organon name for the diagnostic.
        match_reason: Box<dyn Fn(&str) -> Option<String>>,
    },
}

/// Use the compiled contract table directly; only a failed try needs a lookup.
pub fn unreachable(
    result: &str,
    target: Option<&'static str>,
    contracts: &'static [(&str, attribute::Contract<'static>)],
) -> Handler {
    Handler::Unreachable {
        result: result.into(),
        match_reason: Box::new(move |kind| {
            if let Some((name, contract)) = contracts.iter().find(|(_, c)| c.name == kind) {
                return target
                    .is_none_or(|t| kind == t || contract.interfaces.contains(&t))
                    .then(|| (*name).into());
            }
            (target.is_none() || target == Some(kind)).then(|| kind.into())
        }),
    }
}
pub fn attempt(body: Branch, handlers: Vec<Handler>) -> Expr {
    Rc::new(move |f, cx| {
        let failure = match body.visit(f, cx, None) {
            Ok(Some(v)) => return Ok(v),
            Ok(None) => return Err("invalid guarded try body".into()),
            Err(Error::Failure(e)) => e,
            Err(e) => return Err(e),
        };
        for (index, handler) in handlers.iter().enumerate() {
            match handler {
                Handler::Unreachable {
                    result,
                    match_reason,
                } => {
                    if let Some(reason) = match_reason(&failure.kind) {
                        return Err(format!(
                            "Organon try result {result:?} raised {reason}; matched unreachable (handler {})",
                            index + 1
                        ).into());
                    }
                }
                Handler::Catch {
                    matches,
                    name,
                    branch,
                } => {
                    if let Some(value) = matches(&failure)?
                        && let Some(result) =
                            branch.visit(f, cx, name.as_deref().map(|n| (n, value)))?
                    {
                        return Ok(result);
                    }
                }
            }
        }
        Err(Error::Failure(failure))
    })
}
pub fn all(items: Vec<Expr<bool>>) -> Expr<bool> {
    Rc::new(move |f, c| {
        for p in &items {
            if !p(f, c)? {
                return Ok(false);
            }
        }
        Ok(true)
    })
}
pub fn any(items: Vec<Expr<bool>>) -> Expr<bool> {
    Rc::new(move |f, c| {
        for p in &items {
            if p(f, c)? {
                return Ok(true);
            }
        }
        Ok(false)
    })
}
pub fn not(item: Expr<bool>) -> Expr<bool> {
    Rc::new(move |f, c| Ok(!item(f, c)?))
}
/// Evaluate only when read. A rejected reason bypasses handlers inside this invocation.
pub fn predicate_argument(
    arg: Expr,
    accept: impl Fn(Failure) -> Outcome<Failure> + 'static,
) -> Expr {
    Rc::new(move |f, cx| match arg(f, cx) {
        Err(Error::Failure(reason)) => match accept(reason) {
            Ok(reason) => Err(Error::Failure(reason)),
            Err(e) => Err(Error::Caller(Box::new(e))),
        },
        Err(e @ Error::Caller(_)) => Err(Error::Caller(Box::new(e))),
        other => other,
    })
}
pub fn predicate(inputs: Locals, locals: Locals, test: Expr<bool>) -> Expr<bool> {
    Rc::new(move |caller, cx| {
        let mut parameters = Frame::new(&inputs, None);
        parameters.arguments_from = Some(caller);
        test(&Frame::new(&locals, Some(&parameters)), cx).map_err(|e| match e {
            Error::Caller(caller) => *caller,
            other => other,
        })
    })
}
/// Connect a parsed attribute to lexical reads. Parse once when the graph is built.
pub fn parsed<T: Clone + 'static>(
    scope: attribute::Scope,
    attribute: attribute::Attribute<T>,
) -> Expr<T> {
    parsed_with(scope, attribute, Ok)
}
/// An input-derived attribute can be wired directly to the new Element. Constructing
/// that Element then stays fixed even when its outputs read a changing parameter.
pub fn parameter<T: Clone + 'static>(
    scope: attribute::Scope,
    attribute: attribute::Attribute<T>,
    inputs: &BTreeMap<String, Source<Value>>,
) -> Expr<Source<T>> {
    if attribute
        .dependencies
        .iter()
        .all(|n| inputs.contains_key(n))
    {
        for name in &attribute.dependencies {
            scope.set(name, inputs[name].clone());
        }
        let source = attribute.source;
        Rc::new(move |_, _| Ok(source.clone()))
    } else {
        map(parsed(scope, attribute), |v| Ok(Source::from(v)))
    }
}
pub fn parsed_with<T: Clone + 'static>(
    scope: attribute::Scope,
    attribute: attribute::Attribute<T>,
    unpack: fn(Value) -> Outcome<Value>,
) -> Expr<T> {
    Rc::new(move |f, cx| {
        let values = attribute
            .dependencies
            .iter()
            .map(|name| unpack(f.read(name, cx)?))
            .collect::<Outcome<Vec<_>>>()?;
        for (name, v) in attribute.dependencies.iter().zip(values) {
            scope.set(name, Source::from(v));
        }
        let result = attribute.source.read(cx).into_cloned();
        for name in &attribute.dependencies {
            scope.clear(name);
        }
        Ok(result)
    })
}
pub fn output<T: Clone + 'static>(
    locals: Rc<Locals>,
    name: &str,
    convert: impl Fn(Outcome<Value>) -> Outcome<T> + 'static,
    context: &mut impl crate::BindingContext,
) -> Result<Source<T>, crate::BuildError> {
    let name = name.to_owned();
    let initial = Rc::new(std::cell::OnceCell::new());
    let observed = initial.clone();
    let checked = Calculation::new(move |cx| {
        let value = convert(Frame::new(&locals, None).read(&name, cx));
        observed.get_or_init(|| value.as_ref().err().map(ToString::to_string));
        value
    })
    .bind_in(context)?;
    if let Some(Some(error)) = initial.get() {
        return Err(error.clone().into());
    }
    Ok(checked.map(|v| v.clone().unwrap_or_else(|e| panic!("{e}"))))
}
