use std::fmt;

use super::{ColorCoordinates, Opacity, OpacitySample, Srgb, TypedColorSpace};

/// A logical color retaining its authored space, coordinate representation,
/// and intrinsic opacity.
///
/// The value is not converted into a renderer working space and its color
/// coordinates are not premultiplied. Those operations belong to the renderer
/// that evaluates a Canvas.
pub struct Color<S = Srgb, C = [f32; 3], A = f32>
where
    S: TypedColorSpace,
    C: ColorCoordinates<S::Model>,
    A: OpacitySample,
{
    space: S,
    coordinates: C,
    opacity: Opacity<A>,
}

impl<S, C, A> Color<S, C, A>
where
    S: TypedColorSpace,
    C: ColorCoordinates<S::Model>,
    A: OpacitySample,
{
    pub fn new(space: S, coordinates: C, opacity: Opacity<A>) -> Self {
        Self {
            space,
            coordinates,
            opacity,
        }
    }

    pub const fn space(&self) -> &S {
        &self.space
    }

    pub const fn coordinates(&self) -> &C {
        &self.coordinates
    }

    pub const fn opacity(&self) -> Opacity<A> {
        self.opacity
    }

    pub fn into_parts(self) -> (S, C, Opacity<A>) {
        (self.space, self.coordinates, self.opacity)
    }
}

impl<S, C, A> Clone for Color<S, C, A>
where
    S: TypedColorSpace + Clone,
    C: ColorCoordinates<S::Model> + Clone,
    A: OpacitySample,
{
    fn clone(&self) -> Self {
        Self::new(self.space.clone(), self.coordinates.clone(), self.opacity)
    }
}

impl<S, C, A> Copy for Color<S, C, A>
where
    S: TypedColorSpace + Copy,
    C: ColorCoordinates<S::Model> + Copy,
    A: OpacitySample,
{
}

impl<S, C, A> fmt::Debug for Color<S, C, A>
where
    S: TypedColorSpace + fmt::Debug,
    C: ColorCoordinates<S::Model> + fmt::Debug,
    A: OpacitySample,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Color")
            .field("space", &self.space)
            .field("coordinates", &self.coordinates)
            .field("opacity", &self.opacity)
            .finish()
    }
}

impl<S, C, A> PartialEq for Color<S, C, A>
where
    S: TypedColorSpace + PartialEq,
    C: ColorCoordinates<S::Model> + PartialEq,
    A: OpacitySample,
{
    fn eq(&self, other: &Self) -> bool {
        self.space == other.space
            && self.coordinates == other.coordinates
            && self.opacity == other.opacity
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ColorModel, ColorProfile, ColorSpace, IccRenderingIntent, LinearSrgb, Pcs, Rgb,
        SpaceConnectionError, Transfer, TypedColorSpace, connect_spaces,
    };
    use std::sync::LazyLock;

    struct AuthoredRgb([u8; 3]);

    impl ColorCoordinates<Rgb> for AuthoredRgb {}

    #[test]
    fn a_model_can_accept_a_whole_user_defined_coordinate_type() {
        let color = Color::new(Srgb, AuthoredRgb([12, 34, 56]), Opacity::from(255_u8));

        assert_eq!(color.coordinates().0, [12, 34, 56]);
        assert_eq!(color.opacity().sample(), 255);
    }

    #[test]
    fn authored_color_exposes_its_space_for_connections() {
        let color = Color::new(Srgb, [128_u8; 3], Opacity::from(255_u8));
        let intent = IccRenderingIntent::RelativeColorimetric;

        let transfer = connect_spaces(color.space(), LinearSrgb, intent, intent).unwrap();

        assert_eq!(transfer.input_channels(), 3);
        assert_eq!(transfer.output_channels(), 3);
    }

    struct LogicalModel;

    impl crate::TypedColorModel for LogicalModel {
        const COMPONENTS: usize = 2;
        const ERASED: ColorModel = ColorModel::Channels(2);
    }

    struct LogicalSpace;

    struct LogicalProfile;

    impl ColorProfile for LogicalProfile {
        fn model(&self) -> ColorModel {
            ColorModel::Channels(2)
        }

        fn pcs(&self) -> Pcs {
            Pcs::XyzD50
        }

        fn space_to_pcs_transfer(
            &self,
            intent: IccRenderingIntent,
        ) -> Result<Transfer, SpaceConnectionError> {
            Err(SpaceConnectionError::UnsupportedIntent(intent))
        }

        fn pcs_to_space_transfer(
            &self,
            intent: IccRenderingIntent,
        ) -> Result<Transfer, SpaceConnectionError> {
            Err(SpaceConnectionError::UnsupportedIntent(intent))
        }
    }

    impl TypedColorSpace for LogicalSpace {
        type Model = LogicalModel;

        fn color_space(&self) -> ColorSpace {
            static SPACE: LazyLock<ColorSpace> = LazyLock::new(|| ColorSpace::new(LogicalProfile));
            SPACE.clone()
        }
    }

    struct LogicalCoordinates([f32; 2]);

    impl ColorCoordinates<LogicalModel> for LogicalCoordinates {}

    #[test]
    fn a_model_needs_no_cpu_reference_conversion_to_hold_a_color() {
        let color = Color::new(
            LogicalSpace,
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
