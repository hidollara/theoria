use std::{cell::Cell, rc::Rc};

use crate::error::ReactiveError;

/// Shared gate preventing overlapping top-level operations.
pub(crate) struct OperationGate {
    occupied: Cell<bool>,
}

impl OperationGate {
    pub(crate) fn new() -> Rc<Self> {
        Rc::new(Self {
            occupied: Cell::new(false),
        })
    }

    pub(crate) fn acquire(&self) -> Result<OperationGuard<'_>, ReactiveError> {
        if self.occupied.replace(true) {
            return Err(ReactiveError::AccessConflict);
        }
        Ok(OperationGuard { gate: self })
    }
}

pub(crate) struct OperationGuard<'operation> {
    gate: &'operation OperationGate,
}

impl OperationGuard<'_> {
    pub(crate) fn is_for(&self, gate: &Rc<OperationGate>) -> bool {
        std::ptr::eq(self.gate, gate.as_ref())
    }
}

impl Drop for OperationGuard<'_> {
    fn drop(&mut self) {
        debug_assert!(self.gate.occupied.get());
        self.gate.occupied.set(false);
    }
}
