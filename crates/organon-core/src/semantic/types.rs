//! Resolved contract data. No authored syntax objects are retained here.

/// A symbolic reference to a definition in a Program's contract table.
/// Names here identify resolved definitions, not source-level spellings or aliases.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Type(String);

impl Type {
    /// Reference a definition by its Program key. This does not parse an identifier
    /// or check table membership; the resolver checks authored references first.
    pub fn named(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    pub fn name(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug)]
pub struct Port<T = TypeContract> {
    pub name: String,
    pub ty: T,
}

/// Resolved success type and permitted failures. Failure order preserves
/// precedence when more than one interface can accept the same failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeContract {
    pub kind: Type,
    pub errors: Vec<Type>,
}

impl TypeContract {
    pub fn fallible(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn describe(&self) -> String {
        if self.fallible() {
            format!(
                "{} (errors: {})",
                self.kind.name(),
                self.errors
                    .iter()
                    .map(Type::name)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        } else {
            self.kind.name().into()
        }
    }
}

/// Resolved child type and accepted length range used during composition checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChildrenContract {
    pub kind: Type,
    pub minimum: usize,
    pub maximum: Option<usize>,
}

impl ChildrenContract {
    pub fn accepts(&self, count: usize) -> bool {
        self.accepts_range(count, Some(count))
    }

    pub(crate) fn accepts_range(&self, minimum: usize, maximum: Option<usize>) -> bool {
        minimum >= self.minimum
            && self
                .maximum
                .is_none_or(|limit| maximum.is_some_and(|n| n <= limit))
    }
}
