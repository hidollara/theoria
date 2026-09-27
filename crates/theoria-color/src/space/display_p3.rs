use super::{
    ColorProfile, ColorSpace, ColorTransform, IccPcs, IccRenderingIntent, ProfileConnectionError,
    RgbColorSpace, TypedColorSpace,
};
use crate::model::Rgb;

named_space!(DisplayP3, Rgb);

impl DisplayP3 {
    // Exact rational matrices from CSS Color 4's color-conversion sample code.
    // The linear and encoded Display P3 spaces share these primaries.
    pub(crate) const TO_XYZ_D65: [[f64; 3]; 3] = [
        [
            608311.0 / 1250200.0,
            189793.0 / 714400.0,
            198249.0 / 1000160.0,
        ],
        [
            35783.0 / 156275.0,
            247089.0 / 357200.0,
            198249.0 / 2500400.0,
        ],
        [0.0, 32229.0 / 714400.0, 5220557.0 / 5000800.0],
    ];
    pub(crate) const FROM_XYZ_D65: [[f64; 3]; 3] = [
        [
            446124.0 / 178915.0,
            -333277.0 / 357830.0,
            -72051.0 / 178915.0,
        ],
        [-14852.0 / 17905.0, 63121.0 / 35810.0, 423.0 / 17905.0],
        [11844.0 / 330415.0, -50337.0 / 660830.0, 316169.0 / 330415.0],
    ];
}

rgb_color_space!(DisplayP3, DisplayP3);
predefined_space!(DisplayP3, DISPLAY_P3_KEY);
predefined_rgb_profile!(DisplayP3);
