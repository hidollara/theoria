use super::{
    ColorProfile, ColorSpace, ColorSpaceKey, ColorTransform, IccPcs, IccRenderingIntent,
    ProfileConnectionError, RgbColorSpace,
};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_P3_TO_XYZ_D65, XYZ_D65_TO_LINEAR_P3},
};

named_space!(DisplayP3, Rgb);

impl DisplayP3 {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = LINEAR_P3_TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = XYZ_D65_TO_LINEAR_P3;
}

rgb_color_space!(DisplayP3, DisplayP3);
predefined_space!(DisplayP3, DISPLAY_P3_KEY);
predefined_rgb_profile!(DisplayP3);
