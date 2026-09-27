use super::transform;

/// Encodes a linear sRGB component using the standard sRGB transfer curve.
pub fn encode_srgb(value: f32) -> f32 {
    transform::encode_srgb(value)
}
