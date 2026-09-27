use std::fmt;

use super::{ColorCoordinates, ColorSpace, Opacity, OpacitySample, Srgb};

/// A logical color retaining its authored profile, coordinate representation,
/// and intrinsic opacity.
///
/// The value is not converted into a renderer working space and its color
/// coordinates are not premultiplied. Those operations belong to the renderer
/// that evaluates a Canvas.
pub struct Color<P = Srgb, C = [f32; 3], A = f32>
where
    P: ColorSpace,
    C: ColorCoordinates<P::Model>,
    A: OpacitySample,
{
    profile: P,
    coordinates: C,
    opacity: Opacity<A>,
}

impl<P, C, A> Color<P, C, A>
where
    P: ColorSpace,
    C: ColorCoordinates<P::Model>,
    A: OpacitySample,
{
    pub fn new(profile: P, coordinates: C, opacity: Opacity<A>) -> Self {
        Self {
            profile,
            coordinates,
            opacity,
        }
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

impl<P, C, A> Clone for Color<P, C, A>
where
    P: ColorSpace + Clone,
    C: ColorCoordinates<P::Model> + Clone,
    A: OpacitySample,
{
    fn clone(&self) -> Self {
        Self::new(self.profile.clone(), self.coordinates.clone(), self.opacity)
    }
}

impl<P, C, A> Copy for Color<P, C, A>
where
    P: ColorSpace + Copy,
    C: ColorCoordinates<P::Model> + Copy,
    A: OpacitySample,
{
}

impl<P, C, A> fmt::Debug for Color<P, C, A>
where
    P: ColorSpace + fmt::Debug,
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
    P: ColorSpace + PartialEq,
    C: ColorCoordinates<P::Model> + PartialEq,
    A: OpacitySample,
{
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.coordinates == other.coordinates
            && self.opacity == other.opacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ColorModel, ColorSpace, Rgb};

    struct AuthoredRgb([u8; 3]);

    impl ColorCoordinates<Rgb> for AuthoredRgb {}

    #[test]
    fn a_model_can_accept_a_whole_user_defined_coordinate_type() {
        let color = Color::new(Srgb, AuthoredRgb([12, 34, 56]), Opacity::from(255_u8));

        assert_eq!(color.coordinates().0, [12, 34, 56]);
        assert_eq!(color.opacity().sample(), 255);
    }

    struct LogicalModel;

    impl ColorModel for LogicalModel {
        const COMPONENTS: usize = 2;
    }

    struct LogicalProfile;

    impl ColorSpace for LogicalProfile {
        type Model = LogicalModel;
    }

    struct LogicalCoordinates([f32; 2]);

    impl ColorCoordinates<LogicalModel> for LogicalCoordinates {}

    #[test]
    fn a_model_needs_no_cpu_reference_conversion_to_hold_a_color() {
        let color = Color::new(
            LogicalProfile,
            LogicalCoordinates([0.25, 0.75]),
            Opacity::<f32>::ONE,
        );

        assert_eq!(color.coordinates().0, [0.25, 0.75]);
    }

    #[test]
    fn a_typed_color_retains_its_authored_representation() {
        let color = Color::new(Srgb, [255_u8, 128, 64], Opacity::from(128_u8));

        assert_eq!(color.coordinates(), &[255, 128, 64]);
        assert_eq!(color.opacity().sample(), 128);
    }

    #[test]
    fn normalized_srgba_is_retained_exactly() {
        let encoded = [0.8, 0.4, 0.2, 0.6];
        let color = Color::new(
            Srgb,
            [encoded[0], encoded[1], encoded[2]],
            Opacity::<f32>::new(encoded[3]).unwrap(),
        );
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
    fn raw_coordinates_do_not_inherit_a_value_range() {
        let color = Color::new(Srgb, [f32::NAN, 1.1, f32::INFINITY], Opacity::<f32>::ONE);
        assert!(color.coordinates()[0].is_nan());
        assert_eq!(color.coordinates()[1], 1.1);
        assert_eq!(color.coordinates()[2], f32::INFINITY);
    }

    struct UnitRgb([f32; 3]);

    impl TryFrom<[f32; 3]> for UnitRgb {
        type Error = ();

        fn try_from(coordinates: [f32; 3]) -> Result<Self, Self::Error> {
            coordinates
                .iter()
                .all(|component| (0.0..=1.0).contains(component))
                .then_some(Self(coordinates))
                .ok_or(())
        }
    }

    impl ColorCoordinates<Rgb> for UnitRgb {}

    #[test]
    fn constrained_coordinates_fail_during_conversion_not_color_construction() {
        assert!(UnitRgb::try_from([1.1, 0.0, 0.0]).is_err());

        let coordinates = UnitRgb::try_from([0.25, 0.5, 0.75]).unwrap();
        let color = Color::new(Srgb, coordinates, Opacity::<f32>::ONE);
        assert_eq!(color.coordinates().0, [0.25, 0.5, 0.75]);
    }
}
