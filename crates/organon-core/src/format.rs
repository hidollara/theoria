//! Authored Organon structure. Library elements remain uninterpreted nodes.
use crate::{
    Error, names,
    types::{Input, Port, Type, ValueType},
};
mod contract;
mod definition;
mod element;
mod export;
mod external;
mod import;
mod xml;

pub use contract::{Implementation, InterfaceDefinition, Provide};
pub use definition::{Definition, DefinitionFile, FileDeclaration};
pub use element::{Bind, Declaration, ElementDefinition, Instance, Let, ValueExpr};
pub use export::Export;
pub use external::{DeclaredOutput, ExternalDeclaration, ExternalDefinition};
pub use import::{Import, ImportSource, Use};

/// Names belonging to the language itself, independent of any element library.
fn reserved_name(name: &str) -> bool {
    matches!(
        name,
        "organon"
            | "element"
            | "declare"
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
    )
}
