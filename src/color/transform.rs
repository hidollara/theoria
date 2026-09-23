use super::profile_data::{
    SRGB_DECODE_THRESHOLD, SRGB_ENCODE_THRESHOLD, SRGB_GAMMA, SRGB_LINEAR_SCALE, SRGB_POWER_OFFSET,
    SRGB_POWER_SCALE,
};

/// The transfer curve used by a matrix-based RGB profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RgbTransfer {
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

    fn decode_f64(self, value: f64) -> f64 {
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

    fn encode_f64(self, value: f64) -> f64 {
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

/// A matrix/transfer-curve RGB conversion through XYZ D65.
///
/// The matrices describe color meaning, not a renderer's arithmetic precision
/// or pixel storage. This is the first, matrix-based profile capability; it is
/// not a general ICC profile connection-space representation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RgbTransform {
    transfer: RgbTransfer,
    to_xyz_d65: [[f64; 3]; 3],
    from_xyz_d65: [[f64; 3]; 3],
}

impl RgbTransform {
    /// Describes both directions of a profile's XYZ D65 conversion.
    ///
    /// The caller must supply mutually inverse matrices for the linear RGB
    /// coordinates. No color conversion or validation occurs at construction.
    pub const fn new(
        transfer: RgbTransfer,
        to_xyz_d65: [[f64; 3]; 3],
        from_xyz_d65: [[f64; 3]; 3],
    ) -> Self {
        Self {
            transfer,
            to_xyz_d65,
            from_xyz_d65,
        }
    }

    pub const fn transfer(self) -> RgbTransfer {
        self.transfer
    }

    pub const fn to_xyz_d65_matrix(self) -> [[f64; 3]; 3] {
        self.to_xyz_d65
    }

    pub const fn from_xyz_d65_matrix(self) -> [[f64; 3]; 3] {
        self.from_xyz_d65
    }

    pub(crate) fn decode(self, rgb: [f32; 3]) -> [f32; 3] {
        rgb.map(|value| self.transfer.decode(value))
    }

    pub(crate) fn encode(self, rgb: [f32; 3]) -> [f32; 3] {
        rgb.map(|value| self.transfer.encode(value))
    }

    pub(crate) fn rgb_to_xyz_d65(self, rgb: [f32; 3]) -> [f32; 3] {
        multiply(self.to_xyz_d65, self.decode(rgb))
    }

    pub(crate) fn xyz_d65_to_rgb(self, xyz: [f32; 3]) -> [f32; 3] {
        self.encode(multiply(self.from_xyz_d65, xyz))
    }

    pub fn rgb_to_xyz_d65_f64(self, rgb: [f64; 3]) -> [f64; 3] {
        multiply_f64(
            self.to_xyz_d65,
            rgb.map(|value| self.transfer.decode_f64(value)),
        )
    }

    pub fn xyz_d65_to_rgb_f64(self, xyz: [f64; 3]) -> [f64; 3] {
        multiply_f64(self.from_xyz_d65, xyz).map(|value| self.transfer.encode_f64(value))
    }
}

fn multiply(matrix: [[f64; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| {
        row[0] as f32 * vector[0] + row[1] as f32 * vector[1] + row[2] as f32 * vector[2]
    })
}

pub(crate) fn multiply_f64(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    matrix.map(|row| row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2])
}
