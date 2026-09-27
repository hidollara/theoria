use super::{
    ColorProfile, ColorSpace, ColorTransform, DisplayP3, IccPcs, IccRenderingIntent,
    ProfileConnectionError, RgbColorSpace, TypedColorSpace,
};
use crate::model::Rgb;

named_space!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = DisplayP3::TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = DisplayP3::FROM_XYZ_D65;
}

rgb_color_space!(LinearDisplayP3, LinearDisplayP3);
predefined_space!(LinearDisplayP3, LINEAR_DISPLAY_P3_KEY);
predefined_rgb_profile!(LinearDisplayP3);
