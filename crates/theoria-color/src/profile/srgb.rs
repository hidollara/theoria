use super::{ColorProfile, ColorSpaceKey, MatrixRgbProfile, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_SRGB_TO_XYZ_D65, XYZ_D65_TO_LINEAR_SRGB},
    transform::{RgbTransfer, RgbTransform},
};

profile!(Srgb, Rgb);

impl MatrixRgbProfile for Srgb {
    const TRANSFORM: RgbTransform = RgbTransform::new(
        RgbTransfer::Srgb,
        LINEAR_SRGB_TO_XYZ_D65,
        XYZ_D65_TO_LINEAR_SRGB,
    );
}

rgb_color_space!(Srgb, Srgb);
predefined_space!(Srgb, SRGB_KEY);
