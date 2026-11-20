//! Reading the .ito text format into syntax structures.

mod lexer;
mod parser;

use crate::input::{ReaderInput, StrInput};
use crate::syntax::Module;
use std::io::BufRead;

pub use lexer::{Cursor, LexError};
pub use parser::ParseError;

/// Parse a complete .ito module from buffered UTF-8 input.
pub fn parse<R: BufRead>(reader: R) -> Result<Module, ParseError> {
    parser::parse(ReaderInput::new(reader))
}

/// Parse a complete .ito module, scanning the supplied text directly.
pub fn parse_from_str(text: &str) -> Result<Module, ParseError> {
    parser::parse(StrInput::new(text))
}
