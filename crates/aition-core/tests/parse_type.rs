use aition_core::{
    lexer::{self, LexError, LexemeIterator},
    syntax::{
        ParseError, TokenStream, Trivia, TriviaKind,
        ast::{AstNode, Identifier, QualifiedName, Type},
        parse,
    },
};

fn parse_source(source: &str) -> Result<(AstNode<Type>, Vec<Trivia>), ParseError> {
    parse_lexemes(lexer::from_str(source))
}

fn parse_lexemes<'source>(
    lexemes: impl LexemeIterator<'source>,
) -> Result<(AstNode<Type>, Vec<Trivia>), ParseError> {
    parse::<AstNode<Type>>(TokenStream::new(lexemes))
}

fn assert_unexpected(error: ParseError, offset: usize, expected: &str) {
    match error {
        ParseError::Unexpected {
            expected: actual_expected,
            cursor,
            ..
        } => {
            assert_eq!(cursor.offset, offset);
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
fn validates_identifier_spelling_after_lexing_a_whole_word() {
    for name in ["foo幅", "123abc", "幅", "aé", "foo-bar", "a#b"] {
        for prefix in ["", "module.", "T<", "A | "] {
            let source = format!("{prefix}{name}");
            assert_unexpected(
                parse_source(&source).unwrap_err(),
                prefix.len(),
                "an identifier",
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
    for (source, offset, expected) in [
        ("", 0, "a type"),
        ("A |", 3, "a type"),
        ("A..B", 2, "an identifier"),
        ("A.", 2, "an identifier"),
        ("A<>", 2, "a type"),
        ("A<", 2, "a type"),
        ("A<,>", 2, "a type"),
        ("A<B,", 4, "a type"),
        ("A<B C>", 4, "'>'"),
        ("A<B]", 3, "'>'"),
        ("A<B", 3, "'>'"),
        ("()", 1, "a type"),
        ("[A,]", 4, "'('"),
        ("[A,,](B)", 3, "a type"),
        ("[,](B)", 1, "a type"),
        ("[A,", 3, "a type"),
        ("[A B](C)", 3, "']'"),
        ("[A](B,)", 5, "')'"),
        ("[A](B", 5, "')'"),
        ("A Extra", 2, "end of input"),
    ] {
        assert_unexpected(parse_source(source).unwrap_err(), offset, expected);
    }
}

#[test]
fn error_cursors_keep_utf8_byte_offsets_and_line_columns() {
    let source = "\r\nA | 幅";
    let error = parse_source(source).unwrap_err();
    let ParseError::Unexpected { cursor, .. } = error else {
        panic!("expected an unexpected token, got {error:?}");
    };
    assert_eq!(cursor.offset, 6);
    assert_eq!(&source[cursor.offset..], "幅");
    assert_eq!(
        error.to_string(),
        "expected an identifier at line 2, column 5"
    );

    let source = "A.\r\n  ";
    let error = parse_source(source).unwrap_err();
    assert_unexpected(error, source.len(), "an identifier");
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
        .1
        .iter()
        .map(|trivia| (trivia.kind.clone(), &source[trivia.span().range()]))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(parsed.1[0].span().range(), 0..6);
    assert!(
        parsed
            .1
            .windows(2)
            .all(|pair| pair[0].end.offset <= pair[1].start.offset)
    );
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
    assert_unexpected(error, 0, "a type");
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

fn union(items: Vec<Type>) -> Type {
    Type::Union(items.into_iter().map(AstNode::new).collect())
}

// These tests compare grammatical structure independently of source spelling.
// Span correctness is checked separately in source_locations.rs.
trait Structure {
    fn structure(&self) -> Type;
}

impl Structure for (AstNode<Type>, Vec<Trivia>) {
    fn structure(&self) -> Type {
        without_spans(&self.0.value)
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
