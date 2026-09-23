use crate::{
    canvas::{BlendMode, Composite, Composition},
    color::RgbColorSpace,
};

/// Encodes a Canvas composition for the shared Vulkan/WebGPU shader.
pub(crate) fn encode(composition: Composition) -> [u32; 4] {
    let (blend_mode, blend_space) = composition
        .blend()
        .map(|blend| (encode_blend(blend.mode()), encode_space(blend.space())))
        .unwrap_or((0, 0));
    [
        blend_mode,
        encode_composite(composition.composite()),
        blend_space,
        encode_space(composition.composite_space()),
    ]
}

fn encode_blend(mode: BlendMode) -> u32 {
    match mode {
        BlendMode::Normal => 0,
        BlendMode::Multiply => 1,
        BlendMode::Screen => 2,
        BlendMode::Overlay => 3,
        BlendMode::Darken => 4,
        BlendMode::Lighten => 5,
        BlendMode::ColorDodge => 6,
        BlendMode::ColorBurn => 7,
        BlendMode::HardLight => 8,
        BlendMode::SoftLight => 9,
        BlendMode::Difference => 10,
        BlendMode::Exclusion => 11,
        BlendMode::Hue => 12,
        BlendMode::Saturation => 13,
        BlendMode::Color => 14,
        BlendMode::Luminosity => 15,
    }
}

fn encode_composite(composite: Composite) -> u32 {
    match composite {
        Composite::Clear => 0,
        Composite::Copy => 1,
        Composite::Destination => 2,
        Composite::SourceOver => 3,
        Composite::DestinationOver => 4,
        Composite::SourceIn => 5,
        Composite::DestinationIn => 6,
        Composite::SourceOut => 7,
        Composite::DestinationOut => 8,
        Composite::SourceAtop => 9,
        Composite::DestinationAtop => 10,
        Composite::Xor => 11,
        Composite::Plus => 12,
    }
}

fn encode_space(space: RgbColorSpace) -> u32 {
    match space {
        RgbColorSpace::Srgb => 0,
        RgbColorSpace::LinearSrgb => 1,
        RgbColorSpace::DisplayP3 => 2,
        RgbColorSpace::LinearDisplayP3 => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

    #[test]
    fn every_w3c_blend_mode_has_a_stable_unique_shader_identity() {
        for (expected, mode) in BlendMode::ALL.into_iter().enumerate() {
            let composition =
                Composition::new(Composite::SourceOver, LinearSrgb).with_blend(mode, Srgb);
            assert_eq!(encode(composition)[0], expected as u32);
        }
    }

    #[test]
    fn blend_and_composite_spaces_are_encoded_independently() {
        let composition = Composition::new(Composite::SourceIn, LinearDisplayP3)
            .with_blend(BlendMode::Screen, DisplayP3);
        assert_eq!(encode(composition), [2, 5, 2, 3]);
    }
}
