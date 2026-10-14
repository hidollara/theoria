//! Rust port types selected by const attribute requirements.
//! Empty sets expose T; singleton sets expose Result<T, E>; larger sets stay closed.
use crate::{
    DeclaredFailure, Type,
    attribute::{Registry, Value},
    flow::{Error, Failure, Outcome},
};
pub use organon_core::semantic::effects::{Node, evaluate};
use std::marker::PhantomData;

pub struct Include<const YES: bool, M, Tail>(PhantomData<(M, Tail)>);
pub trait Normalize {
    type Set;
}
impl Normalize for () {
    type Set = ();
}
impl<M, Tail: Normalize> Normalize for Include<false, M, Tail> {
    type Set = Tail::Set;
}
impl<M, Tail: Normalize> Normalize for Include<true, M, Tail> {
    type Set = (M, Tail::Set);
}

pub trait Port<T> {
    type Value: Clone + 'static;
    fn pack(value: Outcome<Value>, registry: impl FnOnce() -> Registry) -> Outcome<Self::Value>;
    fn unpack(value: Self::Value) -> Outcome<Value>;
}
impl<T: Clone + 'static> Port<T> for () {
    type Value = T;
    fn pack(value: Outcome<Value>, _: impl FnOnce() -> Registry) -> Outcome<T> {
        Ok(value?.get::<T>()?)
    }
    fn unpack(value: T) -> Outcome<Value> {
        Ok(Value::new(value))
    }
}
impl<T: Clone + 'static, M: Type> Port<T> for (M, ()) {
    type Value = Result<T, M::Value>;
    fn pack(value: Outcome<Value>, registry: impl FnOnce() -> Registry) -> Outcome<Self::Value> {
        match value {
            Ok(value) => Ok(Ok(value.get::<T>()?)),
            Err(Error::Failure(e)) => match e.as_contract(M::ID, &registry())? {
                Some(v) => Ok(Err(v.get::<M::Value>()?)),
                None => Err(Error::Failure(e)),
            },
            Err(e) => Err(e),
        }
    }
    fn unpack(value: Self::Value) -> Outcome<Value> {
        value
            .map(Value::new)
            .map_err(|e| Error::Failure(Failure::new::<M>(e)))
    }
}
impl<T: Clone + 'static, M: Type, N: Type, Tail: crate::failure::Reasons + 'static> Port<T>
    for (M, (N, Tail))
{
    type Value = Result<T, DeclaredFailure<Self>>;
    fn pack(value: Outcome<Value>, registry: impl FnOnce() -> Registry) -> Outcome<Self::Value> {
        match value {
            Ok(value) => Ok(Ok(value.get::<T>()?)),
            Err(Error::Failure(e)) => Ok(Err(DeclaredFailure::checked(e, &registry())?)),
            Err(e) => Err(e),
        }
    }
    fn unpack(value: Self::Value) -> Outcome<Value> {
        value
            .map(Value::new)
            .map_err(|e| Error::Failure(e.into_failure()))
    }
}
