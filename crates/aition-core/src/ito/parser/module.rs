use super::Rule;
use super::choice::choice;
use super::error::ParseError;
use super::repeated::repeated;
use super::state::Parser;
use super::trivia::trivia;
use super::type_statement::type_statement;
use crate::input::Cursor;
use crate::syntax::ast::{AstNode, Module, ModuleItem, NodeSource, Trivia};

enum ModuleEntry {
    Trivia(Trivia),
    Item(AstNode<ModuleItem>),
}

pub(super) fn expected(cursor: Cursor) -> ParseError {
    ParseError::expected_keyword("type", cursor)
}

pub(super) fn body(parser: &mut Parser<'_, '_>) -> Result<AstNode<Module>, ParseError> {
    let start = parser.cursor()?;
    let entries = repeated(choice![
        #[silent]
        trivia.map(ModuleEntry::Trivia),
        type_statement.map(ModuleEntry::Item),
    ])
    .apply(parser)?;
    let end = parser.cursor()?;
    let mut items = Vec::new();
    let mut module_trivia = Vec::new();
    for entry in entries {
        match entry {
            ModuleEntry::Trivia(value) => module_trivia.push(value),
            ModuleEntry::Item(item) => items.push(item),
        }
    }
    Ok(AstNode {
        value: Module { items },
        source: Some(NodeSource {
            start,
            end,
            trivia: module_trivia,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::body;
    use crate::ito::lexer::{self, Symbol};
    use crate::ito::parser::state::Parser;

    #[test]
    fn body_leaves_the_closing_brace_for_the_enclosing_rule() {
        let text = "{type Canvas: {} // trailing\n}";
        let mut tokens = lexer::from_str(text);
        let mut parser = Parser::new(&mut tokens);
        parser.symbol(Symbol::OpenBrace).unwrap().unwrap();

        let module = body(&mut parser).unwrap();
        let source = module.source.unwrap();
        assert_eq!(module.value.items.len(), 1);
        assert_eq!(
            &text[source.start.offset..source.end.offset],
            "type Canvas: {} // trailing\n"
        );
        assert!(parser.symbol(Symbol::CloseBrace).unwrap().is_some());
    }
}
