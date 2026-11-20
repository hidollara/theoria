/// The contents of a module, with items in source order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub items: Vec<ModuleItem>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ModuleItem {}
