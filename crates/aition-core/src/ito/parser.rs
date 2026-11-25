//! Parsing tokens into syntax, without name resolution or type checking.

mod error;
mod expr;
mod identifier;
mod module;
mod rule;
mod types;
mod util;

use super::lexer::Lexer;
use crate::input::Input;
use crate::syntax::Module;

use self::rule::{eoi, sequence};

pub use self::error::ParseError;

pub(super) fn parse<'s>(mut input: impl Input<'s>) -> Result<Module, ParseError> {
    let mut lexer = Lexer::new(&mut input);
    let file = sequence![module::body(), eoi()];
    let (body, ()) = file.apply(&mut lexer)?;
    Ok(body)
}
