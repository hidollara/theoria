use super::super::lexer::Symbol;
use super::list::ParsedList;
use super::name::identifier;
use super::types::type_expr;
use super::{Rule, choice, defer, list, located, repeated, rule, string, symbol, trivia};
use crate::syntax::ast::{AstNode, Expr, Identifier, NodeSource, Trivia};

pub(super) fn expr() -> impl Rule<Output = (Expr, NodeSource)> {
    rule("an expression", |parser| {
        let (mut value, mut source) = parser.read(primary())?;
        let start = source.start;
        let mut gaps = Vec::new();
        for part in parser.read(repeated(expression_part()))? {
            let end;
            match part {
                ExprPart::Trivia(item) => {
                    gaps.push(item);
                    continue;
                }
                ExprPart::Call(arguments) => {
                    gaps.extend(arguments.trivia);
                    end = arguments.end;
                    value = Expr::Call {
                        callee: Box::new((value, source).into()),
                        arguments: arguments.items,
                    };
                }
                ExprPart::Member { name, trivia } => {
                    gaps.extend(trivia);
                    end = name.1.end;
                    value = Expr::Member {
                        base: Box::new((value, source).into()),
                        name: name.into(),
                    };
                }
            }
            source = NodeSource {
                start,
                end,
                trivia: std::mem::take(&mut gaps),
            };
        }
        source.trivia.append(&mut gaps);
        Ok((value, source))
    })
}

enum ExprPart {
    Trivia(Trivia),
    Call(ParsedList<AstNode<Expr>>),
    Member {
        name: (Identifier, NodeSource),
        trivia: Vec<Trivia>,
    },
}

fn expression_part() -> impl Rule<Output = ExprPart> {
    let call = list(
        defer(expr).map(AstNode::from),
        Symbol::OpenParen,
        Symbol::CloseParen,
        Symbol::Comma,
        true,
    )
    .map(ExprPart::Call);
    let member = rule("'.'", |parser| {
        parser.read(symbol(Symbol::Dot))?;
        let trivia = parser.read(repeated(trivia()))?;
        let name = parser.read(identifier())?;
        Ok(ExprPart::Member { name, trivia })
    });
    choice![trivia().map(ExprPart::Trivia), call, member]
}

/// Read an expression before member access or function application.
fn primary() -> impl Rule<Output = (Expr, NodeSource)> {
    let literal = located(rule("a string literal", |parser| {
        let (text, end) = parser.read(string())?;
        let mut gaps = parser.read(repeated(trivia()))?;
        if symbol(Symbol::At).read(parser)?.is_none() {
            return Ok((Expr::Literal { text, ty: None }, gaps, end));
        }
        gaps.extend(parser.read(repeated(trivia()))?);
        let (ty, source) = parser.read(type_expr())?;
        let end = source.end;
        Ok((
            Expr::Literal {
                text,
                ty: Some((ty, source).into()),
            },
            gaps,
            end,
        ))
    }));
    let array = located(rule("'*'", |parser| {
        parser.read(symbol(Symbol::Asterisk))?;
        let mut gaps = parser.read(repeated(trivia()))?;
        let items = parser.read(list(
            defer(expr).map(AstNode::from),
            Symbol::OpenParen,
            Symbol::CloseParen,
            Symbol::Comma,
            true,
        ))?;
        gaps.extend(items.trivia);
        Ok((Expr::Array(items.items), gaps, items.end))
    }));
    let name = located(identifier().map(|(name, source)| {
        let end = source.end;
        (Expr::Name((name, source).into()), Vec::new(), end)
    }));
    choice![literal, array, name]
}
