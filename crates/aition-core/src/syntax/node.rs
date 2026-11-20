use crate::input::Cursor;

/// A syntax value with source information when it came from a parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstNode<T> {
    pub value: T,
    pub source: Option<NodeSource>,
}

/// Source positions and trivia recorded for one parsed node.
/// The file root covers the entire input, including surrounding trivia.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSource {
    /// Position where this node starts.
    pub start: Cursor,
    /// Position just after this node ends.
    pub end: Cursor,
    /// Trivia owned by this node, excluding trivia owned by its children.
    pub trivia: Vec<Trivia>,
}

impl<T> AstNode<T> {
    pub(crate) fn with_source_range(value: T, start: Cursor, end: Cursor) -> Self {
        Self {
            value,
            source: Some(NodeSource {
                start,
                end,
                trivia: Vec::new(),
            }),
        }
    }

    pub fn new(value: T) -> Self {
        Self {
            value,
            source: None,
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
