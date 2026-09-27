use super::{ColorProfile, ColorSpaceKey, MatrixRgbProfile, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_P3_TO_XYZ_D65, XYZ_D65_TO_LINEAR_P3},
    transform::{RgbTransfer, RgbTransform},
};

profile!(LinearDisplayP3, Rgb);

impl MatrixRgbProfile for LinearDisplayP3 {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Linear,
        LINEAR_P3_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_P3,
    );
}

rgb_color_space!(LinearDisplayP3, LinearDisplayP3);
predefined_space!(LinearDisplayP3, LINEAR_DISPLAY_P3_KEY);
