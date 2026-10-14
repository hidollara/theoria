//! Attribute syntax and reference contracts, independent of live Sources.
use super::{Attribute, ResolveContext, Scope};
use crate::{Source, Type};
use std::marker::PhantomData;

/// A declaration's identity and the interfaces it explicitly implements.
#[derive(Clone, Copy, Debug)]
pub struct Contract<'a> {
    pub name: &'a str,
    pub interfaces: &'a [&'a str],
}

#[doc(hidden)]
pub const fn contract(
    name: &'static str,
    contracts: &[(&str, Contract<'static>)],
) -> Contract<'static> {
    let mut i = 0;
    while i < contracts.len() {
        if equal(name, contracts[i].0) {
            return contracts[i].1;
        }
        i += 1;
    }
    // Compiler-generated storage (e.g. a children vector) is not a declaration.
    Contract {
        name,
        interfaces: &[],
    }
}

/// Type information available before any binding has been evaluated.
#[derive(Clone, Copy, Debug)]
pub struct BindingType<'a> {
    pub name: &'a str,
    pub contract: Contract<'a>,
}

#[derive(Clone, Copy, Debug)]
pub struct ParseError {
    message: &'static str,
}

impl ParseError {
    pub const fn new(message: &'static str) -> Self {
        Self { message }
    }

    pub const fn message(&self) -> &'static str {
        self.message
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.message)
    }
}

impl std::error::Error for ParseError {}

impl From<ParseError> for String {
    fn from(error: ParseError) -> Self {
        error.message.into()
    }
}

pub struct ParseContext<'a> {
    bindings: &'a [BindingType<'a>],
    references: Option<&'a mut [bool]>,
}

impl<'a> ParseContext<'a> {
    pub const fn new(bindings: &'a [BindingType<'a>]) -> Self {
        Self {
            bindings,
            references: None,
        }
    }

    /// Record checked requirements without constructing or reading any Sources.
    #[doc(hidden)]
    pub const fn recording(bindings: &'a [BindingType<'a>], references: &'a mut [bool]) -> Self {
        assert!(bindings.len() == references.len());
        Self {
            bindings,
            references: Some(references),
        }
    }

    pub const fn reference<'text, M: Type>(
        &mut self,
        name: &'text str,
    ) -> Result<Reference<'text, M>, ParseError> {
        let mut found = false;
        let mut i = 0;
        while i < self.bindings.len() {
            let binding = &self.bindings[i];
            if equal(binding.name, name) {
                found = true;
                if equal(binding.contract.name, M::ID) {
                    if let Some(references) = &mut self.references {
                        references[i] = true;
                    }
                    return Ok(Reference {
                        name,
                        marker: PhantomData,
                    });
                }
                let mut j = 0;
                while j < binding.contract.interfaces.len() {
                    if equal(binding.contract.interfaces[j], M::ID) {
                        if let Some(references) = &mut self.references {
                            references[i] = true;
                        }
                        return Ok(Reference {
                            name,
                            marker: PhantomData,
                        });
                    }
                    j += 1;
                }
            }
            i += 1;
        }
        Err(ParseError::new(if found {
            "reference does not satisfy the required contract"
        } else {
            "unknown attribute reference"
        }))
    }
}

/// A checked requirement, not a read of the referenced value.
pub struct Reference<'a, M> {
    name: &'a str,
    marker: PhantomData<fn() -> M>,
}

impl<M> Copy for Reference<'_, M> {}
impl<M> Clone for Reference<'_, M> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<M: Type> Reference<'_, M> {
    pub fn resolve(self, cx: &mut ResolveContext<'_>) -> Result<Source<M::Value>, String> {
        cx.reference::<M>(self.name)
    }
}

/// Default preparation for declarations without a custom attribute parser.
#[doc(hidden)]
pub struct ReferenceParser<M>(pub PhantomData<fn() -> M>);

impl<M: Type + 'static> ReferenceParser<M> {
    pub const fn dependencies<const N: usize>(
        &self,
        text: &str,
        bindings: &[BindingType<'_>],
    ) -> [bool; N] {
        let mut references = [false; N];
        let mut cx = ParseContext::recording(bindings, &mut references);
        if let Err(error) = cx.reference::<M>(text) {
            panic!("{}", error.message());
        }
        references
    }

    pub const fn measure(&self, _text: &str, _cx: ParseContext<'_>) -> Result<usize, ParseError> {
        Ok(0)
    }

    pub const fn prepare<'a, const N: usize>(
        &self,
        text: &'a str,
        mut cx: ParseContext<'a>,
    ) -> impl FnOnce(&Scope) -> Result<Attribute<M::Value>, String> + use<'a, M, N> {
        let reference = match cx.reference::<M>(text) {
            Ok(reference) => reference,
            Err(error) => panic!("{}", error.message()),
        };
        move |scope| {
            let mut cx = ResolveContext::new(scope);
            let source = reference.resolve(&mut cx)?;
            Ok(cx.finish(source))
        }
    }
}

pub(crate) const fn equal(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a.as_bytes()[i] != b.as_bytes()[i] {
            return false;
        }
        i += 1;
    }
    true
}
