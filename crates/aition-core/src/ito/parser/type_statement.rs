use super::Rule;
use super::error::ParseError;
use super::identifier::identifier;
use super::repeated::repeated;
use super::state::Parser;
use super::trivia::trivia;
use crate::ito::lexer::Symbol;
use crate::syntax::ast::{AstNode, ModuleItem, NodeSource, TypeMember};

pub(super) fn type_statement(
    parser: &mut Parser<'_, '_>,
) -> Result<AstNode<ModuleItem>, ParseError> {
    let start = parser.cursor()?;
    if parser.keyword("type")?.is_none() {
        return Err(ParseError::expected_keyword("type", start));
    }
    let mut gaps = repeated(trivia).apply(parser)?;
    let name = identifier(parser)?;
    gaps.extend(repeated(trivia).apply(parser)?);
    if parser.symbol(Symbol::Colon)?.is_none() {
        return Err(ParseError::expected(Symbol::Colon, parser.cursor()?));
    }
    gaps.extend(repeated(trivia).apply(parser)?);
    let constructor = if let Some(open) = parser.symbol(Symbol::OpenBracket)? {
        let arguments = list(parser, open, Symbol::CloseBracket, identifier)?;
        gaps.extend(repeated(trivia).apply(parser)?);
        Some(arguments)
    } else {
        None
    };
    let Some(open) = parser.symbol(Symbol::OpenBrace)? else {
        let expected = if constructor.is_some() { "{" } else { "[ or {" };
        return Err(ParseError::expected(expected, parser.cursor()?));
    };
    let members = list(parser, open, Symbol::CloseBrace, member)?;
    Ok(AstNode {
        value: ModuleItem::Type {
            name,
            constructor,
            members,
        },
        source: Some(NodeSource {
            start,
            end: parser.cursor()?,
            trivia: gaps,
        }),
    })
}

fn list<T>(
    parser: &mut Parser<'_, '_>,
    open: AstNode<()>,
    close: Symbol,
    item: impl Rule<Output = T>,
) -> Result<AstNode<Vec<T>>, ParseError> {
    let start = open
        .source
        .expect("a parsed symbol has source positions")
        .start;
    let mut gaps = repeated(trivia).apply(parser)?;
    let mut items = Vec::new();
    while parser.symbol(close)?.is_none() {
        if !items.is_empty() {
            if parser.symbol(Symbol::Comma)?.is_none() {
                return Err(ParseError::expected(
                    format_args!(", or {close}"),
                    parser.cursor()?,
                ));
            }
            gaps.extend(repeated(trivia).apply(parser)?);
            if parser.symbol(close)?.is_some() {
                break;
            }
        }
        items.push(item.apply(parser)?);
        gaps.extend(repeated(trivia).apply(parser)?);
    }
    Ok(AstNode {
        value: items,
        source: Some(NodeSource {
            start,
            end: parser.cursor()?,
            trivia: gaps,
        }),
    })
}

fn member(parser: &mut Parser<'_, '_>) -> Result<AstNode<TypeMember>, ParseError> {
    let start = parser.cursor()?;
    let name = identifier(parser)?;
    let mut gaps = repeated(trivia).apply(parser)?;
    if parser.symbol(Symbol::Colon)?.is_none() {
        return Err(ParseError::expected(Symbol::Colon, parser.cursor()?));
    }
    gaps.extend(repeated(trivia).apply(parser)?);
    let ty = identifier(parser)?;
    Ok(AstNode {
        value: TypeMember { name, ty },
        source: Some(NodeSource {
            start,
            end: parser.cursor()?,
            trivia: gaps,
        }),
    })
}
