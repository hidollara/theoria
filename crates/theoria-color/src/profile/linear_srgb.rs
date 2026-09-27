use super::{ColorProfile, ColorSpaceKey, MatrixRgbProfile, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_SRGB_TO_XYZ_D65, XYZ_D65_TO_LINEAR_SRGB},
    transform::{RgbTransfer, RgbTransform},
};

profile!(LinearSrgb, Rgb);

impl MatrixRgbProfile for LinearSrgb {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Linear,
        LINEAR_SRGB_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_SRGB,
    );
}

rgb_color_space!(LinearSrgb, LinearSrgb);
predefined_space!(LinearSrgb, LINEAR_SRGB_KEY);
