//! Named Rust value slots in the compiler construction plan.
/// A name in the generated Rust value module, including Result/Option/children aliases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Type(pub String);

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Port {
    pub name: String,
    pub ty: Type,
}
