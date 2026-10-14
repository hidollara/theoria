//! A native port with multiple possible errors carries its closed set in its Rust type.
use crate::{
    Type,
    attribute::Registry,
    flow::{Error, Failure, Outcome},
};
use std::{fmt, marker::PhantomData, ops::Deref};

#[doc(hidden)]
pub trait ResultError {
    type Error;
}
impl<T, E> ResultError for Result<T, E> {
    type Error = E;
}

/// A failure from one of the Organon declarations in a port's error contract.
pub struct DeclaredFailure<Set> {
    failure: Failure,
    set: PhantomData<fn() -> Set>,
}

impl<Set> DeclaredFailure<Set> {
    /// The compiler infers the position of the reason in the declared set.
    pub fn new<M: Type, Position>(value: M::Value) -> Self
    where
        Set: membership::Contains<M, Position>,
    {
        Self {
            failure: Failure::new::<M>(value),
            set: PhantomData,
        }
    }

    pub fn into_failure(self) -> Failure {
        self.failure
    }
}
impl<Set: Reasons> DeclaredFailure<Set> {
    /// Recover a statically checked port from an interpreter's typed failure.
    #[doc(hidden)]
    pub fn checked(failure: Failure, registry: &Registry) -> Outcome<Self> {
        let failure = Set::recover(&failure, registry, false)?
            .map_or_else(|| Set::recover(&failure, registry, true), |v| Ok(Some(v)))?
            .ok_or(Error::Failure(failure))?;
        Ok(Self {
            failure,
            set: PhantomData,
        })
    }
}
impl<S> Clone for DeclaredFailure<S> {
    fn clone(&self) -> Self {
        Self {
            failure: self.failure.clone(),
            set: PhantomData,
        }
    }
}
impl<S> fmt::Debug for DeclaredFailure<S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.failure.fmt(f)
    }
}
impl<S> Deref for DeclaredFailure<S> {
    type Target = Failure;
    fn deref(&self) -> &Failure {
        &self.failure
    }
}

// Private proofs prevent provider crates from adding a reason to a generated port.
mod membership {
    pub enum Here {}
    pub struct Next<P>(std::marker::PhantomData<P>);
    pub trait Contains<M, P> {}
    impl<M, Tail> Contains<M, Here> for (M, Tail) {}
    impl<M, Head, Tail, P> Contains<M, Next<P>> for (Head, Tail) where Tail: Contains<M, P> {}
}

#[doc(hidden)]
pub trait Reasons {
    fn recover(
        failure: &Failure,
        registry: &Registry,
        interfaces: bool,
    ) -> Outcome<Option<Failure>>;
}
impl Reasons for () {
    fn recover(_: &Failure, _: &Registry, _: bool) -> Outcome<Option<Failure>> {
        Ok(None)
    }
}
impl<M: Type, Tail: Reasons> Reasons for (M, Tail) {
    fn recover(
        failure: &Failure,
        registry: &Registry,
        interfaces: bool,
    ) -> Outcome<Option<Failure>> {
        if (failure.kind == M::ID || interfaces)
            && let Some(payload) = failure.as_contract(M::ID, registry)?
        {
            if payload.type_id() != std::any::TypeId::of::<M::Value>() {
                return Err(Error::Invalid(format!("invalid payload for {}", M::ID)));
            }
            Ok(Some(Failure {
                kind: M::ID.to_owned(),
                value: payload,
            }))
        } else {
            Tail::recover(failure, registry, interfaces)
        }
    }
}
