use super::rule::{Rule, choice, repeated};
use crate::syntax::Module;

pub(super) fn body() -> Rule<Module> {
    let item = choice([]);
    repeated(item).map(|items| Module { items })
}

#[cfg(test)]
mod tests {
    use super::body;
    use crate::input::StrInput;
    use crate::ito::Cursor;
    use crate::ito::lexer::{Lexer, Symbol};
    use crate::ito::parser::rule::{sequence, symbol};
    use crate::syntax::Module;

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
}
