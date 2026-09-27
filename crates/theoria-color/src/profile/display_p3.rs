use super::{ColorSpace, ColorSpaceKey, MatrixRgbProfile, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_P3_TO_XYZ_D65, XYZ_D65_TO_LINEAR_P3},
    transform::{RgbTransfer, RgbTransform},
};

profile!(DisplayP3, Rgb);

impl MatrixRgbProfile for DisplayP3 {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Srgb,
        LINEAR_P3_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_P3,
    );
}

rgb_color_space!(DisplayP3, DisplayP3);
predefined_space!(DisplayP3, DISPLAY_P3_KEY);
