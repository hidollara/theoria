//! Organon syntax, independent of XML layout and editing state.
//! Names and attribute text are unresolved until analysis; sequence order is retained.
//! Identifier spelling is validated at construction, independently of name resolution.
mod binding;
mod control;
mod declare_element;
mod declare_predicate;
mod element;
mod export;
mod expression;
mod identifier;
mod import;
mod interface;
mod module;
mod predicate;
mod provide;
mod types;

pub use binding::{Bind, Binding, Let};
pub use control::{Branch, BranchMember, Catch, Select, Try, TryMember};
pub use declare_element::{DeclareElement, DeclareElementMember};
pub use declare_predicate::{DeclarePredicate, DeclarePredicateMember};
pub use element::{Element, ElementMember};
pub use export::Export;
pub use expression::{Construction, ElementExpression, PredicateExpression};
pub use identifier::{Identifier, IdentifierRef};
pub use import::{Import, ImportSource, Use};
pub use interface::{Implementation, Interface};
pub use module::{Definition, Item, Module};
pub use predicate::{Predicate, PredicateMember};
pub use provide::Provide;
pub use types::{ChildCount, ChildrenType, Port, TypeAnnotation, TypeName};

pub(super) fn check_depth(depth: usize) -> Result<(), crate::Error> {
    if depth > 128 {
        Err(crate::Error("expression nesting exceeds 128 levels".into()))
    } else {
        Ok(())
    }
}

pub(super) fn validate_definition_name(name: &Identifier, kind: &str) -> Result<(), crate::Error> {
    if reserved_name(name) {
        Err(crate::Error(format!(
            "invalid or reserved {kind} name {name:?}"
        )))
    } else {
        Ok(())
    }
}

/// Names belonging to the language itself, independent of any element library.
pub(crate) fn reserved_name(name: &str) -> bool {
    matches!(
        name,
        "organon"
            | "element"
            | "interface"
            | "input"
            | "output"
            | "let"
            | "bind"
            | "ref"
            | "export"
            | "impl"
            | "provide"
            | "implements"
            | "import"
            | "use"
            | "children"
            | "select"
            | "case"
            | "error"
            | "try"
            | "catch"
            | "unreachable"
            | "all"
            | "any"
            | "not"
            | "predicate"
            | "declare-predicate"
            | "declare-element"
    )
}
