use super::Rule;
use super::error::ParseError;
use super::state::Parser;

/// Try candidates in order. Only unmarked candidates appear in the diagnostic.
macro_rules! choice {
    (@rules) => { $crate::ito::parser::choice::no_match };
    (@rules #[silent] $rule:expr $(, $($rest:tt)*)?) => {
        $crate::ito::parser::choice::or($rule, false, choice!(@rules $($($rest)*)?))
    };
    (@rules $rule:expr $(, $($rest:tt)*)?) => {
        $crate::ito::parser::choice::or($rule, true, choice!(@rules $($($rest)*)?))
    };
    ($($rules:tt)+) => {
        $crate::ito::parser::choice::alternatives(choice!(@rules $($rules)+))
    };
}

pub(super) use choice;

pub(super) fn alternatives<T>(
    candidates: impl Fn(&mut Parser<'_, '_>, &mut Vec<String>) -> Result<T, ParseError>,
) -> impl Rule<Output = T> {
    move |parser: &mut Parser<'_, '_>| candidates(parser, &mut Vec::new())
}

pub(super) fn or<T>(
    candidate: impl Rule<Output = T>,
    report: bool,
    rest: impl Fn(&mut Parser<'_, '_>, &mut Vec<String>) -> Result<T, ParseError>,
) -> impl Fn(&mut Parser<'_, '_>, &mut Vec<String>) -> Result<T, ParseError> {
    move |parser, alternatives| {
        let before = parser.consumed();
        match candidate.apply(parser) {
            Err(ParseError::Unexpected { expected, .. }) if parser.consumed() == before => {
                if report && !expected.is_empty() {
                    alternatives.push(expected);
                }
                rest(parser, alternatives)
            }
            result => result,
        }
    }
}

pub(super) fn no_match<T>(
    parser: &mut Parser<'_, '_>,
    alternatives: &mut Vec<String>,
) -> Result<T, ParseError> {
    let expected = match alternatives.as_mut_slice() {
        [] => String::new(),
        [only] => std::mem::take(only),
        [first, second] => format!("{first} or {second}"),
        [others @ .., last] => format!("{}, or {last}", others.join(", ")),
    };
    Err(ParseError::Unexpected {
        expected,
        cursor: parser.cursor()?,
    })
}

#[cfg(test)]
mod tests;
