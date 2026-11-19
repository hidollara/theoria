use super::super::expr::expr;
use super::super::types::type_expr;
use super::{node_bytes, node_source, read_complete};
use crate::{
    ito::lexer::{self, TokenKind, TokenStream, TokenStreamItem},
    syntax::ast::{AstNode, Expr, QualifiedName, Trivia, TriviaKind, Type},
};

fn comments(trivia: &[Trivia]) -> Vec<&str> {
    trivia
        .iter()
        .filter_map(|item| match &item.kind {
            TriviaKind::LineComment(text) => Some(text.as_ref()),
            _ => None,
        })
        .collect()
}

fn name_trivia<'a>(node: &'a AstNode<QualifiedName>, all: &mut Vec<&'a Trivia>) {
    all.extend(&node_source(node).trivia);
    for part in node.value.as_slice() {
        all.extend(&node_source(part).trivia);
    }
}

fn type_trivia<'a>(node: &'a AstNode<Type>, all: &mut Vec<&'a Trivia>) {
    all.extend(&node_source(node).trivia);
    match &node.value {
        Type::Named { name, arguments } => {
            name_trivia(name, all);
            for argument in arguments {
                type_trivia(argument, all);
            }
        }
        Type::Array(inner) | Type::Group(inner) => type_trivia(inner, all),
        Type::Union(items) => {
            for item in items {
                type_trivia(item, all);
            }
        }
        Type::Function { parameters, result } => {
            for parameter in parameters {
                type_trivia(parameter, all);
            }
            type_trivia(result, all);
        }
    }
}

fn expr_trivia<'a>(node: &'a AstNode<Expr>, all: &mut Vec<&'a Trivia>) {
    all.extend(&node_source(node).trivia);
    match &node.value {
        Expr::Literal { ty, .. } => {
            if let Some(ty) = ty {
                type_trivia(ty, all);
            }
        }
        Expr::Name(name) => all.extend(&node_source(name).trivia),
        Expr::Member { base, name } => {
            expr_trivia(base, all);
            all.extend(&node_source(name).trivia);
        }
        Expr::Call { callee, arguments } => {
            expr_trivia(callee, all);
            for argument in arguments {
                expr_trivia(argument, all);
            }
        }
        Expr::Array(items) => {
            for item in items {
                expr_trivia(item, all);
            }
        }
    }
}

fn assert_exactly_once(source: &str, mut actual: Vec<&Trivia>) {
    actual.sort_by_key(|item| item.start.offset);
    let mut stream = lexer::from_str(source);
    let mut expected = Vec::new();
    while let TokenStreamItem::Token(token) = stream.next().unwrap() {
        let kind = match token.kind {
            TokenKind::Whitespace(text) => TriviaKind::Whitespace(text.into()),
            TokenKind::LineComment(text) => TriviaKind::LineComment(text.into()),
            _ => continue,
        };
        expected.push(Trivia {
            kind,
            start: token.start,
            end: token.end,
        });
    }
    assert_eq!(actual, expected.iter().collect::<Vec<_>>());
}

#[test]
fn nested_types_own_their_trivia_without_copying_it_to_parents() {
    let source = "* // array\n(T< A // generic\n > | B) // end";
    let root = read_complete(lexer::from_str(source), type_expr()).unwrap();
    let Type::Array(group) = &root.value else {
        panic!("expected array")
    };
    let Type::Group(union) = &group.value else {
        panic!("expected group")
    };
    let Type::Union(items) = &union.value else {
        panic!("expected union")
    };
    assert_eq!(comments(&node_source(&root).trivia), [" array", " end"]);
    assert!(comments(&node_source(group).trivia).is_empty());
    assert!(comments(&node_source(union).trivia).is_empty());
    let Type::Named { arguments, .. } = &items[0].value else {
        panic!("expected generic type")
    };
    let Type::Named { name, .. } = &arguments[0].value else {
        panic!("expected named argument")
    };
    assert_eq!(comments(&node_source(name).trivia), [" generic"]);
    let mut all = vec![];
    type_trivia(&root, &mut all);
    assert_exactly_once(source, all);
}

#[test]
fn folded_calls_keep_operation_trivia_and_nested_argument_trivia() {
    let source = "f( // call\n *(\"x\"@T<A // type\n>, // array\n y), // argument\n z) // between\n . // member\n value // end";
    let root = {
        let temporary = source.to_owned();
        read_complete(lexer::from_str(&temporary), expr()).unwrap()
    };
    let Expr::Member { base: call, .. } = &root.value else {
        panic!("expected member")
    };
    let Expr::Call { arguments, .. } = &call.value else {
        panic!("expected call")
    };
    assert_eq!(
        comments(&node_source(&root).trivia),
        [" between", " member", " end"]
    );
    assert_eq!(comments(&node_source(call).trivia), [" call", " argument"]);
    assert_eq!(comments(&node_source(&arguments[0]).trivia), [" array"]);
    let mut all = vec![];
    expr_trivia(&root, &mut all);
    assert_exactly_once(source, all);
    for capacity in 1..=4 {
        let streamed = read_complete(
            lexer::from_reader(std::io::BufReader::with_capacity(
                capacity,
                source.as_bytes(),
            )),
            expr(),
        )
        .unwrap();
        assert_eq!(streamed, root);
    }
}

#[test]
fn single_type_terms_keep_trivia_when_temporary_nodes_are_unwrapped() {
    for source in [
        "( // before\nT< A // inner\n >) // after",
        "[A // input\n]( * B )",
    ] {
        let node = read_complete(lexer::from_str(source), type_expr()).unwrap();
        let mut all = vec![];
        type_trivia(&node, &mut all);
        assert_exactly_once(source, all);
    }
}

#[test]
fn single_expressions_keep_trivia_when_temporary_nodes_are_unwrapped() {
    let source = "*( // before\n f( a // call\n ), // array\n x) // after";
    let node = read_complete(lexer::from_str(source), expr()).unwrap();
    let mut all = vec![];
    expr_trivia(&node, &mut all);
    assert_exactly_once(source, all);
}

#[test]
fn comments_at_every_boundary_are_preserved_exactly_once() {
    let parts = [
        "*", "(", "f", "(", "\"x\"", "@", "[", "*", "a", ".", "T", "<", "U", ",", ">", "]", "(",
        "(", "B", "|", "C", ")", ")", ",", ")", ".", "y", ",", ")",
    ];
    let source = parts.join(" // gap\n");
    let root = read_complete(lexer::from_str(&source), expr()).unwrap();
    let mut all = Vec::new();
    expr_trivia(&root, &mut all);
    assert_exactly_once(&source, all);
}

#[test]
fn argument_expressions_own_trivia_up_to_the_comma_or_closing_parenthesis() {
    let source = "outer(f // before comma\n, f // before close\n) // end";
    let root = read_complete(lexer::from_str(source), expr()).unwrap();
    let Expr::Call { arguments, .. } = &root.value else {
        panic!("expected call");
    };
    assert_eq!(
        comments(&node_source(&arguments[0]).trivia),
        [" before comma"]
    );
    assert_eq!(
        comments(&node_source(&arguments[1]).trivia),
        [" before close"]
    );
    assert_eq!(comments(&node_source(&root).trivia), [" end"]);
    for argument in arguments {
        let Expr::Name(identifier) = &argument.value else {
            panic!("expected name")
        };
        assert!(node_source(identifier).trivia.is_empty());
        assert_eq!(&source[node_bytes(argument)], "f");
    }
    let mut all = Vec::new();
    expr_trivia(&root, &mut all);
    assert_exactly_once(source, all);
    let bare = read_complete(lexer::from_str("f // end"), expr()).unwrap();
    assert_eq!(comments(&node_source(&bare).trivia), [" end"]);
}
