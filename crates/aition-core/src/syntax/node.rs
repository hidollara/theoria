use crate::input::Cursor;

/// A syntax value with source information when it came from a parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstNode<T> {
    pub value: T,
    pub source: Option<NodeSource>,
}

/// Source positions and trivia recorded for one parsed node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSource {
    /// Start of the syntax, excluding leading trivia.
    pub start: Cursor,
    /// Position just after the syntax, excluding trailing trivia.
    pub end: Cursor,
    /// Trivia owned by this node, excluding trivia owned by its children.
    /// The root also owns surrounding trivia outside its start and end.
    pub trivia: Vec<Trivia>,
}

impl<T> AstNode<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            source: None,
        }
    }
}

impl<T> From<(T, NodeSource)> for AstNode<T> {
    fn from((value, source): (T, NodeSource)) -> Self {
        Self {
            value,
            source: Some(source),
        }
    }
}

impl<T> From<T> for AstNode<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Whitespace or a comment owned by a syntax node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trivia {
    pub kind: TriviaKind,
    /// Start of the whitespace or comment, including the comment delimiter.
    pub start: Cursor,
    /// Position just after the trivia. A comment's line ending is separate whitespace.
    pub end: Cursor,
}

/// Trivia text owned by the parse result, independent of the input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TriviaKind {
    Whitespace(Box<str>),
    /// The body after `//`, preserving spaces and excluding the line ending.
    LineComment(Box<str>),
}
