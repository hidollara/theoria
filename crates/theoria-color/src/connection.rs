use super::pcs;
use super::profile_data::{XYZ_D50_TO_XYZ_D65, XYZ_D65_TO_XYZ_D50};
use super::transform::multiply_f64;
use super::{
    ColorSpace, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb, WhitePoint,
    XyzD50, XyzD65,
};

/// A profile with a reference conversion to and from D50-adapted CIE XYZ.
///
/// This provides colorimetric meaning for typed coordinates, including the
/// predefined Lab and RGB profiles. It does not prescribe renderer precision,
/// shader code, gamut mapping, or an ICC rendering intent.
pub trait XyzD50Profile: ColorSpace {
    type ReferenceCoordinates: AsRef<[f64]>;

    fn coordinates_to_xyz_d50(&self, coordinates: &Self::ReferenceCoordinates) -> [f64; 3];

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> Self::ReferenceCoordinates;
}

macro_rules! rgb_profile {
    ($($profile:ty),+ $(,)?) => {
        $(
            impl XyzD50Profile for $profile {
                type ReferenceCoordinates = [f64; 3];

                fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
                    adapt_d65_to_d50(RgbColorSpace::from(*self).to_xyz_d65_f64(*coordinates))
                }

                fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
                    RgbColorSpace::from(*self).xyz_d65_to_rgb_f64(adapt_d50_to_d65(xyz_d50))
                }
            }
        )+
    };
}

rgb_profile!(Srgb, LinearSrgb, DisplayP3, LinearDisplayP3);

impl XyzD50Profile for RgbColorSpace {
    type ReferenceCoordinates = [f64; 3];

    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        adapt_d65_to_d50(self.to_xyz_d65_f64(*coordinates))
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        self.xyz_d65_to_rgb_f64(adapt_d50_to_d65(xyz_d50))
    }
}

impl XyzD50Profile for XyzD50 {
    type ReferenceCoordinates = [f64; 3];

    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        *coordinates
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        xyz_d50
    }
}

impl XyzD50Profile for XyzD65 {
    type ReferenceCoordinates = [f64; 3];

    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        adapt_d65_to_d50(*coordinates)
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        adapt_d50_to_d65(xyz_d50)
    }
}

impl XyzD50Profile for LabD50 {
    type ReferenceCoordinates = [f64; 3];

    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        pcs::lab_to_xyz(*coordinates, WhitePoint::D50.normalized_xyz())
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        pcs::xyz_to_lab(xyz_d50, WhitePoint::D50.normalized_xyz())
    }
}

fn adapt_d65_to_d50(xyz: [f64; 3]) -> [f64; 3] {
    multiply_f64(XYZ_D65_TO_XYZ_D50, xyz)
}

fn adapt_d50_to_d65(xyz: [f64; 3]) -> [f64; 3] {
    multiply_f64(XYZ_D50_TO_XYZ_D65, xyz)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
        for (actual, expected) in actual.into_iter().zip(expected) {
            assert!(
                (actual - expected).abs() < tolerance,
                "{actual} != {expected}"
            );
        }
    }

    #[test]
    fn d65_white_adapts_to_d50_white() {
        near(
            XyzD65.coordinates_to_xyz_d50(&WhitePoint::D65.normalized_xyz()),
            WhitePoint::D50.normalized_xyz(),
            0.000_01,
        );
        near(
            Srgb.coordinates_to_xyz_d50(&[1.0, 1.0, 1.0]),
            WhitePoint::D50.normalized_xyz(),
            0.000_01,
        );
    }

    #[test]
    fn lab_white_is_the_d50_reference_white() {
        near(
            LabD50.xyz_d50_to_coordinates(WhitePoint::D50.normalized_xyz()),
            [100.0, 0.0, 0.0],
            0.000_01,
        );
    }

    #[test]
    fn predefined_profiles_round_trip_via_d50_xyz() {
        fn check<P: XyzD50Profile>(profile: P, coordinates: P::ReferenceCoordinates) {
            let actual =
                profile.xyz_d50_to_coordinates(profile.coordinates_to_xyz_d50(&coordinates));
            for (actual, expected) in actual.as_ref().iter().zip(coordinates.as_ref()) {
                assert!((actual - expected).abs() < 0.000_01);
            }
        }

        check(Srgb, [0.23, 0.51, 0.79]);
        check(LinearSrgb, [0.23, 0.51, 0.79]);
        check(DisplayP3, [0.23, 0.51, 0.79]);
        check(LinearDisplayP3, [0.23, 0.51, 0.79]);
        check(XyzD50, [0.23, 0.51, 0.79]);
        check(XyzD65, [0.23, 0.51, 0.79]);
        check(LabD50, [42.0, 15.0, -30.0]);
        check(RgbColorSpace::Srgb, [0.23, 0.51, 0.79]);
    }

    #[test]
    fn css_reference_color_agrees_across_xyz_whites() {
        let d50 = [0.2005, 0.14089, 0.4472];
        let d65 = [0.21661, 0.14602, 0.59452];
        near(XyzD65.coordinates_to_xyz_d50(&d65), d50, 0.000_1);
    }
}
