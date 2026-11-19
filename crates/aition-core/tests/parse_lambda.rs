use aition_core::{
    lexer,
    syntax::{
        ParseError, TokenStream,
        ast::{AstNode, Binding, BindingBody, Body, Expr, Lambda, Parameter, Type},
        parse,
    },
};

fn expression(source: &str) -> AstNode<Expr> {
    parse(TokenStream::new(lexer::from_str(source))).unwrap().0
}

fn name(node: &AstNode<Expr>) -> &str {
    let Expr::Name(name) = &node.value else {
        panic!("expected name")
    };
    name.value.as_str()
}

#[test]
fn lambdas_keep_parameter_order_annotations_and_empty_parameter_lists() {
    for source in ["[x,y: Width*] (x)", "[x,y: Width*,](x)"] {
        let (lambda, _) =
            parse::<AstNode<Lambda>>(TokenStream::new(lexer::from_str(source))).unwrap();
        assert_eq!(lambda.span.unwrap().range(), 0..source.len());
        let parameters = &lambda.value.parameters;
        assert_eq!(parameters.len(), 2);
        assert_eq!(parameters[0].value.name.value.as_str(), "x");
        assert!(parameters[0].value.ty.is_none());
        assert_eq!(parameters[1].value.name.value.as_str(), "y");
        assert!(matches!(
            parameters[1].value.ty.as_ref().unwrap().value,
            Type::Array(_)
        ));
        assert_eq!(name(&lambda.value.body.value.result), "x");
        assert!(lambda.value.body.value.bindings.is_none());
    }
    let node = expression("[] (x)");
    let Expr::Lambda(lambda) = node.value else {
        panic!("expected lambda")
    };
    assert!(lambda.value.parameters.is_empty());
}

#[test]
fn parameter_type_is_optional_but_an_annotation_requires_a_type() {
    let (parameter, _) = parse::<Parameter>(TokenStream::new(lexer::from_str("width"))).unwrap();
    assert!(parameter.ty.is_none());
    let (parameter, _) =
        parse::<Parameter>(TokenStream::new(lexer::from_str("make: [Width] (Panel)"))).unwrap();
    assert!(matches!(parameter.ty.unwrap().value, Type::Function { .. }));
    let error = parse::<Parameter>(TokenStream::new(lexer::from_str("width:"))).unwrap_err();
    assert!(matches!(error, ParseError::Unexpected { expected, cursor }
        if expected == "a type" && cursor.offset == 6));
}

#[test]
fn array_and_lambda_prefixes_remain_distinct_when_nested() {
    let node = expression("*([x] (x), [] (*(a, b,)),)");
    let Expr::Array(items) = node.value else {
        panic!("expected array")
    };
    assert_eq!(items.len(), 2);
    let Expr::Lambda(first) = &items[0].value else {
        panic!("expected first lambda")
    };
    assert_eq!(name(&first.value.body.value.result), "x");
    let Expr::Lambda(second) = &items[1].value else {
        panic!("expected second lambda")
    };
    let Expr::Array(result) = &second.value.body.value.result.value else {
        panic!("expected array result")
    };
    assert_eq!(result.iter().map(name).collect::<Vec<_>>(), ["a", "b"]);
    assert!(parse::<Expr>(TokenStream::new(lexer::from_str("[x,y]"))).is_err());
}

#[test]
fn body_preserves_absent_empty_and_populated_helper_definitions() {
    let (absent, _) = parse::<Body>(TokenStream::new(lexer::from_str("(x)"))).unwrap();
    let (empty, _) = parse::<Body>(TokenStream::new(lexer::from_str("(x) {}"))).unwrap();
    assert!(absent.bindings.is_none());
    assert_eq!(empty.bindings, Some(vec![]));

    let source = "(result) { first = f(x); result = *(first,); }";
    let (body, _) = parse::<AstNode<Body>>(TokenStream::new(lexer::from_str(source))).unwrap();
    assert_eq!(name(&body.value.result), "result");
    let bindings = body.value.bindings.unwrap();
    assert_eq!(
        bindings
            .iter()
            .map(|node| node.value.name.value.as_str())
            .collect::<Vec<_>>(),
        ["first", "result"]
    );
    assert_eq!(
        &source[bindings[0].span.as_ref().unwrap().range()],
        "first = f(x);"
    );
    assert_eq!(
        &source[bindings[1].span.as_ref().unwrap().range()],
        "result = *(first,);"
    );
    assert_eq!(body.span.unwrap().range(), 0..source.len());
}

#[test]
fn nested_helpers_and_lambda_calls_keep_their_own_bodies() {
    let source = "make = [x] (result) { result = (inner) { inner = x; }; };";
    let (binding, _) = parse::<Binding>(TokenStream::new(lexer::from_str(source))).unwrap();
    let BindingBody::Definition(node) = binding.body else {
        panic!("expected definition")
    };
    let Expr::Lambda(lambda) = node.value else {
        panic!("expected lambda")
    };
    let helpers = lambda.value.body.value.bindings.unwrap();
    let BindingBody::Definition(inner) = &helpers[0].value.body else {
        panic!("expected helper definition")
    };
    let Expr::Body(body) = &inner.value else {
        panic!("expected nested body")
    };
    assert_eq!(name(&body.value.result), "inner");
    assert_eq!(
        body.value.bindings.as_ref().unwrap()[0]
            .value
            .name
            .value
            .as_str(),
        "inner"
    );

    let node = expression("([x] (x))(y)");
    let Expr::Call { callee, arguments } = node.value else {
        panic!("expected call")
    };
    assert_eq!(name(&arguments[0]), "y");
    let Expr::Body(body) = callee.value else {
        panic!("expected grouped callee")
    };
    assert!(matches!(body.value.result.value, Expr::Lambda(_)));
}

#[test]
fn helper_blocks_can_follow_non_lambda_expressions() {
    let node = expression("(value) { value = factory(); }.field");
    let Expr::Member { base, name } = node.value else {
        panic!("expected member")
    };
    assert_eq!(name.value.as_str(), "field");
    let Expr::Body(body) = base.value else {
        panic!("expected body")
    };
    assert_eq!(body.value.bindings.unwrap().len(), 1);
}

#[test]
fn malformed_lambdas_and_helpers_commit_at_their_opening_symbols() {
    for (source, offset, expected) in [
        ("[x]", 3, "'('"),
        ("[,] (x)", 1, "an identifier"),
        ("[x:] (x)", 3, "a type"),
        ("[x,,] (x)", 3, "an identifier"),
        ("[] ()", 4, "an expression"),
        ("[] (x", 5, "')'"),
        ("(x) {", 5, "'}'"),
        ("(x) { y = x }", 12, "';'"),
        ("(x) { y = ; }", 10, "an expression"),
        ("[] (x) { type T; }", 14, "':' or '='"),
    ] {
        let error = parse::<Expr>(TokenStream::new(lexer::from_str(source))).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{source:?}: {error:?}"
        );
    }
}

#[test]
fn lambda_spans_and_trivia_are_identical_for_borrowed_and_streamed_input() {
    let source = "// 幅😀\r\n [x: Width,] (out) { // helper\r\n out = *(x,); } // end";
    let expected = parse::<AstNode<Lambda>>(TokenStream::new(lexer::from_str(source))).unwrap();
    for capacity in 1..=4 {
        let actual = parse::<AstNode<Lambda>>(TokenStream::new(lexer::from_reader(
            std::io::BufReader::with_capacity(capacity, source.as_bytes()),
        )))
        .unwrap();
        assert_eq!(actual, expected);
    }
    let lambda = expected.0;
    let parameter = &lambda.value.parameters[0];
    assert_eq!(
        &source[parameter.span.as_ref().unwrap().range()],
        "x: Width"
    );
    assert_eq!(
        &source[lambda.value.body.span.as_ref().unwrap().range()],
        "(out) { // helper\r\n out = *(x,); }"
    );
}
