use crate::lexer::Symbol;

use super::{
    name::QualifiedName,
    parser::{
        Expected, Parsable, Parser,
        grammar::{rule, separated_until, surrounded_by, symbol},
        node::{AstNode, AstNodeParser},
    },
};

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

impl Parsable for Type {
    fn parser() -> impl Parser<Output = Self> {
        let tail = symbol(Symbol::Pipe)
            .then(postfix())
            .then(
                symbol(Symbol::Pipe)
                    .then(postfix())
                    .map(|(_, ty)| ty)
                    .repeated(),
            )
            .map(|((_, first), rest)| std::iter::once(first).chain(rest).collect::<Vec<_>>());

        postfix()
            .then(tail.optional())
            .map(|(first, rest)| match rest {
                Some(rest) => Type::Union(std::iter::once(first).chain(rest).collect()),
                None => first.value,
            })
            .labelled(Expected::Type)
    }
}

fn postfix() -> impl Parser<Output = AstNode<Type>> {
    AstNodeParser(atom()).fold(symbol(Symbol::Asterisk), |inner, _| {
        Type::Array(Box::new(inner))
    })
}

fn atom() -> impl Parser<Output = Type> {
    let group = surrounded_by(
        rule::<AstNode<Type>>(),
        Symbol::OpenParen,
        Symbol::CloseParen,
    )
    .map(|inner| Type::Group(Box::new(inner)));

    let parameters = surrounded_by(
        separated_until(
            rule::<AstNode<Type>>(),
            Symbol::Comma,
            Symbol::CloseBracket,
            true,
        ),
        Symbol::OpenBracket,
        Symbol::CloseBracket,
    );
    let result = surrounded_by(
        rule::<AstNode<Type>>(),
        Symbol::OpenParen,
        Symbol::CloseParen,
    );
    let function = parameters
        .then(result)
        .map(|(parameters, result)| Type::Function {
            parameters,
            result: Box::new(result),
        });

    let arguments = surrounded_by(
        separated_until(
            rule::<AstNode<Type>>(),
            Symbol::Comma,
            Symbol::CloseAngle,
            false,
        ),
        Symbol::OpenAngle,
        Symbol::CloseAngle,
    );
    let named = rule::<AstNode<QualifiedName>>()
        .then(arguments.optional())
        .map(|(name, arguments)| Type::Named {
            name,
            arguments: arguments.unwrap_or_default(),
        });

    group.or(function).or(named).labelled(Expected::Type)
}
