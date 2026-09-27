use crate::{
    canvas::{Composite, Composition},
    color::{ColorProfile, DisplayP3, LinearDisplayP3, LinearSrgb, Srgb},
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

/// Raster upload uses four named input profiles; cached Canvas images are
/// already in the renderer's working profile (index 4).
pub(crate) fn encode_image_profile(profile: &ColorProfile, working: &ColorProfile) -> u32 {
    if profile == working {
        return 4;
    }
    if profile == &Srgb.into() {
        0
    } else if profile == &LinearSrgb.into() {
        1
    } else if profile == &DisplayP3.into() {
        2
    } else if profile == &LinearDisplayP3.into() {
        3
    } else {
        unreachable!("RGB raster images have one of the four named profiles")
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
