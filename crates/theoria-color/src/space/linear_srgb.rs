use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    RgbColorSpace, TypedColorSpace,
};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_SRGB_TO_XYZ_D65, XYZ_D65_TO_LINEAR_SRGB},
};

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = LINEAR_SRGB_TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = XYZ_D65_TO_LINEAR_SRGB;
}

rgb_color_space!(LinearSrgb, LinearSrgb);
predefined_space!(LinearSrgb, LINEAR_SRGB_KEY);
predefined_rgb_profile!(LinearSrgb);
