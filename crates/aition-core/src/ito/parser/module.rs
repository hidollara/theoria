use super::expr::{expr, parameter};
use super::identifier::{identifier, qualified_name};
use super::rule::{Rule, choice, keyword, list, optional, recursive, repeated, sequence, symbol};
use super::types::{ty, type_parameter};
use super::util::delimited;
use crate::ito::lexer::Symbol;
use crate::syntax::{BindingBody, Module, ModuleItem, TypeBody, TypeMember};

pub(super) fn body() -> Rule<Module> {
    recursive(|body| {
        repeated(choice([
            module_definition(body),
            alias(),
            type_item(),
            binding(),
        ]))
        .map(|items| Module { items })
    })
}

fn module_definition(body: Rule<Module>) -> Rule<ModuleItem> {
    sequence![
        keyword("module"),
        identifier(),
        delimited(symbol(Symbol::OpenBrace), body, symbol(Symbol::CloseBrace)),
    ]
    .map(|(_, name, body)| ModuleItem::Module { name, body })
}

fn alias() -> Rule<ModuleItem> {
    sequence![
        keyword("alias"),
        identifier(),
        symbol(Symbol::Equal),
        qualified_name()
    ]
    .map(|(_, name, _, target)| ModuleItem::Alias { name, target })
}

fn type_item() -> Rule<ModuleItem> {
    use Symbol::{CloseAngle, Comma, OpenAngle};

    let parameters = list(
        symbol(OpenAngle),
        type_parameter(),
        symbol(Comma),
        symbol(CloseAngle),
    );
    sequence![
        keyword("type"),
        identifier(),
        optional(parameters),
        choice([type_declaration(), type_definition()]),
    ]
    .map(|(_, name, parameters, body)| ModuleItem::Type {
        name,
        parameters,
        body,
    })
}

fn type_declaration() -> Rule<TypeBody> {
    use Symbol::{CloseBrace, CloseBracket, Colon, Comma, OpenBrace, OpenBracket};

    let constructor = list(
        symbol(OpenBracket),
        ty(),
        symbol(Comma),
        symbol(CloseBracket),
    );
    let members = list(
        symbol(OpenBrace),
        sequence![identifier(), symbol(Colon), ty()],
        symbol(Comma),
        symbol(CloseBrace),
    );
    sequence![symbol(Colon), optional(constructor), members].map(|(_, constructor, members)| {
        TypeBody::Declaration {
            constructor,
            members: members
                .into_iter()
                .map(|(name, _, ty)| TypeMember { name, ty })
                .collect(),
        }
    })
}

fn type_definition() -> Rule<TypeBody> {
    use Symbol::{CloseBrace, CloseBracket, Comma, Equal, OpenBrace, OpenBracket};

    let constructor = list(
        symbol(OpenBracket),
        parameter(),
        symbol(Comma),
        symbol(CloseBracket),
    );
    let bindings = delimited(
        symbol(OpenBrace),
        repeated(
            sequence![identifier(), symbol(Equal), expr()].map(|(name, _, expr)| (name, expr)),
        ),
        symbol(CloseBrace),
    );
    sequence![symbol(Equal), constructor, bindings].map(|(_, constructor, bindings)| {
        TypeBody::Definition {
            constructor,
            bindings,
        }
    })
}

fn binding() -> Rule<ModuleItem> {
    let declaration =
        sequence![symbol(Symbol::Colon), ty()].map(|(_, ty)| BindingBody::Declaration(ty));
    let definition =
        sequence![symbol(Symbol::Equal), expr()].map(|(_, expr)| BindingBody::Definition(expr));
    sequence![identifier(), choice([declaration, definition])]
        .map(|(name, body)| ModuleItem::Binding { name, body })
}

#[cfg(test)]
mod tests {
    use super::body;
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{Lexer, Symbol};
    use crate::ito::parser::rule::{sequence, symbol};
    use crate::syntax::{Identifier, Module, ModuleItem, TypeBody};

    #[test]
    fn empty_body_leaves_the_closing_brace_unread() {
        let braced = sequence![
            symbol(Symbol::OpenBrace),
            body(),
            symbol(Symbol::CloseBrace)
        ];
        let parsed = braced
            .apply(&mut Lexer::new(&mut StrInput::new("{}")))
            .unwrap();
        assert_eq!(
            parsed,
            (
                Cursor::default(),
                Module { items: vec![] },
                Cursor::default().advanced(['{'])
            )
        );
    }

    #[test]
    fn invalid_braced_body_reports_expectations_at_the_failed_token() {
        let rule = sequence![
            symbol(Symbol::OpenBrace),
            body(),
            symbol(Symbol::CloseBrace)
        ];
        for (text, diagnostic) in [
            ("{ )", "expected } at line 1, column 3"),
            ("{ type )", "expected an identifier at line 1, column 8"),
        ] {
            let error = rule
                .apply(&mut Lexer::new(&mut StrInput::new(text)))
                .unwrap_err();
            assert_eq!(error.to_string(), diagnostic);
        }
    }

    #[test]
    fn body_stops_before_a_closing_brace_with_leading_trivia() {
        let text = "type Canvas:{}// trailing\n}";
        let rule = sequence![body(), symbol(Symbol::CloseBrace)];
        let parsed = rule
            .apply(&mut Lexer::new(&mut StrInput::new(text)))
            .unwrap();
        assert_eq!(
            parsed,
            (
                Module {
                    items: vec![ModuleItem::Type {
                        name: Identifier::new("Canvas")
                            .unwrap()
                            .with_cursor(Cursor::default().advanced("type ".chars())),
                        parameters: None,
                        body: TypeBody::Declaration {
                            constructor: None,
                            members: vec![],
                        },
                    }],
                },
                Cursor::default().advanced("type Canvas:{}// trailing\n".chars())
            )
        );
    }
}
