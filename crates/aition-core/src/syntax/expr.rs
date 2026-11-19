use super::{
    name::Identifier,
    node::AstNode,
    parser::{ParseError, Parser, Syntax, choice, list, optional, sequence, string, symbol},
    token::TokenStream,
    types::Type,
};
use crate::lexer::{LexemeIterator, Symbol};

/// An expression as written in source, before name resolution or type checking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Expr {
    /// `"100"` or `"100"@Number`.
    ///
    /// `text` contains the string contents after string escape decoding, before
    /// interpretation by a type's parser. `ty` records only an explicit `@Type`;
    /// a type required by the surrounding expression is not inserted here.
    Literal {
        text: Box<str>,
        ty: Option<AstNode<Type>>,
    },
    /// `width`.
    Name(AstNode<Identifier>),
    /// `panels.Panel` or `makePanel(width).drawing`.
    ///
    /// Syntax does not distinguish module lookup from access to a value's member.
    Member {
        base: Box<AstNode<Expr>>,
        name: AstNode<Identifier>,
    },
    /// `makePanel(width)` or `makePainter(style)(panel)`.
    /// The callee is an expression, not necessarily a name.
    Call {
        callee: Box<AstNode<Expr>>,
        arguments: Vec<AstNode<Expr>>,
    },
    /// `*(first, second)`, including the empty array `*()` and a trailing comma.
    Array(Vec<AstNode<Expr>>),
    /// `(expression)`. Each explicit pair of grouping parentheses is retained.
    /// Parentheses belonging to a call are represented by `Call` instead.
    Group(Box<AstNode<Expr>>),
}

impl Syntax for Expr {
    fn parser() -> impl Parser<Output = Self> {
        ExprParser
    }
}

struct ExprParser;

impl Parser for ExprParser {
    type Output = Expr;

    fn description(&self) -> String {
        "an expression".into()
    }

    fn read<'s, I: LexemeIterator<'s>>(
        &self,
        mut tokens: TokenStream<'s, I>,
    ) -> Result<(Option<Self::Output>, TokenStream<'s, I>), ParseError> {
        let mark = tokens.mark()?;
        let (value, rest) = primary().read(tokens)?;
        tokens = rest;
        let Some(value) = value else {
            return Ok((None, tokens));
        };
        let mut node = AstNode {
            value,
            span: Some(tokens.span(&mark)),
        };
        loop {
            let (suffix, rest) = suffix().read(tokens)?;
            tokens = rest;
            let Some(suffix) = suffix else {
                return Ok((Some(node.value), tokens));
            };
            let value = match suffix {
                Suffix::Member(name) => Expr::Member {
                    base: Box::new(node),
                    name,
                },
                Suffix::Call(arguments) => Expr::Call {
                    callee: Box::new(node),
                    arguments,
                },
            };
            node = AstNode {
                value,
                span: Some(tokens.span(&mark)),
            };
        }
    }
}

fn primary() -> impl Parser<Output = Expr> {
    let literal = sequence![
        string().try_map("a string with valid escapes", |text| decode_string(&text)),
        optional(sequence![symbol(Symbol::At), AstNode::<Type>::parser()].map(|((), ty)| ty)),
    ]
    .map(|(text, ty)| Expr::Literal { text, ty });
    let array = sequence![
        symbol(Symbol::Asterisk),
        list(
            AstNode::<Expr>::parser(),
            Symbol::OpenParen,
            Symbol::CloseParen,
            Symbol::Comma,
            true,
        ),
    ]
    .map(|((), items)| Expr::Array(items));
    let group = sequence![
        symbol(Symbol::OpenParen),
        AstNode::<Expr>::parser(),
        symbol(Symbol::CloseParen),
    ]
    .map(|((), expr, ())| Expr::Group(Box::new(expr)));
    choice![
        literal,
        array,
        group,
        AstNode::<Identifier>::parser().map(Expr::Name)
    ]
}

enum Suffix {
    Member(AstNode<Identifier>),
    Call(Vec<AstNode<Expr>>),
}

fn suffix() -> impl Parser<Output = Suffix> {
    choice![
        sequence![symbol(Symbol::Dot), AstNode::<Identifier>::parser()]
            .map(|((), name)| Suffix::Member(name)),
        list(
            AstNode::<Expr>::parser(),
            Symbol::OpenParen,
            Symbol::CloseParen,
            Symbol::Comma,
            true,
        )
        .map(Suffix::Call),
    ]
}

fn decode_string(text: &str) -> Option<Box<str>> {
    let mut decoded = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(ch) = chars.next() {
        decoded.push(if ch == '\\' {
            match chars.next()? {
                '"' => '"',
                '\\' => '\\',
                'n' => '\n',
                'r' => '\r',
                't' => '\t',
                _ => return None,
            }
        } else {
            ch
        });
    }
    Some(decoded.into_boxed_str())
}
