use super::expr::expr;
use super::identifier::identifier;
use super::rule::{Rule, choice, keyword, list, optional, repeated, sequence, symbol};
use super::types::{ty, type_parameter};
use crate::ito::lexer::Symbol;
use crate::syntax::{BindingBody, Module, ModuleItem, TypeMember};

pub(super) fn body() -> Rule<Module> {
    repeated(choice([type_declaration(), binding()])).map(|items| Module { items })
}

fn type_declaration() -> Rule<ModuleItem> {
    use Symbol::{
        CloseAngle, CloseBrace, CloseBracket, Colon, Comma, OpenAngle, OpenBrace, OpenBracket,
    };

    let parameters = list(
        symbol(OpenAngle),
        type_parameter(),
        symbol(Comma),
        symbol(CloseAngle),
    );
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
    let declaration = sequence![
        keyword("type"),
        identifier(),
        optional(parameters),
        symbol(Colon),
        optional(constructor),
        members,
    ];

    declaration.map(
        |(_, name, parameters, _, constructor, members)| ModuleItem::Type {
            name,
            parameters,
            constructor,
            members: members
                .into_iter()
                .map(|(name, _, ty)| TypeMember { name, ty })
                .collect(),
        },
    )
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
    use crate::syntax::{Identifier, Module, ModuleItem};

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
                        constructor: None,
                        members: vec![],
                    }],
                },
                Cursor::default().advanced("type Canvas:{}// trailing\n".chars())
            )
        );
    }
}
