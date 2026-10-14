//! Typed value sources and dependency-aware calculation binding.
//!
//! This layer chooses Fixed, Reactive, or Deferred storage. It does not interpret XML or IR.

use std::{cell::Cell, fmt, ops::Deref, rc::Rc};
use theoria_reactive::{Reactive, ReactiveError, Value};

/// Node construction used by generated builders and external element implementations.
#[doc(hidden)]
pub trait BindingContext {
    fn reactive(&self) -> Reactive;
    /// Construction nested in an active read must reuse that evaluation.
    fn during_read(&self) -> bool {
        false
    }
}
#[doc(hidden)]
pub struct DuringRead(pub Reactive);
impl BindingContext for DuringRead {
    fn reactive(&self) -> Reactive {
        self.0.clone()
    }
    fn during_read(&self) -> bool {
        true
    }
}

impl BindingContext for &Reactive {
    fn reactive(&self) -> Reactive {
        (*self).clone()
    }
}

impl BindingContext for theoria_component::MountContext<'_> {
    fn reactive(&self) -> Reactive {
        theoria_component::MountContext::reactive(self).clone()
    }
}

/// Read capability for one output evaluation. Only actual dynamic reads count.
pub struct ReadContext<'read, 'evaluation> {
    native: &'read theoria_reactive::ReadContext<'evaluation>,
    dynamic: Cell<bool>,
}

impl<'read, 'evaluation> ReadContext<'read, 'evaluation> {
    pub(crate) fn observe<T>(&self, read: impl FnOnce(&Self) -> T) -> (T, bool) {
        let child = Self::new(self.native);
        let value = read(&child);
        let dynamic = child.dynamic.get();
        self.track(dynamic);
        (value, dynamic)
    }
    pub(crate) fn track(&self, dynamic: bool) {
        if dynamic {
            self.dynamic.set(true)
        }
    }
    /// Read Organon sources inside a caller-owned Theoria computation.
    pub fn new(native: &'read theoria_reactive::ReadContext<'evaluation>) -> Self {
        Self {
            native,
            dynamic: Cell::new(false),
        }
    }
}

type Evaluate<T> = dyn Fn(&ReadContext<'_, '_>) -> T;

/// A named output's computation, before it is connected to an evaluation graph.
pub struct Calculation<T>(Computation<T>);
enum Computation<T> {
    Evaluate(Rc<Evaluate<T>>),
    Forward(Source<T>),
}

impl<T> Calculation<T> {
    /// Store a computation without evaluating it. Binding classifies actual dependencies.
    /// Computations must depend only on immutable captures and tracked input reads.
    pub fn new(evaluate: impl Fn(&ReadContext<'_, '_>) -> T + 'static) -> Self {
        Self(Computation::Evaluate(Rc::new(evaluate)))
    }
    /// Forward an existing source without inserting another computation.
    pub fn forward(source: Source<T>) -> Self {
        Self(Computation::Forward(source))
    }

    /// Defer an adapter or unresolved reference to its consumer's evaluation.
    /// Attribute parsers use this before bindings exist; generated code also uses it
    /// during nested reads. The enclosing bound calculation tracks these reads.
    pub fn into_deferred(self) -> Source<T> {
        match self.0 {
            Computation::Evaluate(evaluate) => Source(Storage::Deferred(evaluate)),
            Computation::Forward(source) => source,
        }
    }
}

/// A value source used for inputs and outputs. Cloning shares storage, never T.
pub struct Source<T>(Storage<T>);

enum Storage<T> {
    Fixed(Rc<T>),
    Reactive(Value<T>),
    Deferred(Rc<Evaluate<T>>),
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Storage::Fixed(v) => Storage::Fixed(v.clone()),
            Storage::Reactive(v) => Storage::Reactive(v.clone()),
            Storage::Deferred(v) => Storage::Deferred(v.clone()),
        })
    }
}

impl<T: fmt::Debug> fmt::Debug for Source<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Storage::Fixed(value) => f.debug_tuple("Fixed").field(value).finish(),
            Storage::Reactive(_) => f.write_str("Reactive(..)"),
            Storage::Deferred(_) => f.write_str("Deferred(..)"),
        }
    }
}

impl<T> From<T> for Source<T> {
    fn from(value: T) -> Self {
        Self(Storage::Fixed(Rc::new(value)))
    }
}

impl<T> From<Value<T>> for Source<T> {
    fn from(value: Value<T>) -> Self {
        Self(Storage::Reactive(value))
    }
}

impl From<&str> for Source<String> {
    fn from(value: &str) -> Self {
        value.to_owned().into()
    }
}

impl<T: 'static> Source<T> {
    /// Transform a source without erasing fixedness or adding a cache boundary.
    pub fn map<U: 'static>(self, map: impl Fn(&T) -> U + 'static) -> Source<U> {
        if let Some(value) = self.as_fixed() {
            return Source::from(map(value));
        }
        Calculation::new(move |cx| map(&self.read(cx))).into_deferred()
    }
    /// Select a nested output, retaining fixedness and deferred reads.
    /// Dynamic parents select their current child's Source in each reader.
    #[doc(hidden)]
    pub fn project<U: Clone + 'static>(
        self,
        select: impl Fn(&T) -> Source<U> + 'static,
    ) -> Source<U> {
        if let Some(value) = self.as_fixed() {
            return select(value);
        }
        Source(Storage::Deferred(Rc::new(move |cx| {
            let value = self.read(cx);
            let output = select(&value);
            output.read(cx).into_cloned()
        })))
    }

    pub fn as_fixed(&self) -> Option<&T> {
        match &self.0 {
            Storage::Fixed(value) => Some(value),
            _ => None,
        }
    }

    pub fn read<'read>(&'read self, cx: &'read ReadContext<'_, '_>) -> Read<'read, T> {
        Read(match &self.0 {
            Storage::Fixed(value) => ReadStorage::Fixed(value),
            Storage::Reactive(value) => {
                cx.dynamic.set(true);
                ReadStorage::Reactive(cx.native.read(value))
            }
            Storage::Deferred(evaluate) => ReadStorage::Owned(evaluate(cx)),
        })
    }
}

/// Borrowed fixed/native data or an owned deferred result, valid for this read.
pub struct Read<'read, T>(ReadStorage<'read, T>);
enum ReadStorage<'read, T> {
    Fixed(&'read T),
    Reactive(theoria_reactive::Read<'read, T>),
    Owned(T),
}
impl<T> Deref for Read<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.0 {
            ReadStorage::Fixed(v) => v,
            ReadStorage::Reactive(v) => v,
            ReadStorage::Owned(v) => v,
        }
    }
}
impl<T: Clone> Read<'_, T> {
    pub fn cloned(&self) -> T {
        self.deref().clone()
    }
    pub fn into_cloned(self) -> T {
        match self.0 {
            ReadStorage::Fixed(v) => v.clone(),
            ReadStorage::Reactive(v) => v.cloned(),
            ReadStorage::Owned(v) => v,
        }
    }
}

impl<T: Clone + 'static> Calculation<T> {
    /// Evaluate once during construction to classify actual dependencies.
    /// Dynamic outputs share the initial Computed result and subsequent updates.
    pub fn bind(self, reactive: &Reactive) -> Result<Source<T>, ReactiveError> {
        self.bind_in(&mut &*reactive)
    }

    /// Bind using the enclosing component's reactive graph.
    pub fn bind_in(self, context: &mut impl BindingContext) -> Result<Source<T>, ReactiveError> {
        if context.during_read() {
            return Ok(self.into_deferred());
        }
        let evaluate = match self.0 {
            Computation::Forward(source) => return Ok(source),
            Computation::Evaluate(evaluate) => evaluate,
        };
        let dynamic = Rc::new(Cell::new(false));
        let observed = dynamic.clone();
        let reactive = context.reactive();
        let computed = reactive.create_computed(move |native| {
            let cx = ReadContext {
                native,
                dynamic: Cell::new(false),
            };
            let value = evaluate(&cx);
            observed.set(cx.dynamic.get());
            value
        });
        let first = computed.snapshot()?;
        if !dynamic.get() {
            return Ok(Source::from(first.cloned()));
        }
        Ok(Source::from(computed.value()))
    }
}
