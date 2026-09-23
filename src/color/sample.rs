use std::{error::Error, fmt};

/// A directly addressable scalar representation used by logical color coordinates.
///
/// Unsigned integer samples are interpreted as normalized code values by a renderer.
/// Floating-point samples retain their supplied coordinate values and may extend
/// outside the unit interval when their color profile permits it.
pub trait Sample: Copy + fmt::Debug + PartialEq + 'static {}

impl Sample for u8 {}
impl Sample for u16 {}
impl Sample for f32 {}
impl Sample for f64 {}

/// A [`Sample`] whose complete value domain denotes the closed unit interval.
///
/// This is a representation contract only. It deliberately provides no numeric
/// operations; renderers choose the precision in which values are evaluated.
pub trait UnitSample: Sample {}

impl UnitSample for u8 {}
impl UnitSample for u16 {}

/// A finite `f32` value in the closed unit interval.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct UnitF32(f32);

impl UnitF32 {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f32) -> Result<Self, UnitSampleError> {
        if !value.is_finite() {
            return Err(UnitSampleError::NonFinite);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(UnitSampleError::OutOfRange);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f32 {
        self.0
    }
}

impl Sample for UnitF32 {}
impl UnitSample for UnitF32 {}

/// A finite `f64` value in the closed unit interval.
#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
#[repr(transparent)]
pub struct UnitF64(f64);

impl UnitF64 {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);

    pub fn new(value: f64) -> Result<Self, UnitSampleError> {
        if !value.is_finite() {
            return Err(UnitSampleError::NonFinite);
        }
        if !(0.0..=1.0).contains(&value) {
            return Err(UnitSampleError::OutOfRange);
        }
        Ok(Self(value))
    }

    pub const fn get(self) -> f64 {
        self.0
    }
}

impl Sample for UnitF64 {}
impl UnitSample for UnitF64 {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitSampleError {
    NonFinite,
    OutOfRange,
}

impl fmt::Display for UnitSampleError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => formatter.write_str("a unit sample must be finite"),
            Self::OutOfRange => formatter.write_str("a unit sample must be between zero and one"),
        }
    }
}

impl Error for UnitSampleError {}

/// A source's intrinsic contribution independently of geometric coverage.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Opacity<S: UnitSample>(S);

impl<S: UnitSample> Opacity<S> {
    pub const fn new(sample: S) -> Self {
        Self(sample)
    }

    pub const fn sample(self) -> S {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_unit_samples_reject_non_finite_and_out_of_range_values() {
        assert_eq!(UnitF32::new(f32::NAN), Err(UnitSampleError::NonFinite));
        assert_eq!(UnitF32::new(f32::INFINITY), Err(UnitSampleError::NonFinite));
        assert_eq!(UnitF32::new(-0.1), Err(UnitSampleError::OutOfRange));
        assert_eq!(
            UnitF64::new(f64::NEG_INFINITY),
            Err(UnitSampleError::NonFinite)
        );
        assert_eq!(UnitF64::new(1.1), Err(UnitSampleError::OutOfRange));
    }

    #[test]
    fn opacity_retains_its_input_representation() {
        assert_eq!(Opacity::new(128_u8).sample(), 128);
        assert_eq!(Opacity::new(UnitF32::new(0.5).unwrap()).sample().get(), 0.5);
    }
}
