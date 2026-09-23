use std::{error::Error, fmt};

use super::{
    ColorCoordinates, ColorModel, ColorProfile, Opacity, Sample, Srgb, UnitF32, UnitSample,
};

/// A logical color retaining its authored profile, coordinate representation,
/// and intrinsic opacity.
///
/// The value is not converted into a renderer working space and its color
/// coordinates are not premultiplied. Those operations belong to the renderer
/// that evaluates a Canvas.
pub struct Color<P = Srgb, S = f32, A = UnitF32>
where
    P: ColorProfile,
    S: Sample,
    A: UnitSample,
{
    profile: P,
    coordinates: <P::Model as ColorModel>::Coordinates<S>,
    opacity: Opacity<A>,
}

impl<P, S, A> Color<P, S, A>
where
    P: ColorProfile,
    S: Sample,
    A: UnitSample,
{
    pub fn new(
        profile: P,
        coordinates: <P::Model as ColorModel>::Coordinates<S>,
        opacity: Opacity<A>,
    ) -> Result<Self, ColorError> {
        if coordinates
            .as_slice()
            .iter()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(ColorError::NonFiniteComponent);
        }
        Ok(Self {
            profile,
            coordinates,
            opacity,
        })
    }

    pub const fn profile(&self) -> &P {
        &self.profile
    }

    pub const fn coordinates(&self) -> &<P::Model as ColorModel>::Coordinates<S> {
        &self.coordinates
    }

    pub const fn opacity(&self) -> Opacity<A> {
        self.opacity
    }

    pub fn into_parts(self) -> (P, <P::Model as ColorModel>::Coordinates<S>, Opacity<A>) {
        (self.profile, self.coordinates, self.opacity)
    }
}

impl Color {
    /// Creates an opaque color from normalized encoded-sRGB components.
    pub fn rgb(red: f32, green: f32, blue: f32) -> Result<Self, ColorError> {
        Self::rgba(red, green, blue, 1.0)
    }

    /// Creates a straight-alpha color from normalized encoded-sRGB and linear
    /// opacity.
    pub fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Result<Self, ColorError> {
        let coordinates = [red, green, blue];
        validate_srgb(coordinates)?;
        let opacity = UnitF32::new(alpha).map_err(|error| match error {
            super::UnitSampleError::NonFinite => ColorError::NonFiniteComponent,
            super::UnitSampleError::OutOfRange => ColorError::ComponentOutOfRange,
        })?;
        Self::new(Srgb, coordinates, Opacity::new(opacity))
    }

    /// Creates an opaque color from 8-bit sRGB code values.
    ///
    /// This compatibility constructor normalizes its inputs to the default
    /// `f32` representation. Use [`Color::new`] with `u8` coordinates to retain
    /// the authored representation exactly.
    pub fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba8(red, green, blue, u8::MAX)
    }

    /// Creates a straight-alpha color from 8-bit sRGB and alpha code values.
    ///
    /// This compatibility constructor normalizes its inputs to the default
    /// `f32` representation. Use [`Color::new`] with `u8` coordinates and
    /// opacity to retain the authored representation exactly.
    pub fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
        Self::new(
            Srgb,
            [
                f32::from(red) * BYTE_TO_UNIT,
                f32::from(green) * BYTE_TO_UNIT,
                f32::from(blue) * BYTE_TO_UNIT,
            ],
            Opacity::new(
                UnitF32::new(f32::from(alpha) * BYTE_TO_UNIT)
                    .expect("a normalized u8 is a valid unit sample"),
            ),
        )
        .expect("normalized u8 components form a valid color")
    }

    /// Returns the retained straight-alpha encoded-sRGB components.
    pub(crate) const fn to_straight_srgba(self) -> [f32; 4] {
        [
            self.coordinates[0],
            self.coordinates[1],
            self.coordinates[2],
            self.opacity.sample().get(),
        ]
    }
}

impl Color<Srgb, u8, u8> {
    /// Creates an opaque color while retaining its encoded-sRGB `u8` values.
    pub fn srgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::srgba8(red, green, blue, u8::MAX)
    }

    /// Creates a color while retaining its encoded-sRGB and opacity `u8`
    /// values.
    pub fn srgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self::new(Srgb, [red, green, blue], Opacity::new(alpha))
            .expect("integer samples always form a finite color")
    }
}

impl<P, S, A> Clone for Color<P, S, A>
where
    P: ColorProfile + Clone,
    S: Sample,
    A: UnitSample,
    <P::Model as ColorModel>::Coordinates<S>: Clone,
{
    fn clone(&self) -> Self {
        Self::new(self.profile.clone(), self.coordinates.clone(), self.opacity)
            .expect("a previously constructed color remains valid when cloned")
    }
}

impl<P, S, A> Copy for Color<P, S, A>
where
    P: ColorProfile + Copy,
    S: Sample,
    A: UnitSample,
    <P::Model as ColorModel>::Coordinates<S>: Copy,
{
}

impl<P, S, A> fmt::Debug for Color<P, S, A>
where
    P: ColorProfile + fmt::Debug,
    S: Sample,
    A: UnitSample,
    <P::Model as ColorModel>::Coordinates<S>: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Color")
            .field("profile", &self.profile)
            .field("coordinates", &self.coordinates)
            .field("opacity", &self.opacity)
            .finish()
    }
}

impl<P, S, A> PartialEq for Color<P, S, A>
where
    P: ColorProfile + PartialEq,
    S: Sample,
    A: UnitSample,
    <P::Model as ColorModel>::Coordinates<S>: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.coordinates == other.coordinates
            && self.opacity == other.opacity
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorError {
    NonFiniteComponent,
    ComponentOutOfRange,
}

impl fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteComponent => formatter.write_str("color components must be finite"),
            Self::ComponentOutOfRange => {
                formatter.write_str("sRGB and opacity components must be between 0 and 1")
            }
        }
    }
}

impl Error for ColorError {}

fn validate_srgb(coordinates: [f32; 3]) -> Result<(), ColorError> {
    if coordinates.iter().any(|component| !component.is_finite()) {
        return Err(ColorError::NonFiniteComponent);
    }
    if coordinates
        .iter()
        .any(|component| !(0.0..=1.0).contains(component))
    {
        return Err(ColorError::ComponentOutOfRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_color_retains_its_authored_representation() {
        let color = Color::srgba8(255, 128, 64, 128);

        assert_eq!(color.coordinates(), &[255, 128, 64]);
        assert_eq!(color.opacity().sample(), 128);
    }

    #[test]
    fn default_srgba_constructors_retain_normalized_components() {
        let color = Color::rgba8(255, 128, 64, 128);
        assert_eq!(
            color.to_straight_srgba(),
            [1.0, 128.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0]
        );
        assert_eq!(
            Color::rgb8(128, 128, 128),
            Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0).unwrap()
        );
    }

    #[test]
    fn normalized_srgba_is_retained_exactly() {
        let encoded = [0.8, 0.4, 0.2, 0.6];
        let round_trip = Color::rgba(encoded[0], encoded[1], encoded[2], encoded[3])
            .unwrap()
            .to_straight_srgba();

        for (actual, expected) in round_trip.into_iter().zip(encoded) {
            assert!((actual - expected).abs() < 0.000_001);
        }
    }

    #[test]
    fn rejects_invalid_default_srgba_components() {
        assert_eq!(
            Color::rgba(f32::NAN, 0.0, 0.0, 1.0),
            Err(ColorError::NonFiniteComponent)
        );
        assert_eq!(
            Color::rgba(1.1, 0.0, 0.0, 1.0),
            Err(ColorError::ComponentOutOfRange)
        );
        assert_eq!(
            Color::rgba(1.0, 0.0, 0.0, -0.1),
            Err(ColorError::ComponentOutOfRange)
        );
    }
}
