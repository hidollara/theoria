use theoria_canvas::canvas::{Composite, Composition};
use theoria_color::{ColorSpace, DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

/// Encodes only the operation flags. Space conversions are baked into the
/// selected pipeline rather than assigned fixed numeric space identifiers.
pub fn encode(composition: &Composition) -> [u32; 2] {
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

/// Raster upload uses four named input spaces; cached Canvas images are
/// already in the renderer's working space (index 4).
pub fn encode_image_space(space: &ColorSpace, working: &ColorSpace) -> u32 {
    if space == working {
        return 4;
    }
    if space == &Srgb.into() {
        0
    } else if space == &LinearSrgb.into() {
        1
    } else if space == &DisplayP3.into() {
        2
    } else if space == &LinearDisplayP3.into() {
        3
    } else {
        unreachable!("RGB raster images have one of the four named spaces")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use theoria_canvas::canvas::BlendMode;
    use theoria_color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb};

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
    fn spaces_are_selected_by_the_pipeline_not_encoded_as_numbers() {
        let composition = Composition::new(Composite::SourceIn, LinearDisplayP3)
            .with_blend(BlendMode::Screen, DisplayP3);
        assert_eq!(encode(&composition), [1, 5]);
    }
}
