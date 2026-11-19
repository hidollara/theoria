use super::super::lexer::Symbol;
use super::name::qualified_name;
use super::{Rule, choice, defer, list, located, repeated, rule, symbol, trivia};
use crate::syntax::ast::{AstNode, NodeSource, Type};

pub(super) fn type_expr() -> impl Rule<Output = (Type, NodeSource)> {
    rule("a type", |parser| {
        let (first, mut source) = parser.read(defer(term))?;
        let mut gaps = parser.read(repeated(trivia()))?;
        if symbol(Symbol::Pipe).read(parser)?.is_none() {
            source.trivia.append(&mut gaps);
            return Ok((first, source));
        }

        let start = source.start;
        let mut terms = vec![(first, source).into()];
        loop {
            gaps.extend(parser.read(repeated(trivia()))?);
            let (operand, source) = parser.read(defer(term))?;
            let end = source.end;
            terms.push((operand, source).into());
            gaps.extend(parser.read(repeated(trivia()))?);
            if symbol(Symbol::Pipe).read(parser)?.is_none() {
                return Ok((
                    Type::Union(terms),
                    NodeSource {
                        start,
                        end,
                        trivia: gaps,
                    },
                ));
            }
        }
    })
}

/// One union operand. A union inside an operand must be parenthesized.
fn term() -> impl Rule<Output = (Type, NodeSource)> {
    let array = located(rule("'*'", |parser| {
        parser.read(symbol(Symbol::Asterisk))?;
        let gaps = parser.read(repeated(trivia()))?;
        let (inner, source) = parser.read(defer(term))?;
        let end = source.end;
        Ok((Type::Array(Box::new((inner, source).into())), gaps, end))
    }));

    let group = located(rule("'('", |parser| {
        parser.read(symbol(Symbol::OpenParen))?;
        let gaps = parser.read(repeated(trivia()))?;
        let inner = parser.read(defer(type_expr))?;
        let end = parser.read(symbol(Symbol::CloseParen))?;
        Ok((Type::Group(Box::new(inner.into())), gaps, end))
    }));

    let function = located(rule("'['", |parser| {
        let parameters = parser.read(list(
            defer(type_expr).map(AstNode::from),
            Symbol::OpenBracket,
            Symbol::CloseBracket,
            Symbol::Comma,
            true,
        ))?;
        let mut gaps = parameters.trivia;
        gaps.extend(parser.read(repeated(trivia()))?);
        parser.read(symbol(Symbol::OpenParen))?;
        gaps.extend(parser.read(repeated(trivia()))?);
        let result = parser.read(defer(type_expr))?;
        let end = parser.read(symbol(Symbol::CloseParen))?;
        Ok((
            Type::Function {
                parameters: parameters.items,
                result: Box::new(result.into()),
            },
            gaps,
            end,
        ))
    }));

    let named = located(rule("an identifier", |parser| {
        let (name, source) = parser.read(qualified_name())?;
        let name_end = source.end;
        let name = (name, source).into();
        let arguments = list(
            defer(type_expr).map(AstNode::from),
            Symbol::OpenAngle,
            Symbol::CloseAngle,
            Symbol::Comma,
            false,
        );
        let (arguments, gaps, end) = match arguments.read(parser)? {
            Some(list) => (list.items, list.trivia, list.end),
            None => (Vec::new(), Vec::new(), name_end),
        };
        Ok((Type::Named { name, arguments }, gaps, end))
    }));

    choice![array, group, function, named].label("a type")
}
