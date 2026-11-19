use aition_core::{
    lexer::{Cursor, LexError, Lexeme, LexemeIterator, LexemeKind, Symbol},
    syntax::{ParseError, Trivia, TriviaKind, ast::Identifier},
};
use std::rc::Rc;

// `true` means the whole continuation accepted the candidate, not just this
// parser. `false` asks for another candidate; lexical errors stop all choices.
type Search = Result<bool, LexError>;
type Continuation<'a, 's, T> = dyn FnMut(&mut Input<'s>, T, usize) -> Search + 'a;
type Read<T> = dyn for<'s, 'a> Fn(&mut Input<'s>, usize, &mut Continuation<'a, 's, T>) -> Search;

pub(super) struct Parser<T>(Rc<Read<T>>);

impl<T> Clone for Parser<T> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<T: 'static> Parser<T> {
    fn new(
        read: impl for<'s, 'a> Fn(&mut Input<'s>, usize, &mut Continuation<'a, 's, T>) -> Search
        + 'static,
    ) -> Self {
        Self(Rc::new(read))
    }

    pub fn map<U: 'static>(self, f: impl Fn(T) -> U + 'static) -> Parser<U> {
        Parser::new(move |input, start, accept| {
            (self.0)(input, start, &mut |input, value, end| {
                accept(input, f(value), end)
            })
        })
    }
}

pub(super) fn sequence_pair<A: Clone + 'static, B: 'static>(
    first: Parser<A>,
    second: Parser<B>,
) -> Parser<(A, B)> {
    Parser::new(move |input, start, accept| {
        (first.0)(input, start, &mut |input, first, middle| {
            // A rejected continuation tries another second value, then another
            // first value. Each retry starts at its original token index.
            (second.0)(input, middle, &mut |input, second, end| {
                accept(input, (first.clone(), second), end)
            })
        })
    })
}

pub(super) fn choice_pair<T: 'static>(first: Parser<T>, second: Parser<T>) -> Parser<T> {
    Parser::new(move |input, start, accept| {
        if (first.0)(input, start, accept)? {
            Ok(true)
        } else {
            (second.0)(input, start, accept)
        }
    })
}

pub(super) fn defer<T: 'static>(make: impl Fn() -> Parser<T> + 'static) -> Parser<T> {
    Parser::new(move |input, start, accept| (make().0)(input, start, accept))
}

fn token<T: 'static>(
    expected: String,
    matches: impl Fn(&LexemeKind<'_>) -> Option<T> + 'static,
) -> Parser<T> {
    Parser::new(move |input, start, accept| {
        let next = input.at(start)?;
        if let Lookahead::Token(token) = next
            && let Some(value) = matches(&token.kind)
        {
            return accept(input, value, start + 1);
        }
        let cursor = next.cursor();
        input.reject(&expected, cursor);
        Ok(false)
    })
}

pub(super) fn symbol(symbol: Symbol) -> Parser<()> {
    token(format!("'{symbol}'"), move |kind| {
        matches!(kind, LexemeKind::Symbol(found) if *found == symbol).then_some(())
    })
}

pub(super) fn identifier() -> Parser<String> {
    token("an identifier".into(), |kind| {
        let LexemeKind::Word(text) = kind else {
            return None;
        };
        Identifier::new(text.as_ref())
            .ok()
            .map(|name| name.as_str().to_owned())
    })
}

macro_rules! sequence {
    (@collect [$($parsers:expr,)*] [$($values:ident,)*]; $next:expr $(, $rest:expr)*) => {
        $crate::parser::sequence!(@collect
            [$($parsers,)* $next,] [$($values,)* value,]; $($rest),*)
    };
    (@collect [$($parsers:expr,)+] [$($values:ident,)+];) => {
        $crate::parser::sequence!(@pairs $($parsers),+)
            .map(|$crate::parser::sequence!(@pattern $($values),+)| ($($values,)+))
    };
    (@pairs $first:expr, $second:expr $(, $rest:expr)*) => {
        $crate::parser::sequence!(@pairs
            $crate::parser::sequence_pair($first, $second) $(, $rest)*)
    };
    (@pairs $last:expr) => { $last };
    (@pattern $first:pat, $second:pat $(, $rest:pat)*) => {
        $crate::parser::sequence!(@pattern ($first, $second) $(, $rest)*)
    };
    (@pattern $last:pat) => { $last };
    ($($parser:expr),+ $(,)?) => {
        $crate::parser::sequence!(@collect [] []; $($parser),+)
    };
}

macro_rules! choice {
    ($first:expr, $($rest:expr),+ $(,)?) => {
        $crate::parser::choice_pair($first, $crate::parser::choice![$($rest),+])
    };
    ($last:expr $(,)?) => { $last };
}

pub(super) use {choice, sequence};

/// A grammar built entirely from sequence, choice, mapping, and deferred recursion.
/// It permits empty lists and one trailing comma, but not missing items.
pub(super) fn list<T: Clone + 'static>(
    item: Parser<T>,
    open: Symbol,
    close: Symbol,
) -> Parser<Vec<T>> {
    sequence![
        symbol(open),
        choice![
            symbol(close).map(|()| Vec::new()),
            sequence![item.clone(), tail(item, close)]
                .map(|(first, rest)| std::iter::once(first).chain(rest).collect()),
        ],
    ]
    .map(|((), items)| items)
}

fn tail<T: Clone + 'static>(item: Parser<T>, close: Symbol) -> Parser<Vec<T>> {
    let next_item = item.clone();
    choice![
        symbol(close).map(|()| Vec::new()),
        sequence![
            symbol(Symbol::Comma),
            choice![
                symbol(close).map(|()| Vec::new()),
                sequence![item, defer(move || tail(next_item.clone(), close))]
                    .map(|(first, rest)| std::iter::once(first).chain(rest).collect()),
            ],
        ]
        .map(|((), items)| items),
    ]
}

struct Input<'s> {
    lexemes: Box<dyn LexemeIterator<'s> + 's>,
    tokens: Vec<Lexeme<'s>>,
    trivia: Vec<Trivia>,
    read_end: Cursor,
    eoi: Option<Cursor>,
    failure: Option<(Cursor, Vec<String>)>,
}

enum Lookahead<'a, 's> {
    Token(&'a Lexeme<'s>),
    Eoi(Cursor),
}

impl Lookahead<'_, '_> {
    fn cursor(&self) -> Cursor {
        match self {
            Self::Token(token) => token.start,
            Self::Eoi(cursor) => *cursor,
        }
    }
}

impl<'s> Input<'s> {
    fn at(&mut self, index: usize) -> Result<Lookahead<'_, 's>, LexError> {
        while index >= self.tokens.len() && self.eoi.is_none() {
            let Some(lexeme) = self.lexemes.next() else {
                self.eoi = Some(self.read_end);
                break;
            };
            let lexeme = lexeme?;
            self.read_end = lexeme.end;
            let kind = match lexeme.kind {
                LexemeKind::Whitespace(text) => TriviaKind::Whitespace(text.into()),
                LexemeKind::LineComment(text) => TriviaKind::LineComment(text.into()),
                _ => {
                    self.tokens.push(lexeme);
                    continue;
                }
            };
            self.trivia.push(Trivia {
                kind,
                start: lexeme.start,
                end: lexeme.end,
            });
        }
        Ok(match self.tokens.get(index) {
            Some(token) => Lookahead::Token(token),
            None => Lookahead::Eoi(self.eoi.unwrap()),
        })
    }

    fn reject(&mut self, expected: &str, cursor: Cursor) {
        match &mut self.failure {
            Some((farthest, expectations)) if farthest.offset == cursor.offset => {
                if !expectations.iter().any(|item| item == expected) {
                    expectations.push(expected.to_owned());
                }
            }
            Some((farthest, _)) if farthest.offset > cursor.offset => {}
            _ => self.failure = Some((cursor, vec![expected.to_owned()])),
        }
    }
}

pub(super) fn parse<'s, T: 'static>(
    parser: Parser<T>,
    lexemes: impl LexemeIterator<'s> + 's,
) -> Result<(T, Vec<Trivia>), ParseError> {
    let mut input = Input {
        lexemes: Box::new(lexemes),
        tokens: Vec::new(),
        trivia: Vec::new(),
        read_end: Cursor::default(),
        eoi: None,
        failure: None,
    };
    let mut result = None;
    (parser.0)(&mut input, 0, &mut |input, value, end| {
        let next = input.at(end)?;
        if matches!(next, Lookahead::Eoi(_)) {
            result = Some(value);
            Ok(true)
        } else {
            let cursor = next.cursor();
            input.reject("end of input", cursor);
            Ok(false)
        }
    })?;
    match result {
        Some(value) => Ok((value, input.trivia)),
        None => {
            let (cursor, expectations) =
                input.failure.expect("a rejected search records a failure");
            let expected = match expectations.as_slice() {
                [only] => only.clone(),
                [first, second] => format!("{first} or {second}"),
                many => {
                    let (last, prefix) = many.split_last().unwrap();
                    format!("{}, or {last}", prefix.join(", "))
                }
            };
            Err(ParseError::Unexpected { expected, cursor })
        }
    }
}
