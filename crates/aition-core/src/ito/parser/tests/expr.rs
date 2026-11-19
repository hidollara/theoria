use super::super::expr::expr;
use super::{node_bytes, node_source, read_complete};
use crate::{
    ito::{
        ParseError,
        lexer::{self, TokenStream},
    },
    syntax::ast::{AstNode, Expr, Trivia, TriviaKind, Type},
};

fn expression(source: &str) -> Result<AstNode<Expr>, ParseError> {
    read_complete(lexer::from_str(source), expr())
}

fn assert_name(node: &AstNode<Expr>, expected: &str) {
    let Expr::Name(name) = &node.value else {
        panic!("expected name {expected:?}, got {:?}", node.value);
    };
    assert_eq!(name.value.as_str(), expected);
}

#[test]
fn arrays_use_asterisk_parentheses_and_allow_empty_or_trailing_comma() {
    for (source, count) in [("*()", 0), ("*(x)", 1), ("*(x,)", 1), ("*(x,y,)", 2)] {
        let node = expression(source).unwrap();
        let Expr::Array(items) = &node.value else {
            panic!("expected array for {source:?}");
        };
        assert_eq!(items.len(), count);
        assert_eq!(node_bytes(&node), 0..source.len());
        if let Some(first) = items.first() {
            assert_name(first, "x");
            assert_eq!(node_bytes(first), 2..3);
        }
    }
    let node = expression("*(a, *(b, c),)").unwrap();
    let Expr::Array(items) = node.value else {
        panic!("expected array")
    };
    assert_name(&items[0], "a");
    let Expr::Array(nested) = &items[1].value else {
        panic!("expected nested array")
    };
    assert_name(&nested[0], "b");
    assert_name(&nested[1], "c");
}

#[test]
fn member_access_and_calls_keep_each_intermediate_expression_range() {
    let source = "pkg.make(x,)(y).draw() // end";
    let node = expression(source).unwrap();
    let Expr::Call {
        callee: draw,
        arguments,
    } = node.value
    else {
        panic!("expected final call")
    };
    assert!(arguments.is_empty());
    let Expr::Member {
        base: second_call,
        name,
    } = &draw.value
    else {
        panic!("expected member")
    };
    assert_eq!(name.value.as_str(), "draw");
    let Expr::Call {
        callee: first_call,
        arguments,
    } = &second_call.value
    else {
        panic!("expected second call")
    };
    assert_name(&arguments[0], "y");
    let Expr::Call {
        callee: make,
        arguments,
    } = &first_call.value
    else {
        panic!("expected first call")
    };
    assert_name(&arguments[0], "x");
    let Expr::Member { base: pkg, name } = &make.value else {
        panic!("expected module member")
    };
    assert_eq!(name.value.as_str(), "make");
    assert_name(pkg, "pkg");
    for (node, text) in [
        (pkg.as_ref(), "pkg"),
        (make.as_ref(), "pkg.make"),
        (first_call.as_ref(), "pkg.make(x,)"),
        (second_call.as_ref(), "pkg.make(x,)(y)"),
        (draw.as_ref(), "pkg.make(x,)(y).draw"),
    ] {
        assert_eq!(&source[node_bytes(node)], text);
    }
}

#[test]
fn call_and_member_spans_end_at_their_last_token_in_a_sparse_stream() {
    let source = "f()   .name    ";
    struct WithoutWhitespace<S>(S);
    impl<'s, S: TokenStream<'s>> TokenStream<'s> for WithoutWhitespace<S> {
        fn next(&mut self) -> Result<lexer::TokenStreamItem<'s>, lexer::LexError> {
            loop {
                match self.0.next()? {
                    lexer::TokenStreamItem::Token(token)
                        if matches!(token.kind, lexer::TokenKind::Whitespace(_)) =>
                    {
                        continue;
                    }
                    item => return Ok(item),
                }
            }
        }
    }
    let member = read_complete(WithoutWhitespace(lexer::from_str(source)), expr()).unwrap();
    let Expr::Member { base: call, name } = &member.value else {
        panic!("expected member access")
    };
    assert_eq!(&source[node_bytes(call)], "f()");
    assert_eq!(&source[node_bytes(name)], "name");
    assert_eq!(&source[node_bytes(&member)], "f()   .name");
}

#[test]
fn folded_nodes_include_internal_trivia_without_extending_the_previous_node() {
    let source = "f // base\n (x) // call\n . value // end";
    let node = expression(source).unwrap();
    assert_eq!(
        &source[node_bytes(&node)],
        "f // base\n (x) // call\n . value"
    );
    let Expr::Member { base: call, name } = &node.value else {
        panic!("expected member access")
    };
    assert_eq!(name.value.as_str(), "value");
    assert_eq!(&source[node_bytes(call)], "f // base\n (x)");
    let Expr::Call { callee, arguments } = &call.value else {
        panic!("expected call")
    };
    assert_eq!(&source[node_bytes(callee)], "f");
    assert_eq!(&source[node_bytes(&arguments[0])], "x");
    fn comments(trivia: &[Trivia]) -> Vec<&str> {
        trivia
            .iter()
            .filter_map(|item| match &item.kind {
                TriviaKind::LineComment(text) => Some(text.as_ref()),
                _ => None,
            })
            .collect()
    }
    assert_eq!(comments(&node_source(&node).trivia), [" call", " end"]);
    assert_eq!(comments(&node_source(call).trivia), [" base"]);
    assert!(node_source(callee).trivia.is_empty());
}

#[test]
fn literals_decode_escapes_and_keep_only_explicit_type_annotations() {
    let source = r#""幅\n\r\t\"\\\u{1F600}"@Text"#;
    let node = expression(source).unwrap();
    let Expr::Literal { text, ty: Some(ty) } = node.value else {
        panic!("expected typed literal")
    };
    assert_eq!(text.as_ref(), "幅\n\r\t\"\\😀");
    let Type::Named { name, arguments } = &ty.value else {
        panic!("expected named type")
    };
    assert_eq!(name.value.as_slice()[0].value.as_str(), "Text");
    assert!(arguments.is_empty());
    assert_eq!(&source[node_bytes(&ty)], "Text");
    let node = expression(r#""{width}""#).unwrap();
    assert!(matches!(node.value, Expr::Literal { text, ty: None } if text.as_ref() == "{width}"));
}

#[test]
fn type_annotations_use_prefix_arrays_inside_calls() {
    let source = r#"consume("items"@*T<A | B>, next)"#;
    let node = expression(source).unwrap();
    let Expr::Call { arguments, .. } = node.value else {
        panic!("expected call")
    };
    assert_eq!(arguments.len(), 2);
    let Expr::Literal { ty: Some(ty), .. } = &arguments[0].value else {
        panic!("expected typed literal")
    };
    assert_eq!(&source[node_bytes(ty)], "*T<A | B>");
    assert!(matches!(ty.value, Type::Array(_)));
    assert_name(&arguments[1], "next");
}

#[test]
fn malformed_expressions_report_the_required_part_without_falling_back() {
    for (source, offset, expected) in [
        ("", 0, "an expression"),
        ("*", 1, "'('"),
        ("*(", 2, "an expression"),
        ("*(,)", 2, "an expression"),
        ("*(x,,)", 4, "an expression"),
        ("f(,)", 2, "an expression"),
        ("f(x", 3, "')'"),
        ("a.", 2, "an identifier"),
        ("a..b", 2, "an identifier"),
        (r#""x"@"#, 4, "a type"),
        (r#""\q""#, 2, "a valid escape character"),
        (r#"f("\u{z}")"#, 6, "a hexadecimal digit"),
        ("a b", 2, "end of input"),
    ] {
        let error = expression(source).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{source:?}: {error:?}"
        );
    }
}

#[test]
fn lexical_errors_propagate_from_nested_expressions_and_trailing_input() {
    for source in [
        r#"*("unfinished"#,
        r#"f(a, "unfinished"#,
        r#"a "unfinished"#,
    ] {
        assert!(
            matches!(expression(source), Err(ParseError::Lexical(_))),
            "{source:?}"
        );
    }
    let error = read_complete(lexer::from_reader(&b"*(x,\xff)"[..]), expr()).unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, 4);
}

#[test]
fn streamed_expressions_match_borrowed_input_and_own_their_text() {
    let source = "*( // 幅😀\r\n \"文字\\u{1F600}\", pkg.make(x,),) // end";
    let borrowed = expression(source).unwrap();
    for capacity in 1..=4 {
        let streamed = read_complete(
            lexer::from_reader(std::io::BufReader::with_capacity(
                capacity,
                source.as_bytes(),
            )),
            expr(),
        )
        .unwrap();
        assert_eq!(streamed, borrowed);
    }
    let owned = {
        let temporary = source.to_owned();
        expression(&temporary).unwrap()
    };
    assert_eq!(owned, borrowed);
    let comments: Vec<_> = node_source(&owned)
        .trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" 幅😀", " end"]);
    assert_eq!(
        &source[node_bytes(&owned)],
        "*( // 幅😀\r\n \"文字\\u{1F600}\", pkg.make(x,),)"
    );
}
