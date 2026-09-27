use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    RgbColorSpace, Srgb, TypedColorSpace,
};
use crate::model::Rgb;

named_space!(LinearSrgb, Rgb);

impl LinearSrgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = Srgb::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = Srgb::FROM_XYZ_D65;
}

rgb_color_space!(LinearSrgb, LinearSrgb);
predefined_space!(LinearSrgb, LINEAR_SRGB_KEY);
predefined_rgb_profile!(LinearSrgb);
