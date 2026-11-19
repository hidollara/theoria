use aition_core::{
    lexer,
    syntax::{
        ParseError, TokenStream, Trivia, TriviaKind,
        ast::{AstNode, Binding, BindingBody, Expr, Type},
        parse,
    },
};

fn expression(source: &str) -> Result<(AstNode<Expr>, Vec<Trivia>), ParseError> {
    parse(TokenStream::new(lexer::from_str(source)))
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
        let (node, _) = expression(source).unwrap();
        let Expr::Array(items) = node.value else {
            panic!("expected array for {source:?}");
        };
        assert_eq!(items.len(), count);
        assert_eq!(node.span.unwrap().range(), 0..source.len());
        if let Some(first) = items.first() {
            assert_name(first, "x");
            assert_eq!(first.span.as_ref().unwrap().range(), 2..3);
        }
    }
    let (node, _) = expression("*(a, *(b, c),)").unwrap();
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
    let source = " // start\npkg.make(x,)(y).draw() // end";
    let (node, _) = expression(source).unwrap();
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
        assert_eq!(&source[node.span.as_ref().unwrap().range()], text);
    }
}

#[test]
fn explicit_expression_parentheses_are_retained_independently_of_calls() {
    let (node, _) = expression("((f))(x)").unwrap();
    let Expr::Call { callee, arguments } = node.value else {
        panic!("expected call")
    };
    assert_name(&arguments[0], "x");
    let Expr::Group(inner) = callee.value else {
        panic!("expected outer group")
    };
    let Expr::Group(name) = inner.value else {
        panic!("expected inner group")
    };
    assert_name(&name, "f");
    assert_eq!(callee.span.unwrap().range(), 0..5);
    assert_eq!(inner.span.unwrap().range(), 1..4);
    assert_eq!(name.span.unwrap().range(), 2..3);
}

#[test]
fn literals_decode_escapes_and_keep_only_explicit_type_annotations() {
    let source = r#""幅\n\r\t\"\\😀"@Text"#;
    let (node, _) = expression(source).unwrap();
    let Expr::Literal { text, ty: Some(ty) } = node.value else {
        panic!("expected typed literal")
    };
    assert_eq!(text.as_ref(), "幅\n\r\t\"\\😀");
    let Type::Named { name, arguments } = ty.value else {
        panic!("expected named type")
    };
    assert_eq!(name.value.as_slice()[0].value.as_str(), "Text");
    assert!(arguments.is_empty());
    assert_eq!(&source[ty.span.unwrap().range()], "Text");
    let (node, _) = expression(r#""{width}""#).unwrap();
    assert!(matches!(node.value, Expr::Literal { text, ty: None } if text.as_ref() == "{width}"));
}

#[test]
fn binding_declarations_and_definitions_use_separate_bodies() {
    let source = "items: panels.Picture<Image>*;";
    let (binding, _) =
        parse::<AstNode<Binding>>(TokenStream::new(lexer::from_str(source))).unwrap();
    assert_eq!(binding.value.name.value.as_str(), "items");
    assert_eq!(binding.span.unwrap().range(), 0..source.len());
    assert!(matches!(
        binding.value.body,
        BindingBody::Declaration(AstNode {
            value: Type::Array(_),
            ..
        })
    ));

    let (binding, _) =
        parse::<Binding>(TokenStream::new(lexer::from_str("items = *(a, f(b),);"))).unwrap();
    let BindingBody::Definition(node) = binding.body else {
        panic!("expected definition")
    };
    let Expr::Array(items) = node.value else {
        panic!("expected array")
    };
    assert_eq!(items.len(), 2);
    assert_name(&items[0], "a");
    assert!(matches!(items[1].value, Expr::Call { .. }));
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
        ("(a", 2, "')'"),
        ("()", 1, "an expression"),
        (r#""x"@"#, 4, "a type"),
        (r#""\q""#, 0, "a string with valid escapes"),
        ("a b", 2, "end of input"),
    ] {
        let error = expression(source).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{source:?}: {error:?}"
        );
    }
    for (source, offset, expected) in [
        ("x;", 1, "':' or '='"),
        ("x = ;", 4, "an expression"),
        ("x: ;", 3, "a type"),
        ("x = y", 5, "';'"),
    ] {
        let error = parse::<Binding>(TokenStream::new(lexer::from_str(source))).unwrap_err();
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
    let error = parse::<Expr>(TokenStream::new(lexer::from_reader(&b"*(x,\xff)"[..]))).unwrap_err();
    assert!(matches!(error, ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, 4);
}

#[test]
fn streamed_expressions_match_borrowed_input_and_own_their_text() {
    let source = " // 幅😀\r\n *(\"文字\", pkg.make(x,),) // end";
    let borrowed = expression(source).unwrap();
    for capacity in 1..=4 {
        let streamed = parse::<AstNode<Expr>>(TokenStream::new(lexer::from_reader(
            std::io::BufReader::with_capacity(capacity, source.as_bytes()),
        )))
        .unwrap();
        assert_eq!(streamed, borrowed);
    }
    let owned = {
        let temporary = source.to_owned();
        expression(&temporary).unwrap()
    };
    assert_eq!(owned, borrowed);
    let comments: Vec<_> = owned
        .1
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect();
    assert_eq!(comments, [" 幅😀", " end"]);
    assert_eq!(
        &source[owned.0.span.unwrap().range()],
        "*(\"文字\", pkg.make(x,),)"
    );
}
