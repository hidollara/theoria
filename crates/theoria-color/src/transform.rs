// Shared by CPU evaluation and shader generation of the sRGB transfer curve.
pub(crate) const SRGB_DECODE_THRESHOLD: f64 = 0.04045;
pub(crate) const SRGB_ENCODE_THRESHOLD: f64 = 0.003_130_8;
pub(crate) const SRGB_LINEAR_SCALE: f64 = 12.92;
pub(crate) const SRGB_POWER_SCALE: f64 = 1.055;
pub(crate) const SRGB_POWER_OFFSET: f64 = 0.055;
pub(crate) const SRGB_GAMMA: f64 = 2.4;

/// The standard sRGB curve or a linear-light identity curve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RgbTransfer {
    Linear,
    Srgb,
}

impl RgbTransfer {
    pub(crate) fn decode(self, value: f32) -> f32 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                let magnitude = value.abs();
                if magnitude <= SRGB_DECODE_THRESHOLD as f32 {
                    value / SRGB_LINEAR_SCALE as f32
                } else {
                    value.signum()
                        * ((magnitude + SRGB_POWER_OFFSET as f32) / SRGB_POWER_SCALE as f32)
                            .powf(SRGB_GAMMA as f32)
                }
            }
        }
    }

    pub(crate) fn encode(self, value: f32) -> f32 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                let magnitude = value.abs();
                if magnitude <= SRGB_ENCODE_THRESHOLD as f32 {
                    SRGB_LINEAR_SCALE as f32 * value
                } else {
                    value.signum()
                        * (SRGB_POWER_SCALE as f32 * magnitude.powf(1.0 / SRGB_GAMMA as f32)
                            - SRGB_POWER_OFFSET as f32)
                }
            }
        }
    }

    pub(crate) fn decode_f64(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                let magnitude = value.abs();
                if magnitude <= SRGB_DECODE_THRESHOLD {
                    value / SRGB_LINEAR_SCALE
                } else {
                    value.signum()
                        * ((magnitude + SRGB_POWER_OFFSET) / SRGB_POWER_SCALE).powf(SRGB_GAMMA)
                }
            }
        }
    }

    pub(crate) fn encode_f64(self, value: f64) -> f64 {
        match self {
            Self::Linear => value,
            Self::Srgb => {
                let magnitude = value.abs();
                if magnitude <= SRGB_ENCODE_THRESHOLD {
                    SRGB_LINEAR_SCALE * value
                } else {
                    value.signum()
                        * (SRGB_POWER_SCALE * magnitude.powf(1.0 / SRGB_GAMMA) - SRGB_POWER_OFFSET)
                }
            }
        }
    }
}

pub(crate) fn multiply_f64(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}
