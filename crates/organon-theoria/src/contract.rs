//! Rust contracts for Organon declarations, native values, and named outputs.

use crate::{BindingContext, attribute::Value};
use theoria_reactive::ReactiveError;

pub trait Type: Sized {
    type Value: Clone + std::fmt::Debug + 'static;

    fn interpret(
        text: &str,
        scope: &crate::attribute::Scope,
    ) -> Result<crate::attribute::Attribute<Self::Value>, String> {
        reference::<Self>(text, scope)
    }

    fn interpret_fallible(
        text: &str,
        scope: &crate::attribute::Scope,
    ) -> Result<crate::attribute::Attribute<Result<Self::Value, &'static str>>, String> {
        if Self::reference_only() {
            let mut context = crate::attribute::ParseContext::new(scope);
            let source = context.reference_fallible::<Self>(text)?;
            Ok(context.finish(source))
        } else {
            let attribute = Self::interpret(text, scope)?;
            Ok(crate::attribute::Attribute {
                dependencies: attribute.dependencies,
                source: attribute.source.map(|value| Ok(value.clone())),
            })
        }
    }

    /// Extract a value through a generated type or interface contract.
    #[doc(hidden)]
    fn resolve(value: &Value) -> Result<Self::Value, String> {
        value.get()
    }

    #[doc(hidden)]
    fn reference_only() -> bool {
        true
    }
}

/// Fallback used when an interface has no explicit Rust attribute interpreter.
#[doc(hidden)]
pub trait DefaultInterpret: Type {
    const __ORGANON_REFERENCE_ONLY: bool = true;

    fn __organon_interpret(
        text: &str,
        scope: &crate::attribute::Scope,
    ) -> Result<crate::attribute::Attribute<Self::Value>, String> {
        reference::<Self>(text, scope)
    }
}

impl<T: Type> DefaultInterpret for T {}

#[doc(hidden)]
pub trait DefaultParse: Type {
    const __ORGANON_REFERENCE_ONLY: bool = true;

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
    let mut context = crate::attribute::ParseContext::new(scope);
    let source = context.reference::<T>(text)?;
    Ok(context.finish(source))
}
