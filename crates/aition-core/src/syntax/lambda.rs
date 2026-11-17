use super::{body::Body, parameter::Parameter, parser::node::AstNode};

/// Parameters followed by a body: `[parameters] (expression) { ... }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lambda {
    /// Parameters retain their source order. An empty list denotes a function
    /// with no parameters.
    pub parameters: Vec<AstNode<Parameter>>,
    /// The parameters are available to both the result expression and its
    /// helper bindings.
    pub body: AstNode<Body>,
}
