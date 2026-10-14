//! Rust contracts for Organon declarations, native values, and named outputs.

use crate::{BindingContext, attribute::Value};
use theoria_reactive::ReactiveError;

pub trait Type: Sized + 'static {
    /// Canonical Organon declaration identity, preserved across import aliases.
    const ID: &'static str;
    type Value: Clone + std::fmt::Debug + 'static;

    fn interpret(
        text: &str,
        scope: &crate::attribute::Scope,
    ) -> Result<crate::attribute::Attribute<Self::Value>, String> {
        reference::<Self>(text, scope)
    }

    /// Extract a value through a generated type or interface contract.
    #[doc(hidden)]
    fn resolve(value: &Value) -> Result<Self::Value, String> {
        value.get()
    }
}

#[doc(hidden)]
pub trait DefaultParse: Type {
    const __ORGANON_PARSER: crate::attribute::ReferenceParser<Self> =
        crate::attribute::ReferenceParser(std::marker::PhantomData);

    fn __organon_parse(
        text: &str,
        scope: &crate::attribute::Scope,
    ) -> Result<crate::attribute::Attribute<Self::Value>, String> {
        reference::<Self>(text, scope)
    }
}

impl<T: Type> DefaultParse for T {}

pub trait Output {
    type Input: 'static;
    type Value: 'static;
}

pub trait Evaluate: Output {
    fn evaluate(input: &Self::Input, cx: &crate::ReadContext<'_, '_>) -> Self::Value;

    fn calculation(input: std::rc::Rc<Self::Input>) -> crate::Calculation<Self::Value> {
        crate::Calculation::new(move |cx| Self::evaluate(&input, cx))
    }

    /// Outputs implemented by the same native method share this identity.
    #[doc(hidden)]
    fn computation_id() -> Option<std::any::TypeId> {
        None
    }
}

/// Shares native output sources within one constructed Element.
#[doc(hidden)]
#[derive(Default)]
pub struct OutputBindings {
    sources: std::collections::HashMap<std::any::TypeId, Box<dyn std::any::Any>>,
}

impl OutputBindings {
    pub fn bind<O: Evaluate>(
        &mut self,
        calculation: crate::Calculation<O::Value>,
        context: &mut impl BindingContext,
    ) -> Result<crate::Source<O::Value>, ReactiveError>
    where
        O::Value: Clone,
    {
        let Some(key) = O::computation_id() else {
            return calculation.bind_in(context);
        };
        if let Some(source) = self.sources.get(&key) {
            return Ok(source
                .downcast_ref::<crate::Source<O::Value>>()
                .expect("shared output computation has the same result type")
                .clone());
        }
        let source = calculation.bind_in(context)?;
        self.sources.insert(key, Box::new(source.clone()));
        Ok(source)
    }
}

/// Explicitly provide a constructed element's outputs under an interface contract.
pub trait Provide<I> {
    fn provide(&self) -> I;
}

/// An external Element declaration. Tuple storage avoids per-element helper type names.
pub trait Declaration: Sized {
    type Definition;
    type Inputs;
    type Input;
    type Arguments;
    type Outputs;

    #[doc(hidden)]
    fn bind(
        definition: Self::Definition,
        context: &mut impl BindingContext,
    ) -> Result<Self::Outputs, ReactiveError>;
}

/// Supplies output computations; evaluation belongs to Organon.
pub trait Implementation<I: Declaration> {
    fn build(&self, input: I::Input) -> I::Definition;
}

#[derive(Debug)]
pub enum BuildError {
    MissingInput(&'static str),
    Reactive(ReactiveError),
    Attribute(String),
}

impl std::fmt::Display for BuildError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingInput(name) => write!(f, "missing input {name:?}"),
            Self::Reactive(error) => error.fmt(f),
            Self::Attribute(error) => f.write_str(error),
        }
    }
}

impl std::error::Error for BuildError {}
impl From<ReactiveError> for BuildError {
    fn from(value: ReactiveError) -> Self {
        Self::Reactive(value)
    }
}

impl From<String> for BuildError {
    fn from(value: String) -> Self {
        Self::Attribute(value)
    }
}

fn reference<T: Type>(
    text: &str,
    scope: &crate::attribute::Scope,
) -> Result<crate::attribute::Attribute<T::Value>, String> {
    let schema = scope.schema();
    let parsed = crate::attribute::ParseContext::new(&schema).reference::<T>(text)?;
    let mut context = crate::attribute::ResolveContext::new(scope);
    let source = parsed.resolve(&mut context)?;
    Ok(context.finish(source))
}
