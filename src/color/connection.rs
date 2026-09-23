use super::profile_data::{XYZ_D50_TO_XYZ_D65, XYZ_D65_TO_XYZ_D50};
use super::transform::multiply_f64;
use super::{
    ColorModel, ColorProfile, DisplayP3, LabD50, LinearDisplayP3, LinearSrgb, MatrixRgbProfile,
    RgbColorSpace, Srgb, WhitePoint, XyzD50, XyzD65,
};

/// A profile with a reference conversion to and from D50-adapted CIE XYZ.
///
/// This provides colorimetric meaning for typed coordinates, including the
/// predefined Lab and RGB profiles. It does not prescribe renderer precision,
/// shader code, gamut mapping, or an ICC rendering intent.
pub trait XyzD50Profile: ColorProfile {
    fn coordinates_to_xyz_d50(
        &self,
        coordinates: &<Self::Model as ColorModel>::Coordinates<f64>,
    ) -> [f64; 3];

    fn xyz_d50_to_coordinates(
        &self,
        xyz_d50: [f64; 3],
    ) -> <Self::Model as ColorModel>::Coordinates<f64>;
}

macro_rules! rgb_profile {
    ($($profile:ty),+ $(,)?) => {
        $(
            impl XyzD50Profile for $profile {
                fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
                    adapt_d65_to_d50(Self::TRANSFORM.rgb_to_xyz_d65_f64(*coordinates))
                }

                fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
                    Self::TRANSFORM.xyz_d65_to_rgb_f64(adapt_d50_to_d65(xyz_d50))
                }
            }
        )+
    };
}

rgb_profile!(Srgb, LinearSrgb, DisplayP3, LinearDisplayP3);

impl XyzD50Profile for RgbColorSpace {
    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        adapt_d65_to_d50(self.transform().rgb_to_xyz_d65_f64(*coordinates))
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        self.transform()
            .xyz_d65_to_rgb_f64(adapt_d50_to_d65(xyz_d50))
    }
}

impl XyzD50Profile for XyzD50 {
    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        *coordinates
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        xyz_d50
    }
}

impl XyzD50Profile for XyzD65 {
    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        adapt_d65_to_d50(*coordinates)
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        adapt_d50_to_d65(xyz_d50)
    }
}

impl XyzD50Profile for LabD50 {
    fn coordinates_to_xyz_d50(&self, coordinates: &[f64; 3]) -> [f64; 3] {
        lab_to_xyz_d50(*coordinates)
    }

    fn xyz_d50_to_coordinates(&self, xyz_d50: [f64; 3]) -> [f64; 3] {
        xyz_d50_to_lab(xyz_d50)
    }
}

fn adapt_d65_to_d50(xyz: [f64; 3]) -> [f64; 3] {
    multiply_f64(XYZ_D65_TO_XYZ_D50, xyz)
}

fn adapt_d50_to_d65(xyz: [f64; 3]) -> [f64; 3] {
    multiply_f64(XYZ_D50_TO_XYZ_D65, xyz)
}

const LAB_EPSILON: f64 = 216.0 / 24389.0;
const LAB_KAPPA: f64 = 24389.0 / 27.0;

fn xyz_d50_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    let white = WhitePoint::D50.normalized_xyz();
    let scaled: [f64; 3] = std::array::from_fn(|index| xyz[index] / white[index]);
    let f = scaled.map(|value| {
        if value > LAB_EPSILON {
            value.cbrt()
        } else {
            (LAB_KAPPA * value + 16.0) / 116.0
        }
    });
    [
        116.0 * f[1] - 16.0,
        500.0 * (f[0] - f[1]),
        200.0 * (f[1] - f[2]),
    ]
}

fn lab_to_xyz_d50(lab: [f64; 3]) -> [f64; 3] {
    let fy = (lab[0] + 16.0) / 116.0;
    let fx = lab[1] / 500.0 + fy;
    let fz = fy - lab[2] / 200.0;
    let inverse = |value: f64| {
        let cubed = value * value * value;
        if cubed > LAB_EPSILON {
            cubed
        } else {
            (116.0 * value - 16.0) / LAB_KAPPA
        }
    };
    let scaled = [
        inverse(fx),
        if lab[0] > LAB_KAPPA * LAB_EPSILON {
            fy * fy * fy
        } else {
            lab[0] / LAB_KAPPA
        },
        inverse(fz),
    ];
    let white = WhitePoint::D50.normalized_xyz();
    std::array::from_fn(|index| scaled[index] * white[index])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{Color, Opacity, UnitF64};

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
        fn check<P: XyzD50Profile>(
            profile: P,
            coordinates: <P::Model as ColorModel>::Coordinates<f64>,
        ) where
            <P::Model as ColorModel>::Coordinates<f64>: AsRef<[f64]>,
        {
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

    #[test]
    fn explicit_color_conversion_preserves_opacity_and_authored_value() {
        let source = Color::new(
            Srgb,
            [0.23_f64, 0.51, 0.79],
            Opacity::new(UnitF64::new(0.35).unwrap()),
        )
        .unwrap();
        let lab = source.convert_to(LabD50).unwrap();
        let restored = lab.convert_to(Srgb).unwrap();

        assert_eq!(*source.coordinates(), [0.23, 0.51, 0.79]);
        assert_eq!(lab.opacity(), source.opacity());
        near(*restored.coordinates(), *source.coordinates(), 0.000_01);
    }
}
