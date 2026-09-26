use std::{error::Error, fmt};

use super::{ColorCoordinates, ColorProfile, Opacity, OpacityError, OpacitySample, Srgb};

/// A logical color retaining its authored profile, coordinate representation,
/// and intrinsic opacity.
///
/// The value is not converted into a renderer working space and its color
/// coordinates are not premultiplied. Those operations belong to the renderer
/// that evaluates a Canvas.
pub struct Color<P = Srgb, C = [f32; 3], A = f32>
where
    P: ColorProfile,
    C: ColorCoordinates<P::Model>,
    A: OpacitySample,
{
    profile: P,
    coordinates: C,
    opacity: Opacity<A>,
}

impl<P, C, A> Color<P, C, A>
where
    P: ColorProfile,
    C: ColorCoordinates<P::Model>,
    A: OpacitySample,
{
    pub fn new(profile: P, coordinates: C, opacity: Opacity<A>) -> Result<Self, ColorError> {
        if !coordinates.is_finite() {
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

    pub const fn coordinates(&self) -> &C {
        &self.coordinates
    }

    pub const fn opacity(&self) -> Opacity<A> {
        self.opacity
    }

    pub fn into_parts(self) -> (P, C, Opacity<A>) {
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
        let opacity = Opacity::<f32>::new(alpha).map_err(|error| match error {
            OpacityError::NonFinite => ColorError::NonFiniteComponent,
            OpacityError::OutOfRange => ColorError::ComponentOutOfRange,
        })?;
        Self::new(Srgb, coordinates, opacity)
    }
}

impl Color<Srgb, [u8; 3], u8> {
    /// Creates an opaque color while retaining its encoded-sRGB `u8` values.
    pub fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba8(red, green, blue, u8::MAX)
    }

    /// Creates a color while retaining its encoded-sRGB and opacity `u8`
    /// values.
    pub fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self::new(Srgb, [red, green, blue], Opacity::from(alpha))
            .expect("integer samples always form a finite color")
    }
}

impl<P, C, A> Clone for Color<P, C, A>
where
    P: ColorProfile + Clone,
    C: ColorCoordinates<P::Model> + Clone,
    A: OpacitySample,
{
    fn clone(&self) -> Self {
        Self::new(self.profile.clone(), self.coordinates.clone(), self.opacity)
            .expect("a previously constructed color remains valid when cloned")
    }
}

impl<P, C, A> Copy for Color<P, C, A>
where
    P: ColorProfile + Copy,
    C: ColorCoordinates<P::Model> + Copy,
    A: OpacitySample,
{
}

impl<P, C, A> fmt::Debug for Color<P, C, A>
where
    P: ColorProfile + fmt::Debug,
    C: ColorCoordinates<P::Model> + fmt::Debug,
    A: OpacitySample,
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

impl<P, C, A> PartialEq for Color<P, C, A>
where
    P: ColorProfile + PartialEq,
    C: ColorCoordinates<P::Model> + PartialEq,
    A: OpacitySample,
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
    use crate::{ColorModel, ColorProfile, Rgb};

    struct AuthoredRgb([u8; 3]);

    impl ColorCoordinates<Rgb> for AuthoredRgb {
        fn is_finite(&self) -> bool {
            true
        }
    }

    #[test]
    fn a_model_can_accept_a_whole_user_defined_coordinate_type() {
        let color = Color::new(Srgb, AuthoredRgb([12, 34, 56]), Opacity::from(255_u8)).unwrap();

        assert_eq!(color.coordinates().0, [12, 34, 56]);
        assert_eq!(color.opacity().sample(), 255);
    }

    struct LogicalModel;

    impl ColorModel for LogicalModel {
        const COMPONENTS: usize = 2;
    }

    struct LogicalProfile;

    impl ColorProfile for LogicalProfile {
        type Model = LogicalModel;
    }

    struct LogicalCoordinates([f32; 2]);

    impl ColorCoordinates<LogicalModel> for LogicalCoordinates {
        fn is_finite(&self) -> bool {
            self.0.iter().all(|value| value.is_finite())
        }
    }

    #[test]
    fn a_model_needs_no_cpu_reference_conversion_to_hold_a_color() {
        let color = Color::new(
            LogicalProfile,
            LogicalCoordinates([0.25, 0.75]),
            Opacity::<f32>::ONE,
        )
        .unwrap();

        assert_eq!(color.coordinates().0, [0.25, 0.75]);
    }

    #[test]
    fn a_typed_color_retains_its_authored_representation() {
        let color = Color::rgba8(255, 128, 64, 128);

        assert_eq!(color.coordinates(), &[255, 128, 64]);
        assert_eq!(color.opacity().sample(), 128);
    }

    #[test]
    fn eight_bit_srgba_constructors_retain_integer_components() {
        let color = Color::rgba8(255, 128, 64, 128);
        assert_eq!(color.coordinates(), &[255, 128, 64]);
        assert_eq!(color.opacity().sample(), 128);
    }

    #[test]
    fn normalized_srgba_is_retained_exactly() {
        let encoded = [0.8, 0.4, 0.2, 0.6];
        let color = Color::rgba(encoded[0], encoded[1], encoded[2], encoded[3]).unwrap();
        let round_trip = [
            color.coordinates()[0],
            color.coordinates()[1],
            color.coordinates()[2],
            color.opacity().sample(),
        ];

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
