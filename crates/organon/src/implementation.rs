//! Shared evaluation for XML elements and Rust interfaces. No application types.
use std::{cell::Cell, fmt, ops::Deref, rc::Rc};
use theoria_reactive::{Computed, Reactive, ReactiveError, Value};

/// Node construction used by generated builders and interface implementations.
#[doc(hidden)]
pub trait BindingContext {
    fn computed<T: 'static>(
        &mut self,
        read: impl Fn(&mut theoria_reactive::ReadContext<'_>) -> T + 'static,
    ) -> Computed<T>;
    fn constant<T: 'static>(&mut self, value: T) -> Value<T>;
}

impl BindingContext for &Reactive {
    fn computed<T: 'static>(
        &mut self,
        read: impl Fn(&mut theoria_reactive::ReadContext<'_>) -> T + 'static,
    ) -> Computed<T> {
        self.create_computed(read)
    }
    fn constant<T: 'static>(&mut self, value: T) -> Value<T> {
        self.create_state(value).value()
    }
}

impl BindingContext for theoria_component::MountContext<'_> {
    fn computed<T: 'static>(
        &mut self,
        read: impl Fn(&mut theoria_reactive::ReadContext<'_>) -> T + 'static,
    ) -> Computed<T> {
        self.create_computed(read)
    }
    fn constant<T: 'static>(&mut self, value: T) -> Value<T> {
        self.create_state(value).value()
    }
}

/// A generated interface. Tuple storage avoids per-interface helper type names.
pub trait Interface: Sized {
    type Inputs;
    type Input;
    type Arguments;
    type Outputs;
    #[doc(hidden)]
    fn bind(self, context: &mut impl BindingContext) -> Result<Self::Outputs, ReactiveError>;
}

/// Supplies output computations; evaluation belongs to Organon.
pub trait Implementation<I: Interface> {
    fn build(&self, input: I::Input) -> I;
}

#[derive(Debug)]
pub enum BuildError {
    MissingInput(&'static str),
    Reactive(ReactiveError),
}
impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingInput(name) => write!(f, "missing input {name:?}"),
            Self::Reactive(error) => error.fmt(f),
        }
    }
}
impl std::error::Error for BuildError {}
impl From<ReactiveError> for BuildError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

/// Read capability for one output evaluation. Only actual dynamic reads count.
pub struct ReadContext<'read, 'evaluation> {
    native: &'read theoria_reactive::ReadContext<'evaluation>,
    dynamic: Cell<bool>,
}

type Evaluate<T> = dyn Fn(&ReadContext<'_, '_>) -> T;

/// A named output's computation, before it is connected to an evaluation graph.
pub struct Calculation<T>(Rc<Evaluate<T>>);

impl<T> Calculation<T> {
    /// Keep an expression in its consumer's evaluation context without a cache.
    pub fn into_inline(self) -> Source<T> {
        Source(Storage::Inline(self.0))
    }
}

/// Store a computation without evaluating it. Its interface supplies the policy.
/// Computations must depend only on immutable captures and tracked input reads.
pub fn output<T>(evaluate: impl Fn(&ReadContext<'_, '_>) -> T + 'static) -> Calculation<T> {
    Calculation(Rc::new(evaluate))
}

/// An input handle. Cloning shares its storage and never clones T.
pub struct Source<T>(Storage<T>);

enum Storage<T> {
    Fixed(Rc<T>),
    Reactive(Value<T>),
    Inline(Rc<Evaluate<T>>),
}

impl<T> Clone for Source<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Storage::Fixed(v) => Storage::Fixed(v.clone()),
            Storage::Reactive(v) => Storage::Reactive(v.clone()),
            Storage::Inline(v) => Storage::Inline(v.clone()),
        })
    }
}

impl<T: fmt::Debug> fmt::Debug for Source<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Storage::Fixed(value) => f.debug_tuple("Fixed").field(value).finish(),
            Storage::Reactive(_) => f.write_str("Reactive(..)"),
            Storage::Inline(_) => f.write_str("Inline(..)"),
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
    /// Select a nested output, retaining fixedness and inline reads.
    /// Dynamic parents select their current child's Source in each reader.
    #[doc(hidden)]
    pub fn project<U: Clone + 'static>(
        self,
        select: impl Fn(&T) -> Source<U> + 'static,
    ) -> Source<U> {
        if let Some(value) = self.as_fixed() {
            return select(value);
        }
        Source(Storage::Inline(Rc::new(move |cx| {
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
            Storage::Inline(evaluate) => ReadStorage::Inline(evaluate(cx)),
        })
    }
}

/// Borrowed fixed/native data or an owned inline result, valid for this read.
pub struct Read<'read, T>(ReadStorage<'read, T>);
enum ReadStorage<'read, T> {
    Fixed(&'read T),
    Reactive(theoria_reactive::Read<'read, T>),
    Inline(T),
}
impl<T> Deref for Read<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        match &self.0 {
            ReadStorage::Fixed(v) => v,
            ReadStorage::Reactive(v) => v,
            ReadStorage::Inline(v) => v,
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
            ReadStorage::Inline(v) => v,
        }
    }
}

/// A connected output, with one stable native handle and optional fixed value.
pub struct Bound<T> {
    source: Source<T>,
    native: Value<T>,
}

impl<T> Clone for Bound<T> {
    fn clone(&self) -> Self {
        Self {
            source: self.source.clone(),
            native: self.native.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Bound<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Bound")
            .field("source", &self.source)
            .finish_non_exhaustive()
    }
}

impl<T: Clone + 'static> Calculation<T> {
    /// Evaluate once during construction to classify actual dependencies.
    /// Dynamic ordinary outputs reuse this first Computed result. Inline outputs
    /// execute in their consumers; their native adapter remains a stable handle.
    pub fn bind(self, reactive: &Reactive, inline: bool) -> Result<Bound<T>, ReactiveError> {
        self.bind_in(&mut &*reactive, inline)
    }

    /// Bind in the same node-ownership context as the enclosing component.
    pub fn bind_in(
        self,
        context: &mut impl BindingContext,
        inline: bool,
    ) -> Result<Bound<T>, ReactiveError> {
        let dynamic = Rc::new(Cell::new(false));
        let observed = dynamic.clone();
        let evaluate = self.0.clone();
        let computed = context.computed(move |native| {
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
            let value = first.cloned();
            return Ok(Bound {
                source: Source::from(value.clone()),
                native: context.constant(value),
            });
        }
        let native = computed.value();
        let source = if inline {
            Source(Storage::Inline(self.0))
        } else {
            Source::from(native.clone())
        };
        Ok(Bound { source, native })
    }
}

impl<T: 'static> Bound<T> {
    pub fn value(&self) -> Value<T> {
        self.native.clone()
    }
    pub fn as_fixed(&self) -> Option<&T> {
        self.source.as_fixed()
    }
    /// Connect within Organon without erasing fixedness or inline behavior.
    pub fn source(&self) -> Source<T> {
        self.source.clone()
    }
}
