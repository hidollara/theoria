use super::{ColorProfile, ColorSpace, Pcs, SpaceConnectionError, TypedColorSpace};
#[cfg(test)]
use crate::icc_profile::IccRenderingIntent;
#[cfg(test)]
use crate::model::Rgb;

macro_rules! named_space {
    ($name:ident, $model:ty) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
        pub struct $name;

        impl TypedColorSpace for $name {
            type Model = $model;
        }
    };
}

macro_rules! builtin_space {
    ($space:ty, $definition:ident) => {
        struct $definition;

        impl From<$space> for $crate::space::ColorSpace {
            fn from(_: $space) -> Self {
                static SPACE: std::sync::LazyLock<$crate::space::ColorSpace> =
                    std::sync::LazyLock::new(|| {
                        let space = $crate::space::ColorSpace::new($definition);
                        assert_eq!(
                            space.model(),
                            <<$space as $crate::space::TypedColorSpace>::Model as $crate::model::TypedColorModel>::ERASED,
                            "typed and runtime color models differ"
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

mod display_p3;
mod lab_d50;
mod linear_display_p3;
mod linear_srgb;
mod srgb;
mod xyz_d50;
mod xyz_d65;

pub use display_p3::DisplayP3;
pub use lab_d50::LabD50;
pub use linear_display_p3::LinearDisplayP3;
pub use linear_srgb::LinearSrgb;
pub use srgb::Srgb;
pub use xyz_d50::XyzD50;
pub use xyz_d65::XyzD65;

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    fn assert_rgb_space<S: TypedColorSpace<Model = Rgb>>() {}

    #[test]
    fn builtin_spaces_have_canonical_identity() {
        assert_ne!(ColorSpace::from(Srgb), ColorSpace::from(LinearSrgb));
        assert_ne!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD65));
        assert_eq!(ColorSpace::from(Srgb), ColorSpace::from(Srgb));
        assert_eq!(ColorSpace::from(LinearSrgb), ColorSpace::from(LinearSrgb));
        assert_eq!(ColorSpace::from(DisplayP3), ColorSpace::from(DisplayP3));
        assert_eq!(
            ColorSpace::from(LinearDisplayP3),
            ColorSpace::from(LinearDisplayP3)
        );
        assert_eq!(ColorSpace::from(LabD50), ColorSpace::from(LabD50));
        assert_eq!(ColorSpace::from(XyzD50), ColorSpace::from(XyzD50));
        assert_eq!(ColorSpace::from(XyzD65), ColorSpace::from(XyzD65));
    }

    #[test]
    fn xyz_d50_and_d65_profiles_use_their_named_adaptation_transfers() {
        let d50 = ColorSpace::from(XyzD50);
        let d65 = ColorSpace::from(XyzD65);
        assert_eq!(d65.space_to_pcs().unwrap(), XyzD65::to_xyz_d50_transfer());
        assert_eq!(d65.pcs_to_space().unwrap(), XyzD50::to_xyz_d65_transfer());
        assert_eq!(
            d50.direct_to(&d65).unwrap(),
            Some(XyzD50::to_xyz_d65_transfer())
        );
        assert_eq!(
            d65.direct_to(&d50).unwrap(),
            Some(XyzD65::to_xyz_d50_transfer())
        );
    }

    #[test]
    fn rgb_profiles_link_directly_to_xyz_d65_and_xyz_d50() {
        let d50 = ColorSpace::from(XyzD50);
        let d65 = ColorSpace::from(XyzD65);
        for (rgb, to_d65, from_d65) in [
            (
                ColorSpace::from(Srgb),
                Srgb::to_xyz_d65_transfer().unwrap(),
                XyzD65::to_srgb_transfer().unwrap(),
            ),
            (
                ColorSpace::from(LinearSrgb),
                LinearSrgb::to_xyz_d65_transfer().unwrap(),
                XyzD65::to_linear_srgb_transfer().unwrap(),
            ),
            (
                ColorSpace::from(DisplayP3),
                DisplayP3::to_xyz_d65_transfer().unwrap(),
                XyzD65::to_display_p3_transfer().unwrap(),
            ),
            (
                ColorSpace::from(LinearDisplayP3),
                LinearDisplayP3::to_xyz_d65_transfer().unwrap(),
                XyzD65::to_linear_display_p3_transfer().unwrap(),
            ),
        ] {
            assert_eq!(rgb.direct_to(&d65).unwrap(), Some(to_d65));
            assert_eq!(
                rgb.direct_to(&d50).unwrap(),
                Some(rgb.space_to_pcs().unwrap())
            );
            assert_eq!(d65.direct_to(&rgb).unwrap(), Some(from_d65));
            let sample = [0.2, 0.4, 0.6];
            let xyz = rgb.transfer_to(&d65).unwrap().evaluate(&sample).unwrap();
            let restored = d65.transfer_to(&rgb).unwrap().evaluate(&xyz).unwrap();
            for (actual, expected) in restored.into_iter().zip(sample) {
                assert!((actual - expected).abs() < 1e-7, "{actual} != {expected}");
            }
        }
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
    }
}
