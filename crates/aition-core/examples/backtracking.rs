//! Experiment: a candidate succeeds only when its continuation also succeeds.
//!
//! Run with `cargo run -p aition-core --example backtracking`.
//! This is a small grammar fixture, not a replacement for the Aition parser.
//! Choices are ordered: the first complete interpretation wins. Lexical errors
//! abort the search. Grammar failures retain the farthest expectation.
//!
//! The prototype buffers tokens until parsing finishes and clones prefix values
//! when retrying a suffix. It has no memoization, reclamation, or left-recursion
//! support. Mapping functions may run for discarded candidates and must be pure.

#[path = "backtracking/parser.rs"]
mod parser;

use aition_core::lexer::{self, Symbol};
use parser::{Parser, choice, defer, identifier, list, parse, sequence, symbol};

#[derive(Clone, Debug, PartialEq, Eq)]
enum Expr {
    Name(String),
    Array(Vec<Expr>),
    Lambda(Vec<String>, Box<Expr>),
}

fn expression() -> Parser<Expr> {
    let array = list(defer(expression), Symbol::OpenBracket, Symbol::CloseBracket).map(Expr::Array);
    let lambda = sequence![
        list(identifier(), Symbol::OpenBracket, Symbol::CloseBracket),
        symbol(Symbol::OpenParen),
        defer(expression),
        symbol(Symbol::CloseParen),
    ]
    .map(|(parameters, (), body, ())| Expr::Lambda(parameters, Box::new(body)));
    let group = sequence![
        symbol(Symbol::OpenParen),
        defer(expression),
        symbol(Symbol::CloseParen),
    ]
    .map(|((), value, ())| value);
    // Deliberately try the shorter, overlapping candidate first.
    choice![array, lambda, identifier().map(Expr::Name), group]
}

fn binding() -> Parser<(String, Expr)> {
    sequence![
        identifier(),
        symbol(Symbol::Equal),
        defer(expression),
        symbol(Symbol::Semicolon),
    ]
    .map(|(name, (), value, ())| (name, value))
}

fn main() {
    for source in [
        "value = [x];",
        "value = [x] (x);",
        "value = [[x] (x), [y],];",
        "value = [x] (x;",
    ] {
        println!("{source}");
        match parse(binding(), lexer::from_str(source)) {
            Ok((value, _)) => println!("  {value:?}"),
            Err(error) => println!("  {error}"),
        }
    }
}

#[cfg(test)]
#[path = "backtracking/tests.rs"]
mod tests;
