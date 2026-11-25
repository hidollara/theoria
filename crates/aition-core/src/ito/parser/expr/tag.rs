use super::super::error::ParseError;
use super::super::identifier::{identifier, qualified_name};
use super::super::rule::{Rule, adjacent, choice, recursive, repeated, sequence, symbol};
use super::body;
use super::literal::literal;
use crate::ito::Cursor;
use crate::ito::lexer::Symbol::{CloseAngle, Equal, OpenAngle, Slash};
use crate::syntax::{Expr, Identifier, QualifiedName, Tag, TagArgument};

pub(super) fn tag(expr: Rule<Expr>) -> Rule<Expr> {
    sequence![
        symbol(OpenAngle),
        recursive(|nested| {
            let argument = sequence![
                identifier(),
                symbol(Equal),
                choice([literal(), body(expr.clone()).map(Expr::Body)]),
            ]
            .map(|(name, _, value)| TagArgument { name, value });
            let self_closing =
                sequence![symbol(Slash), adjacent(), symbol(CloseAngle)].map(|_| None);
            sequence![
                adjacent(),
                qualified_name(),
                repeated(argument),
                choice([self_closing, paired_tag(nested, expr).map(Some)]),
            ]
            .try_map(|(_, name, arguments, paired)| {
                let children = match paired {
                    Some((children, closing, cursor)) => {
                        if !name
                            .as_slice()
                            .iter()
                            .map(Identifier::as_str)
                            .eq(closing.as_slice().iter().map(Identifier::as_str))
                        {
                            let expected = name
                                .as_slice()
                                .iter()
                                .map(Identifier::as_str)
                                .collect::<Vec<_>>()
                                .join(".");
                            return Err(ParseError::expected(
                                format_args!("closing tag </{expected}>"),
                                cursor,
                            ));
                        }
                        Some(children)
                    }
                    None => None,
                };
                Ok(Tag {
                    name,
                    arguments,
                    children,
                })
            })
        }),
    ]
    .map(|(_, tag)| Expr::Tag(tag))
}

fn paired_tag(nested: Rule<Tag>, expr: Rule<Expr>) -> Rule<(Vec<Expr>, QualifiedName, Cursor)> {
    sequence![
        symbol(CloseAngle),
        tag_children(nested, expr),
        closing_tag(),
    ]
    .map(|(_, children, (name, cursor))| (children, name, cursor))
}

fn tag_children(nested: Rule<Tag>, expr: Rule<Expr>) -> Rule<Vec<Expr>> {
    let text_or_body = repeated(choice([literal(), body(expr).map(Expr::Body)]));
    // Consume each '<' before distinguishing a child tag from the closing tag.
    sequence![
        text_or_body.clone(),
        symbol(OpenAngle),
        repeated(sequence![nested, text_or_body, symbol(OpenAngle)]),
    ]
    .map(|(mut children, _, nested)| {
        for (tag, following, _) in nested {
            children.push(Expr::Tag(tag));
            children.extend(following);
        }
        children
    })
}

fn closing_tag() -> Rule<(QualifiedName, Cursor)> {
    sequence![
        adjacent(),
        symbol(Slash),
        adjacent(),
        qualified_name(),
        adjacent(),
        symbol(CloseAngle),
    ]
    .map(|(_, slash, _, name, _, _)| (name, slash.advanced(['/'])))
}
