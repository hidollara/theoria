use std::{cell::RefCell, fmt, rc::Rc};

use crate::{
    error::ReactiveError,
    evaluation::Evaluation,
    node::{EffectNode, ErasedEffect, EvaluationReader},
    operation::OperationGate,
    read::ReadContext,
};

/// An owning terminal operation over tracked reactive inputs.
///
/// Running an Effect is explicit and unconditional. Dependencies from the
/// latest successful run are retained only so a scheduler can ask whether the
/// Effect has become stale; they do not determine when it runs.
pub struct Effect {
    node: Rc<EffectNode>,
}

impl Effect {
    fn from_node(node: Rc<EffectNode>) -> Self {
        Self { node }
    }

    pub(crate) fn new(
        gate: &Rc<OperationGate>,
        effect: impl FnMut(&mut ReadContext<'_>) + 'static,
    ) -> Self {
        Self::from_node(EffectNode::new(
            Rc::clone(gate),
            Box::new(EffectCallback::new(effect)),
        ))
    }

    /// Runs the Effect once and replaces its tracked dependencies on success.
    pub fn run(&self) -> Result<(), ReactiveError> {
        let evaluation = Evaluation::new(self.node.gate())?;
        evaluation.run(&self.node);
        Ok(())
    }

    /// Reports whether a dependency changed since the last successful run.
    ///
    /// A newly created Effect is stale until its first successful run.
    pub fn is_stale(&self) -> bool {
        self.node.is_stale()
    }
}

impl Clone for Effect {
    fn clone(&self) -> Self {
        Self::from_node(Rc::clone(&self.node))
    }
}

impl PartialEq for Effect {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.node, &other.node)
    }
}

impl Eq for Effect {}

impl fmt::Debug for Effect {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Effect").finish_non_exhaustive()
    }
}

struct EffectCallback<F> {
    callback: RefCell<F>,
}

impl<F> EffectCallback<F> {
    fn new(callback: F) -> Self {
        Self {
            callback: RefCell::new(callback),
        }
    }
}

impl<F> ErasedEffect for EffectCallback<F>
where
    F: FnMut(&mut ReadContext<'_>) + 'static,
{
    fn invoke(&self, reader: &mut dyn EvaluationReader) {
        let mut callback = self
            .callback
            .try_borrow_mut()
            .unwrap_or_else(|_| panic!("reactive Effect was entered recursively"));
        callback(&mut ReadContext::new(reader));
    }
}
