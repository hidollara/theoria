//! Values used by the interpreted preview; generated elements use their own private storage.
use std::{fmt, ops::Deref, rc::Rc};
use theoria_reactive::ReadContext;

/// A fixed value, a host reactive value, or a derived value with optional caching.
/// Reading borrows the value or holds an inline result without cloning T.
pub struct Value<T>(Storage<T>);

enum Storage<T> {
    Fixed(T),
    Reactive(theoria_reactive::Value<T>),
    Inline(Rc<dyn Fn(&ReadContext<'_>) -> T>),
}

impl<T> Value<T> {
    pub fn fixed(value: T) -> Self {
        Self(Storage::Fixed(value))
    }

    pub fn as_fixed(&self) -> Option<&T> {
        match &self.0 {
            Storage::Fixed(value) => Some(value),
            _ => None,
        }
    }

    pub fn is_fixed(&self) -> bool {
        self.as_fixed().is_some()
    }

    /// Access an existing native handle without adding another Computed.
    pub fn as_reactive(&self) -> Option<&theoria_reactive::Value<T>> {
        match &self.0 {
            Storage::Reactive(value) => Some(value),
            _ => None,
        }
    }

    /// Hold an expression that runs in each reader's context without caching.
    pub fn inline(evaluate: impl Fn(&ReadContext<'_>) -> T + 'static) -> Self {
        Self(Storage::Inline(Rc::new(evaluate)))
    }

    /// Accept a host State or Computed through its ordinary read-only Value.
    /// This adapter does not create another Computed.
    pub fn reactive(value: theoria_reactive::Value<T>) -> Self {
        Self(Storage::Reactive(value))
    }
}

impl<T: 'static> Value<T> {
    /// Read in the caller's evaluation, recording only the actual dependencies.
    /// The view cannot be retained as a Computed result beyond that evaluation:
    ///
    /// ```compile_fail
    /// let reactive = theoria_reactive::Reactive::new();
    /// let value = organon_app::evaluation::Source::inline(|_| String::from("temporary"));
    /// let computed = reactive.create_computed(move |cx| value.read(cx));
    /// ```
    pub fn read<'read>(&'read self, cx: &'read ReadContext<'_>) -> Read<'read, T> {
        Read {
            storage: match &self.0 {
                Storage::Fixed(value) => ReadStorage::Fixed(value),
                Storage::Reactive(value) => ReadStorage::Reactive(cx.read(value)),
                Storage::Inline(read) => ReadStorage::Inline(read(cx)),
            },
        }
    }
}

impl<T: Clone> Clone for Value<T> {
    fn clone(&self) -> Self {
        Self(match &self.0 {
            Storage::Fixed(value) => Storage::Fixed(value.clone()),
            Storage::Reactive(value) => Storage::Reactive(value.clone()),
            Storage::Inline(value) => Storage::Inline(value.clone()),
        })
    }
}

impl<T> From<T> for Value<T> {
    fn from(value: T) -> Self {
        Self::fixed(value)
    }
}

impl<T: fmt::Debug> fmt::Debug for Value<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.0 {
            Storage::Fixed(value) => f.debug_tuple("Fixed").field(value).finish(),
            Storage::Reactive(_) => f.write_str("Reactive(..)"),
            Storage::Inline(_) => f.write_str("Inline(..)"),
        }
    }
}

/// A temporary view tied to the value and its reader's evaluation.
///
/// Fixed values are borrowed, reactive values retain Theoria's read guard,
/// and inline results live in this view until it is dropped. Reading an inline
/// expression again evaluates it again; this view is not a persistent cache.
pub struct Read<'read, T> {
    storage: ReadStorage<'read, T>,
}

enum ReadStorage<'read, T> {
    Fixed(&'read T),
    Reactive(theoria_reactive::Read<'read, T>),
    Inline(T),
}

impl<T> Read<'_, T> {
    /// Consume this read, moving an inline result or cloning a borrowed value.
    pub fn into_cloned(self) -> T
    where
        T: Clone,
    {
        match self.storage {
            ReadStorage::Fixed(value) => value.clone(),
            ReadStorage::Reactive(value) => value.cloned(),
            ReadStorage::Inline(value) => value,
        }
    }

    /// Clone the observed value when ownership beyond this read is needed.
    pub fn cloned(&self) -> T
    where
        T: Clone,
    {
        self.deref().clone()
    }
}

impl<T> Deref for Read<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        match &self.storage {
            ReadStorage::Fixed(value) => value,
            ReadStorage::Reactive(value) => value,
            ReadStorage::Inline(value) => value,
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Read<'_, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.deref().fmt(f)
    }
}
