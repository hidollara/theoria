use super::{ColorSpace, ColorSpaceKey, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_P3_TO_XYZ_D65, XYZ_D65_TO_LINEAR_P3},
};

profile!(LinearDisplayP3, Rgb);

impl LinearDisplayP3 {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = LINEAR_P3_TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = XYZ_D65_TO_LINEAR_P3;
}

rgb_color_space!(LinearDisplayP3, LinearDisplayP3);
predefined_space!(LinearDisplayP3, LINEAR_DISPLAY_P3_KEY);
