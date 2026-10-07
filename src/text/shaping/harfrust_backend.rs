use harfrust::{
    BufferFlags, Direction, Feature, GlyphInfo, GlyphPosition, Language, Script,
    ShapeOptions as HarfRustShapeOptions, ShaperData, ShaperInstance, Tag, UnicodeBuffer, script,
};

use crate::{
    font::{GlyphId, ResolvedFont},
    geometry::Offset,
};

use super::{ShapedGlyph, ShapingDirection, ShapingOptions};

pub(super) fn shape_glyphs(
    font: &ResolvedFont,
    text: &str,
    options: &ShapingOptions,
    context: Option<(&str, std::ops::Range<usize>)>,
) -> Vec<ShapedGlyph> {
    let face = font.face();
    let instance = ShaperInstance::from_coords(&face, font.location().coords().iter().copied());
    let shaper_data = ShaperData::new(&face);
    let shaper = shaper_data.shaper(&face).instance(Some(&instance)).build();
    let buffer = configured_buffer(text, options, context);
    let features: Vec<_> = options
        .features
        .iter()
        .copied()
        .map(to_harfrust_feature)
        .collect();
    let shaped = shaper.shape(buffer, HarfRustShapeOptions::default().features(&features));
    positioned_glyphs(shaped.glyph_infos(), shaped.glyph_positions())
}

fn to_harfrust_feature(feature: crate::text::OpenTypeFeature) -> Feature {
    Feature::new(Tag::new(&feature.tag().bytes()), feature.value(), ..)
}

pub(super) fn resolved_direction(text: &str, options: &ShapingOptions) -> ShapingDirection {
    from_harfrust_direction(configured_buffer(text, options, None).direction())
}

fn configured_buffer(
    text: &str,
    options: &ShapingOptions,
    context: Option<(&str, std::ops::Range<usize>)>,
) -> UnicodeBuffer {
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    if let Some((surrounding, range)) = context {
        debug_assert_eq!(surrounding.get(range.clone()), Some(text));
        buffer.set_pre_context(&surrounding[..range.start]);
        buffer.set_post_context(&surrounding[range.end..]);
        if range.start == 0 {
            buffer.set_flags(buffer.flags() | BufferFlags::BEGINNING_OF_TEXT);
        }
        if range.end == surrounding.len() {
            buffer.set_flags(buffer.flags() | BufferFlags::END_OF_TEXT);
        }
    } else {
        buffer.set_flags(BufferFlags::BEGINNING_OF_TEXT | BufferFlags::END_OF_TEXT);
    }
    apply_segment_properties(&mut buffer, options);
    buffer.guess_segment_properties();
    if uses_cursive_elongation(buffer.script()) {
        buffer.set_flags(buffer.flags() | BufferFlags::PRODUCE_SAFE_TO_INSERT_TATWEEL);
    }
    buffer
}

fn uses_cursive_elongation(script: Script) -> bool {
    matches!(
        script,
        script::ARABIC
            | script::HANIFI_ROHINGYA
            | script::MANDAIC
            | script::MONGOLIAN
            | script::NKO
            | script::PHAGS_PA
            | script::SYRIAC
    )
}

fn apply_segment_properties(buffer: &mut UnicodeBuffer, options: &ShapingOptions) {
    if let Some(direction) = options.direction {
        buffer.set_direction(to_harfrust_direction(direction));
    }
    if let Some(tag) = options.script
        && let Some(script) = Script::from_iso15924_tag(Tag::new(&tag))
    {
        buffer.set_script(script);
    }
    if let Some(language) = options.language.as_deref().and_then(Language::new) {
        buffer.set_language(language);
    }
}

fn positioned_glyphs(infos: &[GlyphInfo], positions: &[GlyphPosition]) -> Vec<ShapedGlyph> {
    debug_assert_eq!(infos.len(), positions.len());
    infos
        .iter()
        .zip(positions)
        .map(|(info, position)| {
            ShapedGlyph::new(
                GlyphId::new(info.glyph_id),
                info.cluster as usize,
                Offset::new(position.x_offset as f32, position.y_offset as f32),
                Offset::new(position.x_advance as f32, position.y_advance as f32),
                info.safe_to_insert_tatweel(),
            )
        })
        .collect()
}

const fn to_harfrust_direction(direction: ShapingDirection) -> Direction {
    match direction {
        ShapingDirection::LeftToRight => Direction::LeftToRight,
        ShapingDirection::RightToLeft => Direction::RightToLeft,
        ShapingDirection::TopToBottom => Direction::TopToBottom,
        ShapingDirection::BottomToTop => Direction::BottomToTop,
    }
}

const fn from_harfrust_direction(direction: Direction) -> ShapingDirection {
    match direction {
        Direction::LeftToRight => ShapingDirection::LeftToRight,
        Direction::RightToLeft => ShapingDirection::RightToLeft,
        Direction::TopToBottom => ShapingDirection::TopToBottom,
        Direction::BottomToTop => ShapingDirection::BottomToTop,
        _ => ShapingDirection::LeftToRight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{HorizontalDirection, OpenTypeFeatureTag};

    #[test]
    fn transfers_glyph_identity_cluster_and_position_without_reinterpretation() {
        let mut info = GlyphInfo::default();
        info.glyph_id = 42;
        info.cluster = 7;
        let mut position = GlyphPosition::default();
        position.x_advance = 600;
        position.y_advance = -20;
        position.x_offset = 15;
        position.y_offset = 30;

        let glyphs = positioned_glyphs(&[info], &[position]);
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].glyph_id, GlyphId::new(42));
        assert_eq!(glyphs[0].cluster, 7);
        assert_eq!(glyphs[0].offset, Offset::new(15.0, 30.0));
        assert_eq!(glyphs[0].advance, Offset::new(600.0, -20.0));
    }

    #[test]
    fn semantic_segment_properties_are_converted_at_the_backend_boundary() {
        let options = ShapingOptions::new()
            .direction(HorizontalDirection::RightToLeft)
            .script(*b"Arab")
            .language("AR_eg");
        let mut buffer = UnicodeBuffer::new();
        buffer.push_str("abc");

        apply_segment_properties(&mut buffer, &options);
        buffer.guess_segment_properties();

        assert_eq!(buffer.direction(), Direction::RightToLeft);
        assert_eq!(buffer.script().tag(), Tag::new(b"Arab"));
        assert_eq!(
            buffer.language().as_ref().map(Language::as_str),
            Some("ar-eg")
        );
    }

    #[test]
    fn semantic_feature_identity_is_preserved_during_conversion() {
        let feature = to_harfrust_feature(OpenTypeFeatureTag::STANDARD_LIGATURES.disabled());

        assert_eq!(feature.tag, Tag::new(b"liga"));
        assert_eq!(feature.value, 0);
        assert_eq!(feature.start, 0);
        assert_eq!(feature.end, u32::MAX);
    }
}
