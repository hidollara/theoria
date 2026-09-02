use std::{any::Any, fmt, marker::PhantomData, rc::Rc};

use crate::{
    error::ReactiveError,
    evaluation::Evaluation,
    node::{ComputedNode, ErasedComputation, EvaluationReader, Provider},
    operation::OperationGate,
    read::{ReadContext, Snapshot},
    value::Value,
};

/// An owning handle to a lazily evaluated reactive result.
pub struct Computed<T> {
    value: Value<T>,
}

impl<T> Computed<T> {
    pub(crate) fn new(
        gate: &Rc<OperationGate>,
        builder: impl Fn(&mut ReadContext<'_>) -> T + 'static,
    ) -> Self
    where
        T: 'static,
    {
        let provider: Rc<dyn Provider> = ComputedNode::new(
            Rc::clone(gate),
            Box::new(ComputedEvaluator::<T, _>::new(builder)),
        );
        Self {
            value: Value::new(provider),
        }
    }

    pub(crate) fn provider(&self) -> Rc<dyn Provider> {
        self.value.provider()
    }

    /// Returns a read-only handle without Computed-specific capabilities.
    pub fn value(&self) -> Value<T> {
        self.value.clone()
    }

    /// Resolves the current result without registering a dependency.
    pub fn snapshot(&self) -> Result<Snapshot<T>, ReactiveError>
    where
        T: 'static,
    {
        let evaluation = Evaluation::new(self.value.provider_ref().gate())?;
        Ok(Snapshot::new(evaluation.resolve(self.provider())))
    }
}

impl<T> Clone for Computed<T> {
    fn clone(&self) -> Self {
        Self {
            value: self.value.clone(),
        }
    }
}

impl<T> AsRef<Value<T>> for Computed<T> {
    fn as_ref(&self) -> &Value<T> {
        &self.value
    }
}

impl<T> fmt::Debug for Computed<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Computed").finish_non_exhaustive()
    }
}

struct ComputedEvaluator<T, F> {
    builder: F,
    marker: PhantomData<fn() -> T>,
}

impl<T, F> ComputedEvaluator<T, F> {
    fn new(builder: F) -> Self {
        Self {
            builder,
            marker: PhantomData,
        }
    }
}

impl<T, F> ErasedComputation for ComputedEvaluator<T, F>
where
    T: 'static,
    F: Fn(&mut ReadContext<'_>) -> T + 'static,
{
    fn evaluate(&self, reader: &mut dyn EvaluationReader) -> Rc<dyn Any> {
        Rc::new((self.builder)(&mut ReadContext::new(reader)))
    }
}
