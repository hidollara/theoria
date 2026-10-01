/// A three-channel color-coordinate operation that cannot be expressed as
/// independent curves and affine matrices.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorFunction {
    RgbToHsl,
    HslToRgb,
    RgbToHwb,
    HwbToRgb,
    CartesianToPolar,
    PolarToCartesian,
}

impl ColorFunction {
    pub const fn input_channels(self) -> usize {
        self.channel_counts().0
    }

    pub const fn output_channels(self) -> usize {
        self.channel_counts().1
    }

    const fn channel_counts(self) -> (usize, usize) {
        match self {
            Self::RgbToHsl
            | Self::HslToRgb
            | Self::RgbToHwb
            | Self::HwbToRgb
            | Self::CartesianToPolar
            | Self::PolarToCartesian => (3, 3),
        }
    }

    pub(super) fn evaluate(self, [first, second, third]: [f64; 3]) -> [f64; 3] {
        match self {
            Self::RgbToHsl => rgb_to_hsl([first, second, third]),
            Self::HslToRgb => hsl_to_rgb([first, second, third]),
            Self::RgbToHwb => rgb_to_hwb([first, second, third]),
            Self::HwbToRgb => hwb_to_rgb([first, second, third]),
            Self::CartesianToPolar => {
                let chroma = second.hypot(third);
                let hue = if chroma == 0.0 {
                    0.0
                } else {
                    third.atan2(second).to_degrees().rem_euclid(360.0)
                };
                [first, chroma, hue]
            }
            Self::PolarToCartesian => {
                let angle = third.to_radians();
                [first, second * angle.cos(), second * angle.sin()]
            }
        }
    }
}

fn rgb_hue([red, green, blue]: [f64; 3]) -> f64 {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;
    if delta == 0.0 {
        return 0.0;
    }
    let sector = if max == red {
        (green - blue) / delta
    } else if max == green {
        (blue - red) / delta + 2.0
    } else {
        (red - green) / delta + 4.0
    };
    (sector * 60.0).rem_euclid(360.0)
}

fn rgb_to_hsl(rgb @ [red, green, blue]: [f64; 3]) -> [f64; 3] {
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let light = (min + max) / 2.0;
    let delta = max - min;
    let mut saturation = if delta == 0.0 || light == 0.0 || light == 1.0 {
        0.0
    } else {
        (max - light) / light.min(1.0 - light)
    };
    let mut hue = rgb_hue(rgb);
    if saturation < 0.0 {
        hue = (hue + 180.0).rem_euclid(360.0);
        saturation = -saturation;
    }
    [hue, saturation * 100.0, light * 100.0]
}

fn hsl_to_rgb([hue, saturation, lightness]: [f64; 3]) -> [f64; 3] {
    let saturation = saturation / 100.0;
    let lightness = lightness / 100.0;
    let amplitude = saturation * lightness.min(1.0 - lightness);
    [0.0, 8.0, 4.0].map(|offset| {
        let k = (offset + hue / 30.0_f64).rem_euclid(12.0_f64);
        lightness - amplitude * (-1.0_f64).max((k - 3.0).min(9.0 - k).min(1.0))
    })
}

fn rgb_to_hwb(rgb @ [red, green, blue]: [f64; 3]) -> [f64; 3] {
    let white = red.min(green).min(blue);
    let black = 1.0 - red.max(green).max(blue);
    [rgb_hue(rgb), white * 100.0, black * 100.0]
}

fn hwb_to_rgb([hue, whiteness, blackness]: [f64; 3]) -> [f64; 3] {
    let white = whiteness / 100.0;
    let black = blackness / 100.0;
    if white + black >= 1.0 {
        return [white / (white + black); 3];
    }
    hsl_to_rgb([hue, 100.0, 50.0]).map(|channel| channel * (1.0 - white - black) + white)
}
