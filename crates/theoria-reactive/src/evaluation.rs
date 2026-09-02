use std::{any::Any, rc::Rc};

use crate::{
    error::ReactiveError,
    node::{ComputedNode, EvaluationReader, Provider, ProviderAccess},
    operation::{OperationGate, OperationGuard},
};

/// One top-level evaluation holding exclusive access to its Reactive.
pub(crate) struct Evaluation<'operation> {
    operation: OperationGuard<'operation>,
}

impl<'operation> Evaluation<'operation> {
    pub(crate) fn new(gate: &'operation Rc<OperationGate>) -> Result<Self, ReactiveError> {
        Ok(Self {
            operation: gate.acquire()?,
        })
    }

    pub(crate) fn resolve<T: 'static>(&self, provider: Rc<dyn Provider>) -> Rc<T> {
        debug_assert!(self.operation.is_for(provider.gate()));
        self.resolve_erased(provider)
            .downcast::<T>()
            .unwrap_or_else(|_| panic!("reactive result was read through an incompatible type"))
    }

    pub(crate) fn run(&self, node: &Rc<crate::node::EffectNode>) {
        debug_assert!(self.operation.is_for(node.gate()));
        let mut reader = TrackedReader::new(self);
        node.invoke(&mut reader);
        node.commit(reader.take_providers());
    }

    fn resolve_erased(&self, provider: Rc<dyn Provider>) -> Rc<dyn Any> {
        assert!(
            self.operation.is_for(provider.gate()),
            "reactive input belongs to another Reactive"
        );
        match provider.access() {
            ProviderAccess::Borrowed(_) => {
                panic!("reactive State does not retain immutable snapshots")
            }
            ProviderAccess::Shared(value) => value,
            ProviderAccess::Computed(computed) => self.resolve_computed(computed),
        }
    }

    fn resolve_computed(&self, computed: &ComputedNode) -> Rc<dyn Any> {
        if let Some(value) = computed.cached() {
            return value;
        }

        let mut guard = ComputedEvaluationGuard::enter(computed, self);
        let value = computed.evaluate(guard.reader());
        computed.commit(Rc::clone(&value), guard.take_providers());
        value
    }
}

struct ComputedEvaluationGuard<'evaluation> {
    computed: &'evaluation ComputedNode,
    reader: TrackedReader<'evaluation>,
}

impl<'evaluation> ComputedEvaluationGuard<'evaluation> {
    fn enter(
        computed: &'evaluation ComputedNode,
        evaluation: &'evaluation Evaluation<'evaluation>,
    ) -> Self {
        assert!(
            computed.enter(),
            "reactive dependency cycle detected while evaluating Computed"
        );
        Self {
            computed,
            reader: TrackedReader::new(evaluation),
        }
    }

    fn reader(&mut self) -> &mut dyn EvaluationReader {
        &mut self.reader
    }

    fn take_providers(&mut self) -> Vec<Rc<dyn Provider>> {
        self.reader.take_providers()
    }
}

impl Drop for ComputedEvaluationGuard<'_> {
    fn drop(&mut self) {
        self.computed.leave();
    }
}

struct TrackedReader<'evaluation> {
    evaluation: &'evaluation Evaluation<'evaluation>,
    providers: Vec<Rc<dyn Provider>>,
}

impl<'evaluation> TrackedReader<'evaluation> {
    fn new(evaluation: &'evaluation Evaluation<'evaluation>) -> Self {
        Self {
            evaluation,
            providers: Vec::new(),
        }
    }

    fn take_providers(&mut self) -> Vec<Rc<dyn Provider>> {
        std::mem::take(&mut self.providers)
    }
}

impl EvaluationReader for TrackedReader<'_> {
    fn track(&mut self, provider: Rc<dyn Provider>) {
        assert!(
            self.evaluation.operation.is_for(provider.gate()),
            "reactive input belongs to another Reactive"
        );
        if !self
            .providers
            .iter()
            .any(|candidate| Rc::ptr_eq(candidate, &provider))
        {
            self.providers.push(provider);
        }
    }

    fn resolve_erased(&mut self, provider: Rc<dyn Provider>) -> Rc<dyn Any> {
        let resolved = self.evaluation.resolve_erased(Rc::clone(&provider));
        self.track(provider);
        resolved
    }
}
