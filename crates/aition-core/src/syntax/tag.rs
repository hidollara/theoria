use super::{
    body::Body,
    literal::Literal,
    name::{Identifier, QualifiedName},
    parser::node::AstNode,
};

/// `<Name arguments/>` or `<Name arguments>children</Name>`.
///
/// This records tag syntax without deciding which declaration the name refers
/// to, whether it supports tag calls, or which parameters receive the arguments
/// and children.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tag {
    /// `Panel` or `panels.Panel`. A paired closing tag must repeat this name;
    /// parsing checks the correspondence rather than storing a second name.
    pub name: AstNode<QualifiedName>,
    /// Named arguments in source order. Repeated names are retained so that
    /// later validation can report them instead of silently replacing a value.
    pub arguments: Vec<AstNode<TagArgument>>,
    /// `None` represents `/>`; `Some` represents a paired closing tag, including
    /// `Some(vec![])` for `<Name></Name>`. This preserves spelling without
    /// assigning different argument semantics to the two empty forms.
    /// Children retain their source order and structure.
    pub children: Option<Vec<TagChild>>,
}

/// `name = Literal` or `name = Body` in a tag.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagArgument {
    pub name: AstNode<Identifier>,
    pub value: TagArgumentValue,
}

/// The two forms allowed after `=` in a tag's named argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagArgumentValue {
    /// `"100"` or `"100"@Number`.
    Literal(AstNode<Literal>),
    /// `(expression)` with optional following helper definitions.
    /// A name, call, or nested tag must be placed inside this body.
    Body(AstNode<Body>),
}

/// A nested tag or a parenthesized body between opening and closing tags.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TagChild {
    /// `<Panel width="100"/>`.
    Tag(AstNode<Tag>),
    /// `(expression)` with optional following helper definitions.
    /// Literals and other expressions must be placed inside this body.
    Body(AstNode<Body>),
}
