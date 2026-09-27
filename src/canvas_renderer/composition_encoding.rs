use crate::{
    canvas::{Composite, Composition},
    color::RgbColorSpace,
};

/// Encodes only the operation flags. Profile conversions are baked into the
/// selected pipeline rather than assigned fixed numeric space identifiers.
pub(crate) fn encode(composition: &Composition) -> [u32; 2] {
    let has_blend = u32::from(composition.blend().is_some());
    [has_blend, encode_composite(composition.composite())]
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
    fn profiles_are_selected_by_the_pipeline_not_encoded_as_numbers() {
        let composition = Composition::new(Composite::SourceIn, LinearDisplayP3)
            .with_blend(BlendMode::Screen, DisplayP3);
        assert_eq!(encode(&composition), [1, 5]);
    }
}
