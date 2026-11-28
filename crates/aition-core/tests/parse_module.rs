use aition_core::{
    input::Cursor,
    ito::{LexError, ParseError, parse, parse_from_str},
    syntax::{
        BindingBody, Body, ContractRef, Expr, Identifier, Lambda, Module, ModuleItem, NamedType,
        Parameter, QualifiedName, Tag, TagArgument, Type, TypeBody, TypeMember, TypeParameter,
    },
};
use std::io::{BufReader, ErrorKind};

struct Source<'a>(&'a str);

impl Source<'_> {
    fn cursor(&self, offset: usize) -> Cursor {
        Cursor::default().advanced(self.0[..offset].chars())
    }

    fn id(&self, offset: usize, name: &str) -> Identifier {
        Identifier::new(name)
            .unwrap()
            .with_cursor(self.cursor(offset))
    }

    fn name(&self, offset: usize, name: &str) -> Expr {
        Expr::Name(self.id(offset, name))
    }

    fn assert_items(&self, items: Vec<ModuleItem>) {
        let expected = Module { items };
        assert_eq!(parse_from_str(self.0).unwrap(), expected);
        let reader = BufReader::with_capacity(1, self.0.as_bytes());
        assert_eq!(parse(reader).unwrap(), expected);
    }

    fn qualified_name(&self, offset: usize, spelling: &str) -> QualifiedName {
        let mut parts = spelling.split('.');
        let head = parts.next().unwrap();
        let mut next = offset + head.len() + 1;
        let tail = parts.map(|part| {
            let id = self.id(next, part);
            next += part.len() + 1;
            id
        });
        QualifiedName::new(self.id(offset, head), tail)
    }

    fn ty(&self, offset: usize, spelling: &str) -> Type {
        Type::Named(NamedType {
            name: self.qualified_name(offset, spelling),
            arguments: None,
        })
    }

    fn applied(&self, offset: usize, spelling: &str, arguments: Vec<Type>) -> Type {
        Type::Named(NamedType {
            name: self.qualified_name(offset, spelling),
            arguments: Some(arguments),
        })
    }
}

#[test]
fn empty_input_returns_an_empty_module() {
    Source("").assert_items(vec![]);
}

#[test]
fn aliases_preserve_local_names_and_qualified_targets() {
    let source =
        Source("module ui { alias Card = panels.Panel }\nalias Controls = ui\ncard = <ui.Card/>");
    source.assert_items(vec![
        ModuleItem::Module {
            name: source.id(7, "ui"),
            body: Module {
                items: vec![ModuleItem::Alias {
                    name: source.id(18, "Card"),
                    target: source.qualified_name(25, "panels.Panel"),
                }],
            },
        },
        ModuleItem::Alias {
            name: source.id(46, "Controls"),
            target: source.qualified_name(57, "ui"),
        },
        ModuleItem::Binding {
            name: source.id(60, "card"),
            body: BindingBody::Definition(Expr::Tag(Tag {
                name: source.qualified_name(68, "ui.Card"),
                arguments: vec![],
                children: None,
            })),
        },
    ]);
}

#[test]
fn aliases_require_an_equal_sign_before_the_target() {
    let source = Source("alias Card panels.Panel");
    let error = parse_from_str(source.0).unwrap_err();
    assert!(matches!(
        error,
        ParseError::Unexpected { expected, cursor }
            if expected == "=" && cursor == source.cursor(11)
    ));
}

#[test]
fn inline_modules_preserve_nesting_and_leave_following_items_at_root() {
    let source = Source(
        "module ui { module controls { type Panel: {} } }\nmodule empty {}\npanel = <ui.controls.Panel/>",
    );
    source.assert_items(vec![
        ModuleItem::Module {
            name: source.id(7, "ui"),
            body: Module {
                items: vec![ModuleItem::Module {
                    name: source.id(19, "controls"),
                    body: Module {
                        items: vec![ModuleItem::Type {
                            name: source.id(35, "Panel"),
                            parameters: None,
                            body: TypeBody::Declaration {
                                constructor: None,
                                members: vec![],
                            },
                        }],
                    },
                }],
            },
        },
        ModuleItem::Module {
            name: source.id(56, "empty"),
            body: Module { items: vec![] },
        },
        ModuleItem::Binding {
            name: source.id(65, "panel"),
            body: BindingBody::Definition(Expr::Tag(Tag {
                name: source.qualified_name(74, "ui.controls.Panel"),
                arguments: vec![],
                children: None,
            })),
        },
    ]);
}

#[test]
fn type_definitions_keep_constructor_parameters_and_local_bindings() {
    let source = Source(
        "type Panel<T> = [width: Width,] {\n    base = compute(width)\n    drawing = render(base)\n}\ntype Br = [] {}",
    );
    source.assert_items(vec![
        ModuleItem::Type {
            name: source.id(5, "Panel"),
            parameters: Some(vec![TypeParameter {
                name: source.id(11, "T"),
                bounds: vec![],
            }]),
            body: TypeBody::Definition {
                constructor: vec![Parameter {
                    name: source.id(17, "width"),
                    ty: Some(source.ty(24, "Width")),
                }],
                bindings: vec![
                    (
                        source.id(38, "base"),
                        Expr::Call {
                            callee: Box::new(source.name(45, "compute")),
                            arguments: vec![source.name(53, "width")],
                        },
                    ),
                    (
                        source.id(64, "drawing"),
                        Expr::Call {
                            callee: Box::new(source.name(74, "render")),
                            arguments: vec![source.name(81, "base")],
                        },
                    ),
                ],
            },
        },
        ModuleItem::Type {
            name: source.id(94, "Br"),
            parameters: None,
            body: TypeBody::Definition {
                constructor: vec![],
                bindings: vec![],
            },
        },
    ]);
}

#[test]
fn function_types_can_appear_in_constructors_and_members() {
    let source = Source(
        "type Pipeline: [[Input, Context,](Output | Failure)] { next: []([Output](Result)) }",
    );
    source.assert_items(vec![ModuleItem::Type {
        name: source.id(5, "Pipeline"),
        parameters: None,
        body: TypeBody::Declaration {
            constructor: Some(vec![Type::Function {
                parameters: vec![source.ty(17, "Input"), source.ty(24, "Context")],
                result: Box::new(Type::Union(vec![
                    source.ty(34, "Output"),
                    source.ty(43, "Failure"),
                ])),
            }]),
            members: vec![TypeMember {
                name: source.id(55, "next"),
                ty: Type::Function {
                    parameters: vec![],
                    result: Box::new(Type::Function {
                        parameters: vec![source.ty(65, "Output")],
                        result: Box::new(source.ty(73, "Result")),
                    }),
                },
            }],
        },
    }]);
}

#[test]
fn binding_declarations_and_definitions_keep_module_order() {
    let source = Source("type A: {}\nmake: [A](A)\nempty: [](A)\nvalue = make");
    source.assert_items(vec![
        ModuleItem::Type {
            name: source.id(5, "A"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: None,
                members: vec![],
            },
        },
        ModuleItem::Binding {
            name: source.id(11, "make"),
            body: BindingBody::Declaration(Type::Function {
                parameters: vec![source.ty(18, "A")],
                result: Box::new(source.ty(21, "A")),
            }),
        },
        ModuleItem::Binding {
            name: source.id(24, "empty"),
            body: BindingBody::Declaration(Type::Function {
                parameters: vec![],
                result: Box::new(source.ty(34, "A")),
            }),
        },
        ModuleItem::Binding {
            name: source.id(37, "value"),
            body: BindingBody::Definition(source.name(45, "make")),
        },
    ]);
}

#[test]
fn type_declarations_keep_their_names_and_source_order() {
    let s = Source("// file\ntype// name\r\nCanvas// signature\n:{} type _Width2:{}");
    s.assert_items(vec![
        ModuleItem::Type {
            name: s.id(21, "Canvas"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: None,
                members: vec![],
            },
        },
        ModuleItem::Type {
            name: s.id(49, "_Width2"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: None,
                members: vec![],
            },
        },
    ]);
}

#[test]
fn constructor_and_member_lists_keep_their_order_and_source() {
    let s = Source(concat!(
        "type Panel: [ui.Width, Height,] // members\n",
        "{ width: ui.layout.Width, height: Height, } type Br: [] {} // next\n",
        "type Size: {}",
    ));
    s.assert_items(vec![
        ModuleItem::Type {
            name: s.id(5, "Panel"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: Some(vec![s.ty(13, "ui.Width"), s.ty(23, "Height")]),
                members: vec![
                    TypeMember {
                        name: s.id(45, "width"),
                        ty: s.ty(52, "ui.layout.Width"),
                    },
                    TypeMember {
                        name: s.id(69, "height"),
                        ty: s.ty(77, "Height"),
                    },
                ],
            },
        },
        ModuleItem::Type {
            name: s.id(92, "Br"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: Some(vec![]),
                members: vec![],
            },
        },
        ModuleItem::Type {
            name: s.id(115, "Size"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: None,
                members: vec![],
            },
        },
    ]);
}

#[test]
fn explicit_empty_type_parameters_and_arguments_are_preserved() {
    let s = Source("type A<>: [A<>, A] {}");
    s.assert_items(vec![ModuleItem::Type {
        name: s.id(5, "A"),
        parameters: Some(vec![]),
        body: TypeBody::Declaration {
            constructor: Some(vec![s.applied(11, "A", vec![]), s.ty(16, "A")]),
            members: vec![],
        },
    }]);
}

#[test]
fn type_parameters_keep_bounds_and_type_arguments_in_order() {
    let s =
        Source("type Pair<T: ui.draw & eq<U>, U,>: [T, U] { values: ui.List<Pair<*T | U, U>> }");
    let array = Type::Array(Box::new(s.ty(66, "T")));
    let union = Type::Union(vec![array, s.ty(70, "U")]);
    let pair = s.applied(60, "Pair", vec![union, s.ty(73, "U")]);

    s.assert_items(vec![ModuleItem::Type {
        name: s.id(5, "Pair"),
        parameters: Some(vec![
            TypeParameter {
                name: s.id(10, "T"),
                bounds: vec![
                    ContractRef {
                        name: s.qualified_name(13, "ui.draw"),
                        arguments: None,
                    },
                    ContractRef {
                        name: s.qualified_name(23, "eq"),
                        arguments: Some(vec![s.ty(26, "U")]),
                    },
                ],
            },
            TypeParameter {
                name: s.id(30, "U"),
                bounds: vec![],
            },
        ]),
        body: TypeBody::Declaration {
            constructor: Some(vec![s.ty(36, "T"), s.ty(39, "U")]),
            members: vec![TypeMember {
                name: s.id(44, "values"),
                ty: s.applied(52, "ui.List", vec![pair]),
            }],
        },
    }]);
}

#[test]
fn union_types_keep_their_alternatives_in_constructor_and_member_positions() {
    let s = Source("type Panel: [A|ui.B|A, C] { width: D | E }");
    s.assert_items(vec![ModuleItem::Type {
        name: s.id(5, "Panel"),
        parameters: None,
        body: TypeBody::Declaration {
            constructor: Some(vec![
                Type::Union(vec![s.ty(13, "A"), s.ty(15, "ui.B"), s.ty(20, "A")]),
                s.ty(23, "C"),
            ]),
            members: vec![TypeMember {
                name: s.id(28, "width"),
                ty: Type::Union(vec![s.ty(35, "D"), s.ty(39, "E")]),
            }],
        },
    }]);
}

#[test]
fn array_prefix_binds_more_tightly_than_union_unless_grouped() {
    let s = Source("type Layer: [*A | B] { children: **(A | ui.B) }");
    let array = Type::Array(Box::new(s.ty(14, "A")));
    let union = Type::Union(vec![s.ty(36, "A"), s.ty(40, "ui.B")]);
    let group = Type::Group(Box::new(union));
    let inner = Type::Array(Box::new(group));

    s.assert_items(vec![ModuleItem::Type {
        name: s.id(5, "Layer"),
        parameters: None,
        body: TypeBody::Declaration {
            constructor: Some(vec![Type::Union(vec![array, s.ty(18, "B")])]),
            members: vec![TypeMember {
                name: s.id(23, "children"),
                ty: Type::Array(Box::new(inner)),
            }],
        },
    }]);
}

fn member(base: Expr, name: Identifier) -> Expr {
    Expr::Member {
        base: Box::new(base),
        name,
    }
}

fn call(callee: Expr, arguments: Vec<Expr>) -> Expr {
    Expr::Call {
        callee: Box::new(callee),
        arguments,
    }
}

#[test]
fn a_binding_chains_calls_and_members_in_source_order() {
    let s = Source(
        "type Canvas: {} drawing = panel.paint(make(width), height,).canvas() copy = drawing",
    );
    let paint = member(s.name(26, "panel"), s.id(32, "paint"));
    let width = call(s.name(38, "make"), vec![s.name(43, "width")]);
    let painted = call(paint, vec![width, s.name(51, "height")]);
    let canvas = member(painted, s.id(60, "canvas"));

    s.assert_items(vec![
        ModuleItem::Type {
            name: s.id(5, "Canvas"),
            parameters: None,
            body: TypeBody::Declaration {
                constructor: None,
                members: vec![],
            },
        },
        ModuleItem::Binding {
            name: s.id(16, "drawing"),
            body: BindingBody::Definition(call(canvas, vec![])),
        },
        ModuleItem::Binding {
            name: s.id(69, "copy"),
            body: BindingBody::Definition(s.name(76, "drawing")),
        },
    ]);
}

#[test]
fn unfinished_expressions_report_the_missing_part() {
    let expression_start = "an identifier, a string literal, (, [, *, <, or ?";
    for (text, expected, offset) in [
        ("x =", expression_start, 3),
        ("x = f.", "an identifier", 6),
        ("x = f(a", ")", 7),
        ("x = f(,)", expression_start, 6),
        ("x = f(a,,b)", expression_start, 8),
        ("x = f():", "*, (, [, or an identifier", 8),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(&error, ParseError::Unexpected { expected: actual, cursor }
            if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn branching_keeps_lambdas_local_bindings_and_annotated_literals() {
    let source = Source(r#"x = ?(a,b)[[p: Panel, q] (out) { out = *(p, q, "幅\n":Text,) },]"#);
    source.assert_items(vec![ModuleItem::Binding {
        name: source.id(0, "x"),
        body: BindingBody::Definition(Expr::Match {
            arguments: vec![source.name(6, "a"), source.name(8, "b")],
            branches: vec![Lambda {
                parameters: vec![
                    Parameter {
                        name: source.id(12, "p"),
                        ty: Some(source.ty(15, "Panel")),
                    },
                    Parameter {
                        name: source.id(22, "q"),
                        ty: None,
                    },
                ],
                body: Body {
                    result: Box::new(source.name(26, "out")),
                    bindings: Some(vec![(
                        source.id(33, "out"),
                        Expr::Array(vec![
                            source.name(41, "p"),
                            source.name(44, "q"),
                            Expr::Ascribed {
                                expr: Box::new(Expr::Literal("幅\n".into())),
                                ty: source.ty(source.0.find("Text").unwrap(), "Text"),
                            },
                        ]),
                    )]),
                },
            }],
        }),
    }]);
}

#[test]
fn tag_children_keep_adjacent_bodies_separate() {
    let source = Source(r#"x = <ui.Row width=("1":Width)>(a)(b)<Leaf/>"text"</ui.Row>"#);
    source.assert_items(vec![ModuleItem::Binding {
        name: source.id(0, "x"),
        body: BindingBody::Definition(Expr::Tag(Tag {
            name: source.qualified_name(5, "ui.Row"),
            arguments: vec![TagArgument {
                name: source.id(12, "width"),
                value: Expr::Body(Body {
                    result: Box::new(Expr::Ascribed {
                        expr: Box::new(Expr::Literal("1".into())),
                        ty: source.ty(23, "Width"),
                    }),
                    bindings: None,
                }),
            }],
            children: Some(vec![
                Expr::Body(Body {
                    result: Box::new(source.name(31, "a")),
                    bindings: None,
                }),
                Expr::Body(Body {
                    result: Box::new(source.name(34, "b")),
                    bindings: None,
                }),
                Expr::Tag(Tag {
                    name: source.qualified_name(37, "Leaf"),
                    arguments: vec![],
                    children: None,
                }),
                Expr::Literal("text".into()),
            ]),
        })),
    }]);
}

#[test]
fn a_type_annotation_applies_to_the_call_before_member_access() {
    let source = Source("x = (f(): A | B).member");
    source.assert_items(vec![ModuleItem::Binding {
        name: source.id(0, "x"),
        body: BindingBody::Definition(Expr::Member {
            base: Box::new(Expr::Body(Body {
                result: Box::new(Expr::Ascribed {
                    expr: Box::new(Expr::Call {
                        callee: Box::new(source.name(5, "f")),
                        arguments: vec![],
                    }),
                    ty: Type::Union(vec![source.ty(10, "A"), source.ty(14, "B")]),
                }),
                bindings: None,
            })),
            name: source.id(17, "member"),
        }),
    }]);
}

#[test]
fn malformed_tags_and_invalid_escapes_report_their_positions() {
    for (text, expected, offset) in [
        ("x = <Row></Leaf>", "closing tag </Row>", 11),
        ("x = < Row/>", "an adjacent token", 5),
        ("x = <Row/ >", "an adjacent token", 9),
        ("x = <Row>< /Row>", "an adjacent token", 10),
        (r#"x = <P width="1":Width/>"#, "/ or >", 16),
        (r#"x = "幅\q""#, "a valid string escape", 8),
    ] {
        assert!(matches!(
            parse_from_str(text),
            Err(ParseError::Unexpected { expected: actual, cursor })
                if actual == expected && cursor == Source(text).cursor(offset)
        ));
    }
}

#[test]
fn malformed_signatures_report_the_missing_part() {
    let type_start = "*, (, [, or an identifier";
    for (text, expected, offset) in [
        ("type Br", ": or =", 7),
        ("type Br = {}", "[", 10),
        ("type Br = [] { x: A }", "=", 16),
        ("type Br<,>", "an identifier", 8),
        ("type Br<T U>", ">", 10),
        ("type Br<T:", "an identifier", 10),
        ("type Br<T: C &>", "an identifier", 14),
        ("type Br: [X<,>]", type_start, 12),
        ("type Br: [X<A,,>]", type_start, 14),
        ("type Br: [X<A]", ">", 13),
        ("type Br:", "{", 8),
        ("type Br: []", "{", 11),
        ("type Br: [] type Other: {}", "{", 12),
        ("type Br: [,]", type_start, 10),
        ("type Br: [A B]", "]", 12),
        ("type Br: [A", "]", 11),
        ("type Br: [|A]", type_start, 10),
        ("type Br: [A|, B]", type_start, 12),
        ("type Br: [A||B]", type_start, 12),
        ("type Br: [*]", type_start, 11),
        ("type Br: [()]", type_start, 11),
        ("type Br: [(A|)]", type_start, 13),
        ("type Br: [*(A]", ")", 13),
        ("type Br: [ui.]", "an identifier", 13),
        ("type Br: [ui..A]", "an identifier", 13),
        ("type Br: { x }", ":", 13),
        ("type Br: { x: }", type_start, 14),
        ("type Br: { x: A | }", type_start, 18),
        ("type Br: { x: ui. }", "an identifier", 18),
        ("type Br: { x: A", "}", 15),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor, .. }
            if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn malformed_type_declarations_report_the_expected_token_at_its_position() {
    for (text, expected, offset) in [
        ("typeName", ": or =", 8),
        ("type )", "an identifier", 5),
        ("type ", "an identifier", 5),
        ("type A:{} type", "an identifier", 14),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(error, ParseError::Unexpected { expected: ref actual, cursor, .. }
            if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn invalid_identifiers_report_the_violated_rule_at_the_word_start() {
    let start = "an identifier starting with an ASCII letter or '_'";
    let spelling = "an identifier containing only ASCII letters, digits, or '_'";
    for (text, expected, offset) in [
        ("type 2D", start, 5),
        ("type A: {} type 9幅", start, 16),
        ("type A;", spelling, 5),
        ("type A-B", spelling, 5),
        ("type A: [ui.B幅] {}", spelling, 12),
    ] {
        let error = parse_from_str(text).unwrap_err();
        assert!(
            matches!(&error, ParseError::Unexpected { expected: actual, cursor }
                if actual == expected && cursor.offset == offset),
            "{text:?}: {error}"
        );
    }
}

#[test]
fn comments_and_whitespace_form_an_empty_module() {
    Source("// 幅\r\n ").assert_items(vec![]);
}

#[test]
fn both_parse_apis_report_the_closing_paren_after_trivia() {
    let text = "// 幅\r\n )";
    let closing_paren = text.find(')').unwrap();

    let text_error = parse_from_str(text).unwrap_err();
    assert!(
        matches!(text_error, ParseError::Unexpected { expected, cursor } if expected == "end of input" && cursor.offset == closing_paren)
    );

    let reader_error = parse(text.as_bytes()).unwrap_err();
    assert!(
        matches!(reader_error, ParseError::Unexpected { expected, cursor } if expected == "end of input" && cursor.offset == closing_paren)
    );
}

#[test]
fn invalid_utf8_reports_the_reader_position() {
    let error = parse(&b"\n\xff"[..]).unwrap_err();
    let ParseError::Lexical(LexError::Input { cursor, cause }) = error else {
        panic!("expected an input error");
    };
    assert_eq!(cause.kind(), ErrorKind::InvalidData);
    assert_eq!(cursor.offset, 1);
}
#[test]
fn invalid_utf8_after_trivia_is_reported_at_the_failed_read() {
    let input = &b"// note\r\nword\xff"[..];
    let error = aition_core::ito::parse(input).unwrap_err();
    assert!(matches!(error, aition_core::ito::ParseError::Lexical(_)));
    assert_eq!(error.cursor().offset, input.len() - 1);
}
