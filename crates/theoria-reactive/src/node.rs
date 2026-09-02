use std::{
    any::Any,
    cell::{Cell, Ref, RefCell},
    rc::{Rc, Weak},
};

use crate::operation::OperationGate;

pub(crate) enum ProviderAccess<'provider> {
    Borrowed(Ref<'provider, dyn Any>),
    Shared(Rc<dyn Any>),
    Computed(&'provider ComputedNode),
}

pub(crate) trait Provider: Any {
    fn gate(&self) -> &Rc<OperationGate>;

    fn consumers(&self) -> &ConsumerSet;

    fn access(&self) -> ProviderAccess<'_>;
}

impl dyn Provider {
    pub(crate) fn invalidate_consumers(&self) {
        let mut pending = self.live_consumers();
        while let Some(consumer) = pending.pop() {
            match consumer {
                Consumer::Computed(computed) => {
                    if computed.invalidate() {
                        pending.extend(computed.consumers.live());
                    }
                }
                Consumer::Effect(effect) => {
                    effect.mark_dirty();
                }
            }
        }
    }

    fn add_consumer(&self, consumer: ConsumerRef) {
        self.consumers().add(consumer);
    }

    fn remove_consumer(&self, consumer: &ConsumerRef) {
        self.consumers().remove(consumer);
    }

    fn live_consumers(&self) -> Vec<Consumer> {
        self.consumers().live()
    }

    #[cfg(test)]
    pub(crate) fn consumer_count(&self) -> usize {
        self.consumers().len()
    }
}

pub(crate) struct ConsumerSet {
    entries: RefCell<Vec<ConsumerRef>>,
}

impl ConsumerSet {
    pub(crate) fn new() -> Self {
        Self {
            entries: RefCell::new(Vec::new()),
        }
    }

    fn add(&self, consumer: ConsumerRef) {
        let mut consumers = self.entries.borrow_mut();
        if !consumers
            .iter()
            .any(|candidate| candidate.ptr_eq(&consumer))
        {
            consumers.push(consumer);
        }
    }

    fn remove(&self, consumer: &ConsumerRef) {
        self.entries
            .borrow_mut()
            .retain(|candidate| !candidate.ptr_eq(consumer));
    }

    fn live(&self) -> Vec<Consumer> {
        let mut consumers = self.entries.borrow_mut();
        let mut live = Vec::with_capacity(consumers.len());
        consumers.retain(|consumer| {
            let Some(consumer) = consumer.upgrade() else {
                return false;
            };
            live.push(consumer);
            true
        });
        live
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.borrow().len()
    }
}

pub(crate) struct ComputedNode {
    gate: Rc<OperationGate>,
    consumers: ConsumerSet,
    computation: Box<dyn ErasedComputation>,
    cached: RefCell<Option<Rc<dyn Any>>>,
    dependencies: DependencyEdges,
    evaluating: Cell<bool>,
}

impl ComputedNode {
    pub(crate) fn new(
        gate: Rc<OperationGate>,
        computation: Box<dyn ErasedComputation>,
    ) -> Rc<Self> {
        Rc::new_cyclic(|consumer| Self {
            gate,
            consumers: ConsumerSet::new(),
            computation,
            cached: RefCell::new(None),
            dependencies: DependencyEdges::new(ConsumerRef::Computed(consumer.clone())),
            evaluating: Cell::new(false),
        })
    }

    pub(crate) fn evaluate(&self, reader: &mut dyn EvaluationReader) -> Rc<dyn Any> {
        self.computation.evaluate(reader)
    }

    pub(crate) fn cached(&self) -> Option<Rc<dyn Any>> {
        self.cached.borrow().clone()
    }

    pub(crate) fn commit(&self, value: Rc<dyn Any>, providers: Vec<Rc<dyn Provider>>) {
        self.dependencies.replace(providers);
        self.cached.replace(Some(value));
    }

    fn invalidate(&self) -> bool {
        let previous = self.cached.borrow_mut().take();
        previous.is_some()
    }

    pub(crate) fn enter(&self) -> bool {
        !self.evaluating.replace(true)
    }

    pub(crate) fn leave(&self) {
        self.evaluating.set(false);
    }
}

impl Provider for ComputedNode {
    fn gate(&self) -> &Rc<OperationGate> {
        &self.gate
    }

    fn consumers(&self) -> &ConsumerSet {
        &self.consumers
    }

    fn access(&self) -> ProviderAccess<'_> {
        ProviderAccess::Computed(self)
    }
}

pub(crate) struct EffectNode {
    effect: Box<dyn ErasedEffect>,
    gate: Rc<OperationGate>,
    dependencies: DependencyEdges,
    dirty: Cell<bool>,
}

impl EffectNode {
    pub(crate) fn new(gate: Rc<OperationGate>, effect: Box<dyn ErasedEffect>) -> Rc<Self> {
        Rc::new_cyclic(|consumer| Self {
            effect,
            gate,
            dependencies: DependencyEdges::new(ConsumerRef::Effect(consumer.clone())),
            dirty: Cell::new(true),
        })
    }

    pub(crate) fn invoke(&self, reader: &mut dyn EvaluationReader) {
        self.effect.invoke(reader);
    }

    pub(crate) fn is_stale(&self) -> bool {
        self.dirty.get()
    }

    pub(crate) fn mark_dirty(&self) {
        self.dirty.set(true);
    }

    pub(crate) fn commit(&self, providers: Vec<Rc<dyn Provider>>) {
        self.dependencies.replace(providers);
        self.dirty.set(false);
    }

    pub(crate) fn gate(&self) -> &Rc<OperationGate> {
        &self.gate
    }
}

struct DependencyEdges {
    consumer: ConsumerRef,
    providers: RefCell<Vec<Rc<dyn Provider>>>,
}

impl DependencyEdges {
    fn new(consumer: ConsumerRef) -> Self {
        Self {
            consumer,
            providers: RefCell::new(Vec::new()),
        }
    }

    fn replace(&self, providers: Vec<Rc<dyn Provider>>) {
        let previous = std::mem::take(&mut *self.providers.borrow_mut());
        for provider in &previous {
            if !providers
                .iter()
                .any(|candidate| Rc::ptr_eq(candidate, provider))
            {
                provider.remove_consumer(&self.consumer);
            }
        }
        for provider in &providers {
            if !previous
                .iter()
                .any(|candidate| Rc::ptr_eq(candidate, provider))
            {
                provider.add_consumer(self.consumer.clone());
            }
        }
        *self.providers.borrow_mut() = providers;
    }
}

impl Drop for DependencyEdges {
    fn drop(&mut self) {
        for provider in self.providers.get_mut().drain(..) {
            provider.remove_consumer(&self.consumer);
        }
    }
}

#[derive(Clone)]
enum ConsumerRef {
    Computed(Weak<ComputedNode>),
    Effect(Weak<EffectNode>),
}

impl ConsumerRef {
    fn upgrade(&self) -> Option<Consumer> {
        match self {
            Self::Computed(value) => value.upgrade().map(Consumer::Computed),
            Self::Effect(effect) => effect.upgrade().map(Consumer::Effect),
        }
    }

    fn ptr_eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Computed(left), Self::Computed(right)) => Weak::ptr_eq(left, right),
            (Self::Effect(left), Self::Effect(right)) => Weak::ptr_eq(left, right),
            _ => false,
        }
    }
}

enum Consumer {
    Computed(Rc<ComputedNode>),
    Effect(Rc<EffectNode>),
}

pub(crate) trait EvaluationReader {
    fn track(&mut self, provider: Rc<dyn Provider>);

    fn resolve_erased(&mut self, provider: Rc<dyn Provider>) -> Rc<dyn Any>;
}

pub(crate) trait ErasedComputation {
    fn evaluate(&self, reader: &mut dyn EvaluationReader) -> Rc<dyn Any>;
}

pub(crate) trait ErasedEffect {
    fn invoke(&self, reader: &mut dyn EvaluationReader);
}

#[cfg(test)]
mod tests {
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        rc::Rc,
    };

    use crate::reactive::Reactive;

    #[test]
    fn repeated_reads_register_one_consumer() {
        let reactive = Reactive::new();
        let source = reactive.create_state(1);
        let source_value = source.value();
        let output =
            reactive.create_computed(move |cx| *cx.read(&source_value) + *cx.read(&source_value));

        assert_eq!(*output.snapshot().expect("evaluation is available"), 2);
        assert_eq!(source.provider().consumer_count(), 1);
    }

    #[test]
    fn dropping_a_computed_detaches_it_from_its_providers() {
        let reactive = Reactive::new();
        let source = reactive.create_state(1);
        let source_value = source.value();

        let output = reactive.create_computed(move |cx| *cx.read(&source_value) + 1);
        assert_eq!(*output.snapshot().expect("evaluation is available"), 2);
        assert_eq!(source.provider().consumer_count(), 1);
        drop(output);

        assert_eq!(source.provider().consumer_count(), 0);
    }

    #[test]
    fn dropping_an_effect_detaches_it_from_its_providers() {
        let reactive = Reactive::new();
        let state = reactive.create_state(1);
        let value = state.value();

        let effect = reactive.create_effect(move |cx| {
            let _ = cx.read(&value);
        });
        assert!(effect.run().is_ok());
        assert_eq!(state.provider().consumer_count(), 1);
        drop(effect);

        assert_eq!(state.provider().consumer_count(), 0);
    }

    #[test]
    fn a_panicking_computed_keeps_the_last_successful_dependencies() {
        let reactive = Reactive::new();
        let left = reactive.create_state(1);
        let right = reactive.create_state(10);
        let left_value = left.value();
        let right_value = right.value();
        let use_right = Rc::new(Cell::new(false));
        let builder_use_right = Rc::clone(&use_right);
        let should_panic = Rc::new(Cell::new(false));
        let builder_should_panic = Rc::clone(&should_panic);
        let computed = reactive.create_computed(move |cx| {
            let input = if builder_use_right.get() {
                *cx.read(&right_value)
            } else {
                *cx.read(&left_value)
            };
            assert!(!builder_should_panic.replace(false), "requested panic");
            input
        });

        assert_eq!(*computed.snapshot().expect("evaluation is available"), 1);
        assert_eq!(left.provider().consumer_count(), 1);
        assert_eq!(right.provider().consumer_count(), 0);

        use_right.set(true);
        should_panic.set(true);
        reactive
            .mutate(|cx| cx.set(&left, 2))
            .expect("mutation is available");
        assert!(
            catch_unwind(AssertUnwindSafe(|| computed
                .snapshot()
                .expect("evaluation is available")))
            .is_err()
        );
        assert_eq!(left.provider().consumer_count(), 1);
        assert_eq!(right.provider().consumer_count(), 0);

        assert_eq!(*computed.snapshot().expect("evaluation is available"), 10);
        assert_eq!(left.provider().consumer_count(), 0);
        assert_eq!(right.provider().consumer_count(), 1);
    }

    #[test]
    fn a_panicking_effect_keeps_the_last_successful_dependencies() {
        let reactive = Reactive::new();
        let left = reactive.create_state(1);
        let right = reactive.create_state(2);
        let left_value = left.value();
        let right_value = right.value();
        let use_right = Rc::new(Cell::new(false));
        let effect_use_right = Rc::clone(&use_right);
        let should_panic = Rc::new(Cell::new(false));
        let effect_should_panic = Rc::clone(&should_panic);
        let effect = reactive.create_effect(move |cx| {
            if effect_use_right.get() {
                let _ = cx.read(&right_value);
            } else {
                let _ = cx.read(&left_value);
            }
            assert!(!effect_should_panic.replace(false), "requested panic");
        });

        assert!(effect.run().is_ok());
        assert_eq!(left.provider().consumer_count(), 1);
        assert_eq!(right.provider().consumer_count(), 0);

        use_right.set(true);
        should_panic.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| effect.run().is_ok())).is_err());
        assert!(!effect.is_stale());
        assert_eq!(left.provider().consumer_count(), 1);
        assert_eq!(right.provider().consumer_count(), 0);

        reactive
            .mutate(|cx| cx.set(&left, 2))
            .expect("mutation is available");
        assert!(effect.is_stale());
        should_panic.set(true);
        assert!(catch_unwind(AssertUnwindSafe(|| effect.run().is_ok())).is_err());
        assert!(effect.is_stale());
        assert_eq!(left.provider().consumer_count(), 1);
        assert_eq!(right.provider().consumer_count(), 0);

        assert!(effect.run().is_ok());
        assert_eq!(left.provider().consumer_count(), 0);
        assert_eq!(right.provider().consumer_count(), 1);
    }
}
