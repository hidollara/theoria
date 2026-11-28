mod literal;
mod tag;

use self::literal::literal;
use self::tag::tag;
use super::identifier::identifier;
use super::rule::{Rule, choice, list, optional, recursive, repeated, sequence, symbol};
use super::types::ty;
use super::util::delimited;
use crate::ito::lexer::Symbol::*;
use crate::syntax::{Body, Expr, Identifier, Lambda, Parameter};

enum Operation {
    Member(Identifier),
    Call(Vec<Expr>),
}

pub(super) fn expr() -> Rule<Expr> {
    recursive(|expr| {
        sequence![
            term(expr.clone()),
            repeated(choice([member(), call(expr)])),
            optional(sequence![symbol(Colon), ty()]),
        ]
        .map(|(term, operations, annotation)| {
            let expr = operations.into_iter().fold(term, |base, op| match op {
                Operation::Member(name) => Expr::Member {
                    base: Box::new(base),
                    name,
                },
                Operation::Call(arguments) => Expr::Call {
                    callee: Box::new(base),
                    arguments,
                },
            });
            match annotation {
                Some((_, ty)) => Expr::Ascribed {
                    expr: Box::new(expr),
                    ty,
                },
                None => expr,
            }
        })
    })
}

fn term(expr: Rule<Expr>) -> Rule<Expr> {
    choice([
        identifier().map(Expr::Name),
        literal(),
        body(expr.clone()).map(Expr::Body),
        lambda(expr.clone()).map(Expr::Lambda),
        array(expr.clone()),
        tag(expr.clone()),
        type_match(expr),
    ])
}

fn array(expr: Rule<Expr>) -> Rule<Expr> {
    sequence![
        symbol(Asterisk),
        list(symbol(OpenParen), expr, symbol(Comma), symbol(CloseParen)),
    ]
    .map(|(_, items)| Expr::Array(items))
}

fn type_match(expr: Rule<Expr>) -> Rule<Expr> {
    sequence![
        symbol(Question),
        list(
            symbol(OpenParen),
            expr.clone(),
            symbol(Comma),
            symbol(CloseParen)
        ),
        list(
            symbol(OpenBracket),
            lambda(expr),
            symbol(Comma),
            symbol(CloseBracket)
        ),
    ]
    .map(|(_, arguments, branches)| Expr::Match {
        arguments,
        branches,
    })
}

fn member() -> Rule<Operation> {
    sequence![symbol(Dot), identifier()].map(|(_, name)| Operation::Member(name))
}

fn call(expr: Rule<Expr>) -> Rule<Operation> {
    list(symbol(OpenParen), expr, symbol(Comma), symbol(CloseParen)).map(Operation::Call)
}

fn body(expr: Rule<Expr>) -> Rule<Body> {
    let binding =
        sequence![identifier(), symbol(Equal), expr.clone()].map(|(name, _, expr)| (name, expr));
    let bindings = optional(delimited(
        symbol(OpenBrace),
        repeated(binding),
        symbol(CloseBrace),
    ));
    let result = delimited(symbol(OpenParen), expr, symbol(CloseParen));
    sequence![result, bindings].map(|(result, bindings)| Body {
        result: Box::new(result),
        bindings,
    })
}

fn lambda(expr: Rule<Expr>) -> Rule<Lambda> {
    let parameters = list(
        symbol(OpenBracket),
        parameter(),
        symbol(Comma),
        symbol(CloseBracket),
    );
    sequence![parameters, body(expr)].map(|(parameters, body)| Lambda { parameters, body })
}

pub(super) fn parameter() -> Rule<Parameter> {
    sequence![identifier(), optional(sequence![symbol(Colon), ty()])].map(|(name, annotation)| {
        Parameter {
            name,
            ty: annotation.map(|(_, ty)| ty),
        }
    })
}
