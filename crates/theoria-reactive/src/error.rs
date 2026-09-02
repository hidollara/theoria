use std::{error::Error, fmt};

/// A failure to enter or combine reactive operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactiveError {
    /// Evaluation and mutation, or two mutation batches, would overlap.
    AccessConflict,
}

impl fmt::Display for ReactiveError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::AccessConflict => "reactive evaluation and mutation access conflict",
        })
    }
}

impl Error for ReactiveError {}
