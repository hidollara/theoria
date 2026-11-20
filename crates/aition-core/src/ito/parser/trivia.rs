use super::error::ParseError;
use super::state::Parser;
use crate::syntax::ast::{AstNode, Trivia, TriviaKind};

/// Read one whitespace or line comment token.
pub(super) fn trivia(parser: &mut Parser<'_, '_>) -> Result<Trivia, ParseError> {
    if let Some(space) = parser.whitespace()? {
        return Ok(text_trivia(space, TriviaKind::Whitespace));
    }
    if let Some(comment) = parser.comment()? {
        return Ok(text_trivia(comment, TriviaKind::LineComment));
    }
    Err(ParseError::expected(
        "whitespace or a comment",
        parser.cursor()?,
    ))
}

fn text_trivia(text: AstNode<Box<str>>, kind: fn(Box<str>) -> TriviaKind) -> Trivia {
    let source = text
        .source
        .expect("a parsed text token has source positions");
    Trivia {
        kind: kind(text.value),
        start: source.start,
        end: source.end,
    }
}

#[cfg(test)]
mod tests {
    use super::trivia;
    use crate::ito::lexer;
    use crate::ito::parser::Rule;
    use crate::ito::parser::error::ParseError;
    use crate::ito::parser::state::Parser;
    use crate::syntax::ast::TriviaKind;

    #[test]
    fn a_space_and_a_comment_are_two_separate_matches() {
        let mut stream = lexer::from_str(" // note");
        let mut parser = Parser::new(&mut stream);

        let space = trivia.apply(&mut parser).unwrap();
        let comment = trivia.apply(&mut parser).unwrap();

        assert_eq!(space.kind, TriviaKind::Whitespace(" ".into()));
        assert_eq!(comment.kind, TriviaKind::LineComment(" note".into()));
        assert!(parser.is_eoi().unwrap());
    }

    #[test]
    fn a_word_is_reported_at_its_start_and_left_unread() {
        let mut stream = lexer::from_str(" word");
        let mut parser = Parser::new(&mut stream);
        trivia.apply(&mut parser).unwrap();

        let error = trivia.apply(&mut parser).unwrap_err();

        assert_eq!(
            error.to_string(),
            "expected whitespace or a comment at line 1, column 2"
        );
        assert_eq!(error.cursor().offset, 1);
        let unread = parser.word().unwrap().unwrap();
        assert_eq!(unread.value.as_ref(), "word");
    }

    #[test]
    fn missing_trivia_after_a_line_break_is_reported_at_eoi() {
        let mut stream = lexer::from_str("\r\n");
        let mut parser = Parser::new(&mut stream);
        trivia.apply(&mut parser).unwrap();

        let error = trivia.apply(&mut parser).unwrap_err();

        assert!(matches!(error, ParseError::Unexpected { .. }));
        assert_eq!(error.cursor().offset, 2);
        assert_eq!((error.cursor().line, error.cursor().column), (2, 1));
    }
}
