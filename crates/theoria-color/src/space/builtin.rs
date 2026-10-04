use std::marker::PhantomData;

use super::{ColorProfile, ColorSpace, ConnectionSpace, SpaceConnectionError, TypedColorSpace};
#[cfg(test)]
use crate::icc_profile::IccRenderingIntent;
#[cfg(test)]
use crate::model::Rgb;
use crate::model::{ColorModel, TypedColorModel};
use crate::transfer::Transfer;

/// Adapts a type's definition to the runtime profile interface without state.
struct TypedProfile<S>(PhantomData<fn() -> S>);

impl<S> TypedProfile<S> {
    const fn new() -> Self {
        Self(PhantomData)
    }
}

impl<S: TypedColorSpace> ColorProfile for TypedProfile<S> {
    fn model(&self) -> ColorModel {
        S::Model::MODEL
    }

    fn connection_space(&self) -> ConnectionSpace {
        S::connection_space()
    }

    fn transfer_to_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        S::transfer_to_connection_space()
    }

    fn transfer_from_connection_space(&self) -> Result<Transfer, SpaceConnectionError> {
        S::transfer_from_connection_space()
    }

    fn direct_transfer_to(
        &self,
        destination: &ColorSpace,
    ) -> Result<Option<Transfer>, SpaceConnectionError> {
        S::direct_transfer_to(destination)
    }

    fn polar_hue_index(&self) -> Option<usize> {
        S::polar_hue_index()
    }

    fn hue_is_powerless(&self, coordinates: &[f64]) -> bool {
        S::hue_is_powerless(coordinates)
    }
}

macro_rules! builtin_space {
    ($space:ident) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $space;

        impl From<$space> for $crate::space::ColorSpace {
            fn from(_: $space) -> Self {
                static SPACE: std::sync::LazyLock<$crate::space::ColorSpace> =
                    std::sync::LazyLock::new(|| {
                        let space =
                            $crate::space::ColorSpace::new($crate::space::builtin::TypedProfile::<
                                $space,
                            >::new());
                        assert_eq!(
                            space.channels(),
                            3,
                            "built-in spaces have three coordinates"
                        );
                        space
                    });
                SPACE.clone()
            }
        }

        impl From<&$space> for $crate::space::ColorSpace {
            fn from(space: &$space) -> Self {
                (*space).into()
            }
        }
    };
}

mod a98_rgb;
mod calculation;
mod display_p3;
mod hsl_srgb;
mod hwb_srgb;
mod lab_d50;
mod lch_d50;
mod linear_display_p3;
mod linear_srgb;
mod oklab;
mod oklch_d65;
mod prophoto_rgb;
mod rec2020;
mod srgb;
mod xyz_d50;
mod xyz_d65;

pub use a98_rgb::A98Rgb;
pub use display_p3::DisplayP3;
pub use hsl_srgb::HslSrgb;
pub use hwb_srgb::HwbSrgb;
pub use lab_d50::LabD50;
pub use lch_d50::LchD50;
pub use linear_display_p3::LinearDisplayP3;
pub use linear_srgb::LinearSrgb;
pub use oklab::OklabD65;
pub use oklch_d65::OklchD65;
pub use prophoto_rgb::ProPhotoRgb;
pub use rec2020::Rec2020;
pub use srgb::Srgb;
pub use xyz_d50::XyzD50;
pub use xyz_d65::XyzD65;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transfer::TransferStage;
    use std::collections::HashSet;

    fn assert_rgb_space<S: TypedColorSpace<Model = Rgb>>() {}

    #[test]
    fn typed_definitions_are_forwarded_to_canonical_runtime_spaces() {
        fn check<S: TypedColorSpace + Copy>(definition: S) {
            let space: ColorSpace = definition.into();
            assert_eq!(space, definition.into());
            assert_eq!(space.model(), S::Model::MODEL);
            assert_eq!(space.connection_space(), S::connection_space());
            assert_eq!(space.polar_hue_index(), S::polar_hue_index());
            for coordinates in [[0.0; 3], [0.5, 0.4, 120.0]] {
                assert_eq!(
                    space.hue_is_powerless(&coordinates),
                    S::hue_is_powerless(&coordinates)
                );
            }
            assert_eq!(
                space.transfer_to_connection_space().unwrap(),
                S::transfer_to_connection_space().unwrap()
            );
            assert_eq!(
                space.transfer_from_connection_space().unwrap(),
                S::transfer_from_connection_space().unwrap()
            );
            for destination in [ColorSpace::from(Srgb), ColorSpace::from(LabD50)] {
                if space != destination {
                    assert_eq!(
                        space.direct_to(&destination).unwrap(),
                        S::direct_transfer_to(&destination).unwrap()
                    );
                }
            }
        }
        check(Srgb);
        check(LinearSrgb);
        check(DisplayP3);
        check(LinearDisplayP3);
        check(A98Rgb);
        check(ProPhotoRgb);
        check(Rec2020);
        check(XyzD50);
        check(XyzD65);
        check(LabD50);
        check(LchD50);
        check(OklabD65);
        check(OklchD65);
        check(HslSrgb);
        check(HwbSrgb);
    }

    #[test]
    fn builtin_spaces_have_canonical_identity() {
        assert_ne!(ColorSpace::from(Srgb), ColorSpace::from(LinearSrgb));
        assert_ne!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD65));
        assert_eq!(ColorSpace::from(Srgb), ColorSpace::from(Srgb));
        assert_eq!(ColorSpace::from(LinearSrgb), ColorSpace::from(LinearSrgb));
        assert_eq!(ColorSpace::from(DisplayP3), ColorSpace::from(DisplayP3));
        assert_eq!(ColorSpace::from(A98Rgb), ColorSpace::from(A98Rgb));
        assert_eq!(ColorSpace::from(ProPhotoRgb), ColorSpace::from(ProPhotoRgb));
        assert_eq!(ColorSpace::from(Rec2020), ColorSpace::from(Rec2020));
        assert_eq!(ColorSpace::from(OklabD65), ColorSpace::from(OklabD65));
        assert_eq!(ColorSpace::from(HslSrgb), ColorSpace::from(HslSrgb));
        assert_eq!(ColorSpace::from(HwbSrgb), ColorSpace::from(HwbSrgb));
        assert_eq!(ColorSpace::from(LchD50), ColorSpace::from(LchD50));
        assert_eq!(ColorSpace::from(OklchD65), ColorSpace::from(OklchD65));
        assert_eq!(
            ColorSpace::from(LinearDisplayP3),
            ColorSpace::from(LinearDisplayP3)
        );
        assert_eq!(ColorSpace::from(LabD50), ColorSpace::from(LabD50));
        assert_eq!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD50));
        assert_eq!(ColorSpace::from(XyzD65), ColorSpace::from(XyzD65));
    }

    #[test]
    fn xyz_d50_and_d65_connection_spaces_use_their_named_adaptation_transfers() {
        let d50 = ColorSpace::from(XyzD50);
        let d65 = ColorSpace::from(XyzD65);
        assert_eq!(d50.connection_space(), ConnectionSpace::XyzD50);
        assert_eq!(d65.connection_space(), ConnectionSpace::XyzD65);
        assert!(d65.transfer_to_connection_space().unwrap().is_empty());
        assert!(d65.transfer_from_connection_space().unwrap().is_empty());
        assert!(d50.direct_to(&d65).unwrap().is_none());
        assert!(d65.direct_to(&d50).unwrap().is_none());
        assert_eq!(
            d50.transfer_to(&d65).unwrap(),
            XyzD50::transfer_to_xyz_d65()
        );
        assert_eq!(
            d65.transfer_to(&d50).unwrap(),
            XyzD65::transfer_to_xyz_d50()
        );
    }

    #[test]
    fn rgb_profiles_connect_through_xyz_d65() {
        let d50 = ColorSpace::from(XyzD50);
        let d65 = ColorSpace::from(XyzD65);
        for rgb in [
            ColorSpace::from(Srgb),
            ColorSpace::from(LinearSrgb),
            ColorSpace::from(DisplayP3),
            ColorSpace::from(LinearDisplayP3),
            ColorSpace::from(A98Rgb),
            ColorSpace::from(Rec2020),
        ] {
            assert!(rgb.direct_to(&d65).unwrap().is_none());
            assert!(rgb.direct_to(&d50).unwrap().is_none());
            assert!(d65.direct_to(&rgb).unwrap().is_none());
            assert_eq!(rgb.connection_space(), ConnectionSpace::XyzD65);
            assert_eq!(
                rgb.transfer_to(&d65).unwrap(),
                rgb.transfer_to_connection_space().unwrap()
            );
            assert_eq!(
                d65.transfer_to(&rgb).unwrap(),
                rgb.transfer_from_connection_space().unwrap()
            );
            assert_eq!(
                rgb.transfer_to(&d50).unwrap(),
                rgb.transfer_to_connection_space()
                    .unwrap()
                    .then(&XyzD65::transfer_to_xyz_d50())
                    .unwrap()
            );
            let sample = [0.2, 0.4, 0.6];
            let xyz = rgb.transfer_to(&d65).unwrap().evaluate(&sample).unwrap();
            let restored = d65.transfer_to(&rgb).unwrap().evaluate(&xyz).unwrap();
            for (actual, expected) in restored.into_iter().zip(sample) {
                assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn prophoto_connects_through_xyz_d50() {
        let prophoto = ColorSpace::from(ProPhotoRgb);
        let xyz = ColorSpace::from(XyzD50);
        assert_eq!(prophoto.connection_space(), ConnectionSpace::XyzD50);
        for sample in [[0.2, 0.4, 0.6], [-0.1, 0.5, 1.2]] {
            let converted = prophoto
                .transfer_to(&xyz)
                .unwrap()
                .evaluate(&sample)
                .unwrap();
            let restored = xyz
                .transfer_to(&prophoto)
                .unwrap()
                .evaluate(&converted)
                .unwrap();
            for (actual, expected) in restored.into_iter().zip(sample) {
                assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn oklab_round_trips_through_xyz_d65() {
        let oklab = ColorSpace::from(OklabD65);
        let xyz = ColorSpace::from(XyzD65);
        assert_eq!(oklab.connection_space(), ConnectionSpace::XyzD65);
        for sample in [[0.5, 0.1, -0.1], [1.0, 0.0, 0.0], [0.2, -0.2, 0.15]] {
            let converted = oklab.transfer_to(&xyz).unwrap().evaluate(&sample).unwrap();
            let restored = xyz
                .transfer_to(&oklab)
                .unwrap()
                .evaluate(&converted)
                .unwrap();
            for (actual, expected) in restored.into_iter().zip(sample) {
                assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn css_polar_spaces_convert_and_round_trip() {
        let srgb = ColorSpace::from(Srgb);
        for (space, red) in [
            (ColorSpace::from(HslSrgb), [0.0, 50.0, 100.0]),
            (ColorSpace::from(HwbSrgb), [0.0, 0.0, 0.0]),
        ] {
            let converted = space.transfer_to(&srgb).unwrap().evaluate(&red).unwrap();
            for (actual, expected) in converted.iter().zip([1.0, 0.0, 0.0]) {
                assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
            }
            let restored = srgb
                .transfer_to(&space)
                .unwrap()
                .evaluate(&converted)
                .unwrap();
            for (index, (actual, expected)) in restored.iter().zip(red).enumerate() {
                let difference = if index == 0 {
                    (actual - expected + 180.0).rem_euclid(360.0) - 180.0
                } else {
                    actual - expected
                };
                assert!(difference.abs() < 1e-5, "{actual} != {expected}");
            }
        }

        for (space, cartesian, polar) in [
            (
                ColorSpace::from(LchD50),
                ColorSpace::from(LabD50),
                [50.0, 40.0, 90.0],
            ),
            (
                ColorSpace::from(OklchD65),
                ColorSpace::from(OklabD65),
                [0.5, 0.2, 90.0],
            ),
        ] {
            let converted = space
                .transfer_to(&cartesian)
                .unwrap()
                .evaluate(&polar)
                .unwrap();
            assert!((converted[0] - polar[0]).abs() < 1e-6);
            assert!(converted[1].abs() < 1e-6);
            assert!((converted[2] - polar[1]).abs() < 1e-6);
            let restored = cartesian
                .transfer_to(&space)
                .unwrap()
                .evaluate(&converted)
                .unwrap();
            for (actual, expected) in restored.iter().zip(polar) {
                assert!((actual - expected).abs() < 1e-5, "{actual} != {expected}");
            }
        }
    }

    #[test]
    fn srgb_curve_definitions_are_ordinary_piecewise_values() {
        use crate::transfer::{CurveSegment, CurveSegmentKind, TransferCurve, TransferCurveKind};

        let decoded_half = ((0.5_f64 + 0.055) / 1.055).powf(2.4);
        let encoded_half = 1.055 * 0.5_f64.powf(1.0 / 2.4) - 0.055;
        assert!((Srgb::decode_curve().evaluate(0.5) - decoded_half).abs() < 1e-12);
        assert!((Srgb::encode_curve().evaluate(0.5) - encoded_half).abs() < 1e-12);

        for (named, threshold, expected, is_decode) in [
            (Srgb::decode_curve(), 0.04045, 1.0 / 12.92, true),
            (Srgb::encode_curve(), 0.003_130_8, 12.92, false),
        ] {
            let TransferCurveKind::Piecewise(segments) = named.kind() else {
                panic!("an sRGB curve must have a piecewise definition");
            };
            let generic = TransferCurve::piecewise(segments.to_vec()).unwrap();
            assert_eq!(named, &generic);
            assert_eq!(&generic == Srgb::decode_curve(), is_decode);
            assert_eq!(&generic == Srgb::encode_curve(), !is_decode);
            let mut changed = segments.to_vec();
            changed[0] = CurveSegment::exclusive(-threshold, CurveSegmentKind::Constant(0.0));
            let changed = TransferCurve::piecewise(changed).unwrap();
            assert_ne!(&changed, Srgb::decode_curve());
            assert_ne!(&changed, Srgb::encode_curve());
            for value in [-2.0, -1.0, -threshold, 0.0, threshold, 0.5, 1.0, 2.0] {
                assert!((named.evaluate(value) - generic.evaluate(value)).abs() < 1e-12);
                assert!((named.evaluate(-value) + named.evaluate(value)).abs() < 1e-12);
            }
            assert!((named.evaluate(1e-5) / 1e-5 - expected).abs() < 1e-12);
        }
    }

    #[test]
    fn related_builtin_spaces_use_direct_calculator_transfers() {
        for (source, destination, stages) in [
            (ColorSpace::from(HslSrgb), ColorSpace::from(Srgb), 1),
            (ColorSpace::from(Srgb), ColorSpace::from(HslSrgb), 1),
            (ColorSpace::from(HwbSrgb), ColorSpace::from(Srgb), 1),
            (ColorSpace::from(Srgb), ColorSpace::from(HwbSrgb), 1),
            (ColorSpace::from(HslSrgb), ColorSpace::from(LinearSrgb), 2),
            (ColorSpace::from(LinearSrgb), ColorSpace::from(HslSrgb), 2),
            (ColorSpace::from(HwbSrgb), ColorSpace::from(LinearSrgb), 2),
            (ColorSpace::from(LinearSrgb), ColorSpace::from(HwbSrgb), 2),
            (ColorSpace::from(HslSrgb), ColorSpace::from(HwbSrgb), 2),
            (ColorSpace::from(HwbSrgb), ColorSpace::from(HslSrgb), 2),
            (ColorSpace::from(LchD50), ColorSpace::from(LabD50), 1),
            (ColorSpace::from(LabD50), ColorSpace::from(LchD50), 1),
            (ColorSpace::from(OklchD65), ColorSpace::from(OklabD65), 1),
            (ColorSpace::from(OklabD65), ColorSpace::from(OklchD65), 1),
        ] {
            let direct = source.direct_to(&destination).unwrap().unwrap();
            assert_eq!(direct.stages().len(), stages);
            assert!(
                direct
                    .stages()
                    .iter()
                    .all(|stage| !matches!(stage, TransferStage::Matrix(_)))
            );
            assert_eq!(source.transfer_to(&destination).unwrap(), direct);
        }
    }

    #[test]
    fn calculator_color_conversions_preserve_edges_and_extended_coordinates() {
        let srgb = ColorSpace::from(Srgb);
        for space in [ColorSpace::from(HslSrgb), ColorSpace::from(HwbSrgb)] {
            let to = srgb.transfer_to(&space).unwrap();
            let from = space.transfer_to(&srgb).unwrap();
            for r in [0.0, 0.25, 0.5, 1.0] {
                for g in [0.0, 0.25, 0.5, 1.0] {
                    for b in [0.0, 0.25, 0.5, 1.0] {
                        let input = [r, g, b];
                        let output = from.evaluate(&to.evaluate(&input).unwrap()).unwrap();
                        for (actual, expected) in output.iter().zip(input) {
                            assert!((actual - expected).abs() < 1e-12);
                        }
                    }
                }
            }
        }
        let hls = ColorSpace::from(HslSrgb).transfer_to(&srgb).unwrap();
        assert_eq!(
            hls.evaluate(&[-720.0, 50.0, 100.0]).unwrap(),
            [1.0, 0.0, 0.0]
        );
        assert_eq!(
            hls.evaluate(&[-600.0, 50.0, 100.0]).unwrap(),
            [0.0, 1.0, 0.0]
        );
        let hwb = ColorSpace::from(HwbSrgb).transfer_to(&srgb).unwrap();
        assert_eq!(hwb.evaluate(&[42.0, 80.0, 80.0]).unwrap(), [0.5; 3]);
        let lab = ColorSpace::from(LabD50);
        let lch = ColorSpace::from(LchD50);
        let polar = lab.transfer_to(&lch).unwrap();
        assert_eq!(polar.evaluate(&[50.0, 0.0, 0.0]).unwrap(), [50.0, 0.0, 0.0]);
        assert_eq!(
            polar.evaluate(&[50.0, 0.0, -40.0]).unwrap(),
            [50.0, 40.0, 270.0]
        );
        let huge = polar.evaluate(&[50.0, 3e200, 4e200]).unwrap();
        assert!((huge[1] / 5e200 - 1.0).abs() < 1e-12);
    }

    #[test]
    fn polar_geometry_is_supplied_by_the_space() {
        for (space, hue_index, neutral, chromatic) in [
            (
                ColorSpace::from(HslSrgb),
                0,
                [0.0, 50.0, 0.0],
                [120.0, 50.0, 100.0],
            ),
            (
                ColorSpace::from(HwbSrgb),
                0,
                [0.0, 100.0, 0.0],
                [120.0, 0.0, 0.0],
            ),
            (
                ColorSpace::from(LchD50),
                2,
                [50.0, 0.001, 0.0],
                [50.0, 40.0, 120.0],
            ),
            (
                ColorSpace::from(OklchD65),
                2,
                [0.5, 0.000001, 0.0],
                [0.5, 0.2, 120.0],
            ),
        ] {
            assert_eq!(space.polar_hue_index(), Some(hue_index));
            assert!(space.hue_is_powerless(&neutral));
            assert!(!space.hue_is_powerless(&chromatic));
        }
        assert_eq!(ColorSpace::from(Srgb).polar_hue_index(), None);
    }

    #[test]
    fn loaded_spaces_preserve_identity_without_equating_channel_counts() {
        let bytes = moxcms::ColorProfile::new_srgb().encode().unwrap();
        let first = crate::IccProfile::from_bytes(&bytes)
            .unwrap()
            .color_space(IccRenderingIntent::RelativeColorimetric);
        let same = first.clone();
        let separate = crate::IccProfile::from_bytes(&bytes)
            .unwrap()
            .color_space(IccRenderingIntent::RelativeColorimetric);
        assert_eq!(first, same);
        assert_ne!(first, separate);
        assert_eq!(first.channels(), 3);
        assert_eq!(first.connection_space(), ConnectionSpace::XyzD50);
        let keys = HashSet::from([first]);
        assert!(keys.contains(&same));
        assert!(!keys.contains(&separate));
    }

    #[test]
    fn spaces_declare_their_coordinate_model() {
        assert_rgb_space::<Srgb>();
        assert_rgb_space::<LinearSrgb>();
        assert_rgb_space::<DisplayP3>();
        assert_rgb_space::<LinearDisplayP3>();
        assert_rgb_space::<A98Rgb>();
        assert_rgb_space::<ProPhotoRgb>();
        assert_rgb_space::<Rec2020>();
    }
}
