use std::{error::Error, fmt};

/// A storage representation accepted by [`Opacity`].
///
/// Floating-point values are validated when wrapped in `Opacity`; every
/// integer code value already denotes a valid opacity.
pub trait OpacitySample: Copy + fmt::Debug + PartialEq + 'static {
    fn validate(self) -> Result<(), OpacityError>;
}

impl OpacitySample for f32 {
    fn validate(self) -> Result<(), OpacityError> {
        validate_f64(f64::from(self))
    }
}

impl OpacitySample for f64 {
    fn validate(self) -> Result<(), OpacityError> {
        validate_f64(self)
    }
}

impl OpacitySample for u8 {
    fn validate(self) -> Result<(), OpacityError> {
        Ok(())
    }
}

impl OpacitySample for u16 {
    fn validate(self) -> Result<(), OpacityError> {
        Ok(())
    }
}

/// A source's intrinsic contribution independently of geometric coverage.
///
/// Floating-point opacity is finite and in the closed unit interval. Integer
/// opacity retains its authored code value until a renderer interprets it.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct Opacity<S: OpacitySample>(S);

impl<S: OpacitySample> Opacity<S> {
    pub fn new(sample: S) -> Result<Self, OpacityError> {
        sample.validate()?;
        Ok(Self(sample))
    }

    pub const fn sample(self) -> S {
        self.0
    }
}

impl Opacity<f32> {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);
}

impl Opacity<f64> {
    pub const ZERO: Self = Self(0.0);
    pub const ONE: Self = Self(1.0);
}

impl Opacity<u8> {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(u8::MAX);
}

impl Opacity<u16> {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(u16::MAX);
}

impl From<u8> for Opacity<u8> {
    fn from(value: u8) -> Self {
        Self(value)
    }
}

impl From<u16> for Opacity<u16> {
    fn from(value: u16) -> Self {
        Self(value)
    }
}

fn validate_f64(value: f64) -> Result<(), OpacityError> {
    if !value.is_finite() {
        return Err(OpacityError::NonFinite);
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(OpacityError::OutOfRange);
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpacityError {
    NonFinite,
    OutOfRange,
}

impl fmt::Display for OpacityError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite => formatter.write_str("opacity must be finite"),
            Self::OutOfRange => formatter.write_str("opacity must be between zero and one"),
        }
    }
}

impl Error for OpacityError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floating_opacity_rejects_non_finite_and_out_of_range_values() {
        assert_eq!(Opacity::<f32>::new(f32::NAN), Err(OpacityError::NonFinite));
        assert_eq!(
            Opacity::<f32>::new(f32::INFINITY),
            Err(OpacityError::NonFinite)
        );
        assert_eq!(Opacity::<f32>::new(-0.1), Err(OpacityError::OutOfRange));
        assert_eq!(
            Opacity::<f64>::new(f64::NEG_INFINITY),
            Err(OpacityError::NonFinite)
        );
        assert_eq!(Opacity::<f64>::new(1.1), Err(OpacityError::OutOfRange));
    }

    #[test]
    fn opacity_retains_its_input_representation() {
        assert_eq!(Opacity::from(128_u8).sample(), 128);
        assert_eq!(Opacity::<f32>::new(0.5).unwrap().sample(), 0.5);
    }
}
