use crate::{
    canvas::{Composite, Composition},
    color::RgbColorSpace,
};

/// Encodes a Canvas composition for the shared Vulkan/WebGPU shader.
pub(crate) fn encode(composition: &Composition) -> [u32; 4] {
    let (has_blend, blend_space) = composition
        .blend()
        .map(|blend| (1, encode_space(blend.space())))
        .unwrap_or((0, 0));
    [
        has_blend,
        encode_composite(composition.composite()),
        blend_space,
        encode_space(composition.composite_space()),
    ]
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

pub(crate) fn encode_space(space: RgbColorSpace) -> u32 {
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
    use crate::canvas::BlendMode;
    use crate::color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

    #[test]
    fn blend_presence_is_encoded_independently_of_its_implementation() {
        for mode in BlendMode::ALL {
            let composition =
                Composition::new(Composite::SourceOver, LinearSrgb).with_blend(mode, Srgb);
            assert_eq!(
                encode(&composition)[0],
                u32::from(mode != BlendMode::Normal)
            );
        }
    }

    #[test]
    fn blend_and_composite_spaces_are_encoded_independently() {
        let composition = Composition::new(Composite::SourceIn, LinearDisplayP3)
            .with_blend(BlendMode::Screen, DisplayP3);
        assert_eq!(encode(&composition), [1, 5, 2, 3]);
    }
}
