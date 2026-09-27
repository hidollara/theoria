use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    RgbColorSpace, TypedColorSpace,
};
use crate::model::Rgb;

named_space!(Srgb, Rgb);

impl Srgb {
    // Display P3 uses the same transfer curve. Keep these aliases tied to the
    // values used by CPU evaluation and generated transfer shaders.
    pub const DECODE_THRESHOLD: f64 = crate::transform::SRGB_DECODE_THRESHOLD;
    pub const ENCODE_THRESHOLD: f64 = crate::transform::SRGB_ENCODE_THRESHOLD;
    pub const LINEAR_SCALE: f64 = crate::transform::SRGB_LINEAR_SCALE;
    pub const POWER_SCALE: f64 = crate::transform::SRGB_POWER_SCALE;
    pub const POWER_OFFSET: f64 = crate::transform::SRGB_POWER_OFFSET;
    pub const GAMMA: f64 = crate::transform::SRGB_GAMMA;

    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded sRGB spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [506752.0 / 1228815.0, 87881.0 / 245763.0, 12673.0 / 70218.0],
        [87098.0 / 409605.0, 175762.0 / 245763.0, 12673.0 / 175545.0],
        [7918.0 / 409605.0, 87881.0 / 737289.0, 1001167.0 / 1053270.0],
    ];
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [12831.0 / 3959.0, -329.0 / 214.0, -1974.0 / 3959.0],
        [
            -851781.0 / 878810.0,
            1648619.0 / 878810.0,
            36519.0 / 878810.0,
        ],
        [705.0 / 12673.0, -2585.0 / 12673.0, 705.0 / 667.0],
    ];
}

rgb_color_space!(Srgb, Srgb);
predefined_space!(Srgb, SRGB_KEY);
predefined_rgb_profile!(Srgb);
