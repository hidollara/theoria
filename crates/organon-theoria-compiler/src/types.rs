//! Named Rust value slots in the compiler construction plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub kind: Type,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    Element(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub ty: ValueType,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueType {
    pub kind: Type,
    /// Result<T, &'static str>, independent from reactive evaluation policy.
    pub fallible: bool,
}
