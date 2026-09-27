use super::{ColorSpace, ColorSpaceKey, RgbColorSpace};
use crate::{
    model::Rgb,
    profile_data::{LINEAR_SRGB_TO_XYZ_D65, XYZ_D65_TO_LINEAR_SRGB},
};

profile!(Srgb, Rgb);

impl Srgb {
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = LINEAR_SRGB_TO_XYZ_D65;
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = XYZ_D65_TO_LINEAR_SRGB;
}

rgb_color_space!(Srgb, Srgb);
predefined_space!(Srgb, SRGB_KEY);
