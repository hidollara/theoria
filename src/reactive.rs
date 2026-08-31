//! Runtime-owned reactive values for higher-level Theoria components.
//!
//! A [`Value<T>`] is an opaque read-only value. It can be fixed, backed by a
//! [`State<T>`], or derived by a computed node without exposing that
//! distinction to consumers. Dynamic nodes and Effects belong to a
//! [`ReactiveScope`] and are released together when that scope is dropped.
//!
//! State changes enter through an [`EventContext`]. Computed values derive
//! values through tracked reads, and Effects form the terminal boundary from
//! the reactive graph to renderers and other external systems. The Runtime
//! evaluates Effects deterministically after an event turn.

mod context;
mod effect;
mod runtime;
mod value;

use std::{cell::RefCell, fmt, rc::Rc};

pub use context::{EffectContext, EventContext, ReadContext};
use effect::{EffectId, EffectNode};
use runtime::{ComputedNode, ErasedNode, NodeId, RuntimeInner, StateNode};
use value::DynamicHandle;
pub use value::{ComputeResult, Snapshot, State, Value};

/// Owns the storage and evaluation context for a group of reactive scopes.
///
/// The current implementation is deliberately single-threaded. Its storage is
/// private so it can later change representation without changing State or
/// Value.
#[derive(Clone)]
pub struct ReactiveRuntime {
    inner: Rc<RuntimeInner>,
}

impl ReactiveRuntime {
    pub fn new() -> Self {
        Self {
            inner: Rc::new(RuntimeInner::new()),
        }
    }

    /// Creates an ownership scope for State, computed nodes, and Effects.
    pub fn scope(&self) -> ReactiveScope {
        ReactiveScope {
            runtime: Rc::clone(&self.inner),
            nodes: RefCell::new(Vec::new()),
            effects: RefCell::new(Vec::new()),
        }
    }

    /// Runs one imperative event turn, then evaluates changed Effects.
    ///
    /// Nested event turns and synchronous event turns started by an Effect are
    /// rejected, preserving one stable State snapshot per Effect pass.
    pub fn run_event_turn<R>(&self, handler: impl FnOnce(&mut EventContext<'_>) -> R) -> R {
        self.inner.run_event_turn(handler)
    }

    /// Evaluates Effects whose tracked dependencies have changed.
    ///
    /// This is primarily needed once after graph construction. Event turns
    /// flush Effects automatically.
    pub fn flush_effects(&self) {
        self.inner.flush_effects();
    }
}

impl Default for ReactiveRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for ReactiveRuntime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReactiveRuntime")
            .finish_non_exhaustive()
    }
}

/// Owns a set of reactive nodes with a common lifetime.
///
/// Dropping the scope removes all its Effects before removing its value nodes.
/// Handles are generational, so stale Values cannot alias later arena entries.
pub struct ReactiveScope {
    runtime: Rc<RuntimeInner>,
    nodes: RefCell<Vec<NodeId>>,
    effects: RefCell<Vec<EffectId>>,
}

impl ReactiveScope {
    /// Creates State owned by this scope.
    pub fn state<T: 'static>(&self, value: T) -> State<T> {
        State::from_handle(self.insert(StateNode::new(value)))
    }

    /// Creates a lazily evaluated Value owned by this scope.
    ///
    /// The dependencies from the latest successful evaluation are the Values
    /// actually read through [`ReadContext`].
    pub fn computed<T: 'static>(
        &self,
        builder: impl Fn(&mut ReadContext<'_>) -> T + 'static,
    ) -> Value<T> {
        self.computed_incremental(move |context, _previous| {
            ComputeResult::Replace(builder(context))
        })
    }

    /// Creates a computed Value whose builder can reuse its previous result.
    ///
    /// `previous` is an optimization input. The semantic result must remain a
    /// function of Values read from `context`; historical state belongs in an
    /// event-side [`EventContext::update`].
    pub fn computed_incremental<T: 'static>(
        &self,
        builder: impl Fn(&mut ReadContext<'_>, Option<&T>) -> ComputeResult<T> + 'static,
    ) -> Value<T> {
        Value::reactive(self.insert(ComputedNode::new(builder)))
    }

    /// Registers a terminal reaction owned by this scope.
    ///
    /// Reads through [`EffectContext`] are tracked dynamically. The Effect is
    /// first run by [`ReactiveRuntime::flush_effects`] or at the end of the
    /// next event turn, then rerun only when a dependency revision changes.
    pub fn effect(&self, effect: impl FnMut(&mut EffectContext<'_>) + 'static) {
        let id = self.runtime.insert_effect(Rc::new(EffectNode::new(effect)));
        self.effects.borrow_mut().push(id);
    }

    fn insert<T: 'static>(&self, node: impl ErasedNode + 'static) -> DynamicHandle<T> {
        let id = self.runtime.insert(Rc::new(node));
        self.nodes.borrow_mut().push(id);
        DynamicHandle::new(Rc::downgrade(&self.runtime), id)
    }
}

impl fmt::Debug for ReactiveScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReactiveScope")
            .finish_non_exhaustive()
    }
}

impl Drop for ReactiveScope {
    fn drop(&mut self) {
        for id in self.effects.get_mut().drain(..).rev() {
            self.runtime.remove_effect(id);
        }
        for id in self.nodes.get_mut().drain(..).rev() {
            self.runtime.remove(id);
        }
    }
}

#[cfg(test)]
mod tests;
