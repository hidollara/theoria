use std::{fmt, rc::Rc};

use super::{
    runtime::{Dependency, NodeId, RuntimeInner, StateNode},
    value::{Snapshot, State, Value, ValueInner},
};

/// Records dependencies during one computed evaluation.
pub struct ReadContext<'evaluation> {
    runtime: &'evaluation RuntimeInner,
    dependencies: Vec<Dependency>,
}

impl<'evaluation> ReadContext<'evaluation> {
    pub(super) fn new(runtime: &'evaluation RuntimeInner) -> Self {
        Self {
            runtime,
            dependencies: Vec::new(),
        }
    }

    /// Reads a Value and records it as a dependency of this computation.
    pub fn read<T: 'static>(&mut self, value: &Value<T>) -> Snapshot<T> {
        read_tracked(
            self.runtime,
            &mut self.dependencies,
            value,
            "a computed value cannot depend on a different ReactiveRuntime",
        )
    }

    pub(super) fn into_dependencies(self) -> Vec<Dependency> {
        self.dependencies
    }
}

/// Capabilities available while handling one imperative event turn.
pub struct EventContext<'turn> {
    runtime: &'turn RuntimeInner,
}

impl<'turn> EventContext<'turn> {
    pub(super) fn new(runtime: &'turn RuntimeInner) -> Self {
        Self { runtime }
    }

    /// Reads the current snapshot without creating a reactive dependency.
    pub fn read_current<T: 'static>(&self, value: &Value<T>) -> Snapshot<T> {
        read_untracked(
            self.runtime,
            value,
            "an event cannot read a value from a different ReactiveRuntime",
        )
    }

    /// Replaces State during this event turn.
    pub fn set<T: 'static>(&mut self, state: &State<T>, value: T) {
        self.with_state(state, |node| node.set(value));
    }

    /// Replaces State using its current value during this event turn.
    pub fn update<T: 'static>(&mut self, state: &State<T>, update: impl FnOnce(&T) -> T) {
        self.with_state(state, |node| node.update(update));
    }

    fn with_state<T: 'static, R>(
        &self,
        state: &State<T>,
        operation: impl FnOnce(&StateNode<T>) -> R,
    ) -> R {
        let runtime = state.handle.runtime();
        assert!(
            std::ptr::eq(runtime.as_ref(), self.runtime),
            "an event cannot mutate State from a different ReactiveRuntime"
        );
        runtime.with_state(state.handle.id, operation)
    }
}

impl fmt::Debug for EventContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EventContext")
            .finish_non_exhaustive()
    }
}

/// Tracked read capabilities available while a terminal Effect runs.
pub struct EffectContext<'evaluation> {
    runtime: &'evaluation RuntimeInner,
    dependencies: Vec<Dependency>,
}

impl<'evaluation> EffectContext<'evaluation> {
    pub(super) fn new(runtime: &'evaluation RuntimeInner) -> Self {
        Self {
            runtime,
            dependencies: Vec::new(),
        }
    }

    /// Reads a Value and records it as a dependency of this Effect.
    pub fn read<T: 'static>(&mut self, value: &Value<T>) -> Snapshot<T> {
        read_tracked(
            self.runtime,
            &mut self.dependencies,
            value,
            "an Effect cannot depend on a different ReactiveRuntime",
        )
    }

    pub(super) fn into_dependencies(self) -> Vec<Dependency> {
        self.dependencies
    }
}

impl fmt::Debug for EffectContext<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectContext")
            .finish_non_exhaustive()
    }
}

fn read_tracked<T: 'static>(
    expected_runtime: &RuntimeInner,
    dependencies: &mut Vec<Dependency>,
    value: &Value<T>,
    cross_runtime_message: &str,
) -> Snapshot<T> {
    match &value.inner {
        ValueInner::Constant(value) => Snapshot::new(Rc::clone(value)),
        ValueInner::Reactive(handle) => {
            let runtime = handle.runtime();
            assert!(
                std::ptr::eq(runtime.as_ref(), expected_runtime),
                "{cross_runtime_message}"
            );
            let result = runtime.resolve(handle.id);
            record_dependency(dependencies, handle.id, runtime.revision(handle.id));
            Snapshot::new(result)
        }
    }
}

fn read_untracked<T: 'static>(
    expected_runtime: &RuntimeInner,
    value: &Value<T>,
    cross_runtime_message: &str,
) -> Snapshot<T> {
    match &value.inner {
        ValueInner::Constant(value) => Snapshot::new(Rc::clone(value)),
        ValueInner::Reactive(handle) => {
            let runtime = handle.runtime();
            assert!(
                std::ptr::eq(runtime.as_ref(), expected_runtime),
                "{cross_runtime_message}"
            );
            Snapshot::new(runtime.resolve(handle.id))
        }
    }
}

fn record_dependency(dependencies: &mut Vec<Dependency>, id: NodeId, revision: u64) {
    if let Some(existing) = dependencies
        .iter_mut()
        .find(|dependency| dependency.id() == id)
    {
        existing.set_revision(revision);
        return;
    }
    dependencies.push(Dependency::new(id, revision));
}
