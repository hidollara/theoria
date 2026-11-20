//! Reading the .ito text format into syntax structures.

mod lexer;
mod parser;

use crate::syntax::ast::{AstNode, Module};
use std::io::BufRead;

pub use lexer::LexError;
pub use parser::ParseError;

/// Parse a complete .ito module from buffered UTF-8 input.
pub fn parse<R: BufRead>(reader: R) -> Result<AstNode<Module>, ParseError> {
    parser::parse(lexer::from_reader(reader))
}

/// Parse a complete .ito module, scanning the supplied text directly.
pub fn parse_from_str(text: &str) -> Result<AstNode<Module>, ParseError> {
    parser::parse(lexer::from_str(text))
}
