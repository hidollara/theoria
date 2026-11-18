use aition_core::{
    lexer::{self, LexError, LexemeIterator, Symbol},
    syntax::{
        Expected, ParseError, Parsed, TokenStream, TriviaKind,
        ast::{AstNode, Identifier, QualifiedName, Type},
        parse,
    },
};

fn parse_source(source: &str) -> Result<Parsed<AstNode<Type>>, ParseError> {
    parse_lexemes(lexer::from_str(source))
}

fn parse_lexemes<'source>(
    lexemes: impl LexemeIterator<'source>,
) -> Result<Parsed<AstNode<Type>>, ParseError> {
    parse::<AstNode<Type>>(TokenStream::new(lexemes))
}

fn assert_unexpected(error: ParseError, span: std::ops::Range<usize>, expected: Expected) {
    match error {
        ParseError::Unexpected {
            span: actual_span,
            expected: actual_expected,
            ..
        } => {
            assert_eq!(actual_span, span);
            assert_eq!(actual_expected, expected);
        }
        other => panic!("expected a syntax error, got {other:?}"),
    }
}

fn named(name: &str) -> Type {
    Type::Named {
        name: QualifiedName::new(Identifier::new(name).unwrap().into(), []).into(),
        arguments: Vec::new(),
    }
}

fn group(ty: Type) -> Type {
    Type::Group(Box::new(ty.into()))
}

fn array(ty: Type) -> Type {
    Type::Array(Box::new(ty.into()))
}

fn function(parameters: Vec<Type>, result: Type) -> Type {
    Type::Function {
        parameters: parameters.into_iter().map(AstNode::new).collect(),
        result: Box::new(result.into()),
    }
}

#[test]
fn reads_identifier_spellings_without_normalizing_names() {
    for name in ["Panel", "drawingOf", "_", "_width", "width2", "read_error"] {
        assert_eq!(parse_source(name).unwrap().structure(), named(name));
    }
}

#[test]
fn validates_identifier_spelling_after_lexing_a_whole_word() {
    for name in ["foo幅", "123abc", "幅", "aé", "foo-bar", "a#b"] {
        for prefix in ["", "module.", "T<", "A | "] {
            let source = format!("{prefix}{name}");
            assert_unexpected(
                parse_source(&source).unwrap_err(),
                prefix.len()..source.len(),
                Expected::Identifier,
            );
        }
    }
}

#[test]
fn accepts_type_arguments_with_or_without_a_trailing_comma() {
    for (sources, arguments) in [
        (["T<A>", "T<A,>"], vec![named("A")]),
        (["T<A, B>", "T<A, B,>"], vec![named("A"), named("B")]),
    ] {
        let expected = Type::Named {
            name: QualifiedName::new(Identifier::new("T").unwrap().into(), []).into(),
            arguments: arguments.into_iter().map(AstNode::new).collect(),
        };
        for source in sources {
            assert_eq!(
                parse_source(source).unwrap().structure(),
                expected,
                "{source:?}"
            );
        }
    }
}

#[test]
fn reads_qualified_names_and_nested_type_arguments() {
    let image = Type::Named {
        name: QualifiedName::new(Identifier::new("Image").unwrap().into(), []).into(),
        arguments: vec![named("Pixel").into()],
    };
    let expected = Type::Named {
        name: QualifiedName::new(
            Identifier::new("panels").unwrap().into(),
            [Identifier::new("Picture").unwrap().into()],
        )
        .into(),
        arguments: vec![image.into(), union(vec![named("A"), named("B")]).into()],
    };
    assert_eq!(
        parse_source("panels.Picture<Image<Pixel>, A | B,>")
            .unwrap()
            .structure(),
        expected
    );
}

#[test]
fn keeps_union_order_repetitions_and_explicit_groups() {
    assert_eq!(
        parse_source("A | (B | A) | A").unwrap().structure(),
        union(vec![
            named("A"),
            group(union(vec![named("B"), named("A")])),
            named("A"),
        ])
    );
    assert_eq!(
        parse_source("(((Panel)))").unwrap().structure(),
        group(group(group(named("Panel"))))
    );
}

#[test]
fn array_suffix_binds_more_tightly_than_union() {
    assert_eq!(
        parse_source("A | B**").unwrap().structure(),
        union(vec![named("A"), array(array(named("B")))])
    );
    assert_eq!(
        parse_source("(A | B)*").unwrap().structure(),
        array(group(union(vec![named("A"), named("B")])))
    );
    assert_eq!(
        parse_source("[A] (B)*").unwrap().structure(),
        array(function(vec![named("A")], named("B")))
    );
}

#[test]
fn function_delimiters_do_not_add_group_nodes() {
    let expected = function(vec![named("A"), named("B")], named("C"));
    for source in ["[A,B](C)", "[A, B,] (C)", "\n[ A , B , ]\t( C )\r\n"] {
        assert_eq!(
            parse_source(source).unwrap().structure(),
            expected,
            "{source:?}"
        );
    }
    assert_eq!(
        parse_source("[A] ((B))").unwrap().structure(),
        function(vec![named("A")], group(named("B")))
    );
}

#[test]
fn reads_zero_parameter_and_higher_order_function_types() {
    assert_eq!(
        parse_source("[] (Canvas)").unwrap().structure(),
        function(Vec::new(), named("Canvas"))
    );
    assert_eq!(
        parse_source("[Style] ([Panel] (Canvas))")
            .unwrap()
            .structure(),
        function(
            vec![named("Style")],
            function(vec![named("Panel")], named("Canvas"))
        )
    );
    assert_eq!(
        parse_source("[[Panel] (Canvas)] (Canvas | Error)")
            .unwrap()
            .structure(),
        function(
            vec![function(vec![named("Panel")], named("Canvas"))],
            union(vec![named("Canvas"), named("Error")])
        )
    );
}

#[test]
fn reports_expected_syntax_and_the_unexpected_token() {
    for (source, span, expected) in [
        ("", 0..0, Expected::Type),
        ("A |", 3..3, Expected::Type),
        ("A..B", 2..3, Expected::Identifier),
        ("A.", 2..2, Expected::Identifier),
        ("A<>", 2..3, Expected::Type),
        ("A<B]", 3..4, Expected::Symbol(Symbol::CloseAngle)),
        ("A<B", 3..3, Expected::Symbol(Symbol::CloseAngle)),
        ("()", 1..2, Expected::Type),
        ("[A,]", 4..4, Expected::Symbol(Symbol::OpenParen)),
        ("[A,,](B)", 3..4, Expected::Type),
        ("[A](B,)", 5..6, Expected::Symbol(Symbol::CloseParen)),
        ("[A](B", 5..5, Expected::Symbol(Symbol::CloseParen)),
        ("A Extra", 2..7, Expected::EndOfInput),
    ] {
        assert_unexpected(parse_source(source).unwrap_err(), span, expected);
    }
}

#[test]
fn error_ranges_use_utf8_byte_boundaries_in_the_original_source() {
    let source = "\r\nA | 幅";
    let error = parse_source(source).unwrap_err();
    let ParseError::Unexpected { ref span, .. } = error else {
        panic!("expected an unexpected token, got {error:?}");
    };
    assert_eq!(*span, 6..9);
    assert_eq!(&source[span.clone()], "幅");
    assert_eq!(
        error.to_string(),
        "expected an identifier at line 2, column 5"
    );

    let source = "A.\r\n  ";
    let error = parse_source(source).unwrap_err();
    assert_unexpected(error, source.len()..source.len(), Expected::Identifier);
}

#[test]
fn rejects_value_syntax_and_invalid_type_forms() {
    for source in [
        "123",
        "panel-width",
        "A?",
        "[x: A](B)",
        "[A] -> B",
        "[A]()",
        "A || B",
        "A<B,,>",
        "A<B>.C",
        "A)",
        "A;",
    ] {
        assert!(parse_source(source).is_err(), "{source:?}");
    }
}

#[test]
fn retains_trivia_in_order_without_attaching_it_to_syntax() {
    let source = "// 型\r\n T< // argument\n A,\tB > // end";
    let parsed = parse_source(source).unwrap();
    assert_eq!(
        parsed.structure(),
        parse_source("T<A, B>").unwrap().structure()
    );

    let expected = [
        (TriviaKind::LineComment(" 型".into()), "// 型"),
        (TriviaKind::Whitespace("\r\n ".into()), "\r\n "),
        (TriviaKind::Whitespace(" ".into()), " "),
        (TriviaKind::LineComment(" argument".into()), "// argument"),
        (TriviaKind::Whitespace("\n ".into()), "\n "),
        (TriviaKind::Whitespace("\t".into()), "\t"),
        (TriviaKind::Whitespace(" ".into()), " "),
        (TriviaKind::Whitespace(" ".into()), " "),
        (TriviaKind::LineComment(" end".into()), "// end"),
    ];
    let actual: Vec<_> = parsed
        .trivia()
        .iter()
        .map(|trivia| (trivia.kind.clone(), &source[trivia.span.clone()]))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(parsed.trivia()[0].span, 0..6);
    assert!(
        parsed
            .trivia()
            .windows(2)
            .all(|pair| pair[0].span.end <= pair[1].span.start)
    );
}

#[test]
fn line_comments_keep_body_spaces_but_exclude_line_endings() {
    for newline in ["\n", "\r\n", "\r"] {
        let source = format!("A// hello  {newline}");
        let parsed = parse_source(&source).unwrap();
        assert_eq!(parsed.structure(), named("A"));
        assert_eq!(parsed.trivia().len(), 2);
        let comment = &parsed.trivia()[0];
        assert_eq!(comment.kind, TriviaKind::LineComment(" hello  ".into()));
        assert_eq!(&source[comment.span.clone()], "// hello  ");
        let whitespace = &parsed.trivia()[1];
        assert_eq!(whitespace.kind, TriviaKind::Whitespace(newline.into()));
        assert_eq!(&source[whitespace.span.clone()], newline);
    }
    let parsed = parse_source("A//").unwrap();
    assert_eq!(parsed.trivia().len(), 1);
    assert_eq!(parsed.trivia()[0].span, 1..3);
    assert_eq!(parsed.trivia()[0].kind, TriviaKind::LineComment("".into()));
}

#[test]
fn retained_comments_preserve_the_lexed_body_independently_of_the_input() {
    let parsed = {
        let source = String::from("A// \"幅\" // body  ");
        parse_source(&source).unwrap()
    };
    assert_eq!(
        parsed.trivia()[0].kind,
        TriviaKind::LineComment(" \"幅\" // body  ".into()),
    );
    assert_eq!(parsed.trivia()[0].span, 1..19);
}

#[test]
fn trivia_does_not_merge_tokens_or_hide_syntax_errors() {
    for (source, span, expected) in [
        ("A B", 2..3, Expected::EndOfInput),
        ("A// comment\nB", 12..13, Expected::EndOfInput),
        ("A / B", 2..3, Expected::EndOfInput),
        ("A | // 幅", 10..10, Expected::Type),
        ("// only a comment", 17..17, Expected::Type),
    ] {
        assert_unexpected(parse_source(source).unwrap_err(), span, expected);
    }
}

#[test]
fn lexical_errors_propagate_from_nested_types_and_trailing_input() {
    for source in [
        "\"",
        "// comment\n\"幅",
        "A | \"幅",
        "T<\"幅",
        "[A] (\"幅",
        "A // comment\n\"幅\\",
    ] {
        let opening = source.find('"').unwrap();
        let error = parse_source(source).unwrap_err();
        assert!(
            matches!(
                error,
                ParseError::Lexical(LexError::UnterminatedString { .. })
            ),
            "{source:?}"
        );
        assert_eq!(error.cursor().offset, opening);
        assert_eq!(
            error.to_string(),
            format!(
                "unterminated string at line {}, column {}",
                error.cursor().line,
                error.cursor().column
            ),
        );
        assert!(std::error::Error::source(&error).is_some());
    }
}

#[test]
fn a_complete_string_is_a_valid_token_but_not_a_type() {
    let error = parse_source("\"hello\"").unwrap_err();
    assert!(std::error::Error::source(&error).is_none());
    assert_unexpected(error, 0..7, Expected::Type);
}

#[test]
fn streaming_and_string_parsing_return_the_same_owned_result() {
    use std::io::BufReader;
    for source in [
        "Panel",
        "// 型😀\r\n [T<A, B>,] (C | (D*)*) // 終わり",
        "[] (panels.Picture<Pixel>)\u{2003}",
    ] {
        let expected = parse_source(source).unwrap();
        for capacity in 1..=8 {
            let reader = BufReader::with_capacity(capacity, source.as_bytes());
            assert_eq!(
                parse_lexemes(lexer::from_reader(reader)).unwrap(),
                expected,
                "{capacity}"
            );
        }
    }
}

#[test]
fn streaming_reports_eof_positions_after_trivia() {
    for source in ["", " // 型", "A | // 終わり\n", "T<A,\u{2003}"] {
        let expected = parse_source(source).unwrap_err();
        let actual = parse_lexemes(lexer::from_reader(std::io::BufReader::with_capacity(
            1,
            source.as_bytes(),
        )))
        .unwrap_err();
        assert_eq!(actual.cursor(), expected.cursor());
        assert_eq!(actual.to_string(), expected.to_string());
    }
}

#[test]
fn streaming_rejects_invalid_and_incomplete_utf8() {
    use std::io::{BufReader, ErrorKind};
    for bytes in [
        &[0xff][..],
        &[0x80],
        &[0xc0, 0xaf],
        &[0xf5],
        &[0xe5],
        &[0xe5, 0xb9],
        &[0xe5, b'A'],
        &[0xe0, 0x80, 0x80],
        &[0xed, 0xa0, 0x80],
        &[0xf4, 0x90, 0x80, 0x80],
    ] {
        let mut source = b"A // ".to_vec();
        source.extend_from_slice(bytes);
        let error = parse_lexemes(lexer::from_reader(BufReader::with_capacity(
            1,
            source.as_slice(),
        )))
        .unwrap_err();
        assert_eq!(error.cursor().offset, 5);
        let ParseError::Lexical(LexError::Input(error)) = error else {
            panic!("expected an input error");
        };
        assert_eq!(error.start.offset, 5);
        assert_eq!(error.cause.kind(), ErrorKind::InvalidData);
        assert!(std::error::Error::source(&error).is_some());
    }
}

#[test]
fn reader_failures_are_not_treated_as_eof_or_unterminated_strings() {
    use std::io::{self, BufRead, Read};

    struct FailsAfter<'a>(&'a [u8]);
    impl Read for FailsAfter<'_> {
        fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
            let buffer = self.fill_buf()?;
            let count = output.len().min(buffer.len());
            output[..count].copy_from_slice(&buffer[..count]);
            self.consume(count);
            Ok(count)
        }
    }
    impl BufRead for FailsAfter<'_> {
        fn fill_buf(&mut self) -> io::Result<&[u8]> {
            if self.0.is_empty() {
                Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "test read failure",
                ))
            } else {
                Ok(self.0)
            }
        }
        fn consume(&mut self, count: usize) {
            self.0 = &self.0[count..];
        }
    }

    for (prefix, start) in [
        (&b"A"[..], 1),
        (&b"A // "[..], 5),
        (&b"\""[..], 1),
        (&[b'/', b'/', 0xe5][..], 2),
    ] {
        let error = parse_lexemes(lexer::from_reader(FailsAfter(prefix))).unwrap_err();
        assert_eq!(error.cursor().offset, start);
        let ParseError::Lexical(LexError::Input(error)) = error else {
            panic!("expected an input error");
        };
        assert_eq!(error.start.offset, start);
        assert_eq!(error.cause.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(error.cause.to_string(), "test read failure");
    }
}

#[test]
fn diagnostics_locate_tokens_and_eof_without_retaining_the_source() {
    use std::io::BufReader;
    for (source, line, column) in [
        ("", 1, 1),
        ("A.\r\n\t", 2, 2),
        ("//幅😀\r\nA | \t幅", 2, 6),
        ("\"幅\nx", 1, 1),
        ("A\n\"text", 2, 1),
        ("//e\u{301}\t", 1, 6),
        ("//😀\r", 2, 1),
        ("//😀\r\n", 2, 1),
        ("//a\r\r\n", 3, 1),
    ] {
        let expected = (line, column);
        let error = {
            let owned_source = source.to_owned();
            parse_source(&owned_source).unwrap_err()
        };
        assert_eq!(
            (error.cursor().line, error.cursor().column),
            expected,
            "{source:?}"
        );
        if let ParseError::Unexpected { span, cursor, .. } = &error {
            assert_eq!(cursor.offset, span.start);
        }
        for capacity in 1..=4 {
            let reader = BufReader::with_capacity(capacity, source.as_bytes());
            let streamed = parse_lexemes(lexer::from_reader(reader)).unwrap_err();
            assert_eq!(
                (streamed.cursor().line, streamed.cursor().column),
                expected,
                "{source:?}"
            );
            assert_eq!(streamed.cursor(), error.cursor(), "{source:?}");
            assert_eq!(streamed.to_string(), error.to_string(), "{source:?}");
        }
    }
}

#[test]
fn trivia_retains_character_locations_including_delimiters() {
    let parsed = parse_source("\t//幅😀\r\n  T //end").unwrap();
    let locations: Vec<_> = parsed
        .trivia()
        .iter()
        .map(|trivia| (trivia.cursor.line, trivia.cursor.column))
        .collect();
    assert_eq!(locations, [(1, 1), (1, 2), (1, 6), (2, 4), (2, 5)]);
    for trivia in parsed.trivia() {
        assert_eq!(trivia.cursor.offset, trivia.span.start);
    }
}

#[test]
fn invalid_utf8_is_located_at_the_start_of_the_unread_character() {
    use std::io::BufReader;
    for suffix in [&[0xff][..], &[0xe5], &[0xe5, 0xb9], &[0xe5, b'A']] {
        let prefix = "A //幅😀\r\n//\t";
        let mut bytes = prefix.as_bytes().to_vec();
        bytes.extend_from_slice(suffix);
        let error = parse_lexemes(lexer::from_reader(BufReader::with_capacity(
            1,
            bytes.as_slice(),
        )))
        .unwrap_err();
        assert_eq!((error.cursor().line, error.cursor().column), (2, 4));
        assert_eq!(error.cursor().offset, prefix.len());
        assert!(matches!(error, ParseError::Lexical(LexError::Input(_))));
    }
}

fn union(items: Vec<Type>) -> Type {
    Type::Union(items.into_iter().map(AstNode::new).collect())
}

// These tests compare grammatical structure independently of source spelling.
// Span correctness is checked separately in source_locations.rs.
trait Structure {
    fn structure(&self) -> Type;
}

impl Structure for Parsed<AstNode<Type>> {
    fn structure(&self) -> Type {
        without_spans(&self.syntax().value)
    }
}

fn without_spans(ty: &Type) -> Type {
    let child = |node: &AstNode<Type>| AstNode::new(without_spans(&node.value));
    match ty {
        Type::Named { name, arguments } => {
            let mut parts = name
                .value
                .as_slice()
                .iter()
                .map(|node| AstNode::new(node.value.clone()));
            Type::Named {
                name: QualifiedName::new(parts.next().unwrap(), parts).into(),
                arguments: arguments.iter().map(child).collect(),
            }
        }
        Type::Array(inner) => Type::Array(Box::new(child(inner))),
        Type::Group(inner) => Type::Group(Box::new(child(inner))),
        Type::Union(items) => Type::Union(items.iter().map(child).collect()),
        Type::Function { parameters, result } => Type::Function {
            parameters: parameters.iter().map(child).collect(),
            result: Box::new(child(result)),
        },
    }
}
