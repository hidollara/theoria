//! Type syntax and its precedence: primary types, array suffixes, then unions.

use super::{
    name::QualifiedName,
    node::AstNode,
    parser::{ParseError, Parser, Syntax, choice, list, optional, repeated, sequence, symbol},
    token::{TokenKind, TokenStream},
};
use crate::lexer::{LexemeIterator, Symbol};

/// A type expression represented by its grammatical structure.
/// Explicit grouping, alternative order, and repetitions are retained.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Type {
    /// `Panel` or `panels.Picture<Image>`.
    Named {
        name: AstNode<QualifiedName>,
        arguments: Vec<AstNode<Type>>,
    },
    /// `T*`.
    Array(Box<AstNode<Type>>),
    /// `[T, U] (V)`; parameters contain type expressions.
    Function {
        parameters: Vec<AstNode<Type>>,
        result: Box<AstNode<Type>>,
    },
    /// `A | B | C`. Alternatives retain their order and any repetitions.
    Union(Vec<AstNode<Type>>),
    /// `"(" Type ")"`. Each explicit pair of grouping parentheses is retained.
    Group(Box<AstNode<Type>>),
}

impl Syntax for Type {
    fn parser() -> impl Parser<Output = Self> {
        TypeParser
    }
}

struct TypeParser;

impl Parser for TypeParser {
    type Output = Type;

    fn description(&self) -> String {
        "a type".into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        // Build the recursive grammar when reading, not in Syntax::parser().
        sequence![
            PostfixTypeParser,
            repeated(sequence![symbol(Symbol::Pipe), PostfixTypeParser].map(|(_, ty)| ty)),
        ]
        .map(|(first, rest)| {
            if rest.is_empty() {
                first.value
            } else {
                Type::Union(std::iter::once(first).chain(rest).collect())
            }
        })
        .read(tokens)
    }
}

/// Primary types followed by array suffixes bind more tightly than unions.
struct PostfixTypeParser;

impl Parser for PostfixTypeParser {
    type Output = AstNode<Type>;

    fn description(&self) -> String {
        "a type".into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mark = tokens.mark()?;
        let (value, mut tokens) = primary().read(tokens)?;
        let Some(value) = value else {
            return Ok((None, tokens));
        };
        let mut node = AstNode {
            value,
            span: Some(tokens.span(&mark)),
        };
        while tokens
            .next_if(|kind| matches!(kind, TokenKind::Symbol(Symbol::Asterisk)))?
            .is_some()
        {
            node = AstNode {
                value: Type::Array(Box::new(node)),
                span: Some(tokens.span(&mark)),
            };
        }
        Ok((Some(node), tokens))
    }
}

fn primary() -> impl Parser<Output = Type> {
    let group = sequence![
        symbol(Symbol::OpenParen),
        AstNode::<Type>::parser(),
        symbol(Symbol::CloseParen),
    ]
    .map(|(_, inner, _)| Type::Group(Box::new(inner)));

    let function = sequence![
        list(
            AstNode::<Type>::parser(),
            Symbol::OpenBracket,
            Symbol::CloseBracket,
            Symbol::Comma,
            true,
        ),
        symbol(Symbol::OpenParen),
        AstNode::<Type>::parser(),
        symbol(Symbol::CloseParen),
    ]
    .map(|(parameters, _, result, _)| Type::Function {
        parameters,
        result: Box::new(result),
    });

    let named = sequence![
        AstNode::<QualifiedName>::parser(),
        optional(list(
            AstNode::<Type>::parser(),
            Symbol::OpenAngle,
            Symbol::CloseAngle,
            Symbol::Comma,
            false,
        )),
    ]
    .map(|(name, arguments)| Type::Named {
        name,
        arguments: arguments.unwrap_or_default(),
    });

    choice![group, function, named]
}
