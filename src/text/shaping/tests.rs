use super::*;
use crate::{
    font::GlyphId,
    geometry::Offset,
    text::{
        MissingGlyphPolicy, OpenTypeFeatureTag, ShapingOptions,
        shaping::ShapingDirection,
        test_support::{bundled_latin_font, windows_test_font, windows_vertical_fonts},
    },
};

#[cfg(target_os = "windows")]
#[test]
fn top_to_bottom_shaping_selects_vertical_punctuation_forms() {
    let Some(fonts) = windows_vertical_fonts() else {
        return;
    };
    let font = fonts[0].clone();
    let horizontal = GlyphRun::shape(font.clone(), "「」").unwrap();
    let vertical = GlyphRun::shape_with(
        font,
        "「」",
        &ShapingOptions::default().exact_direction(ShapingDirection::TopToBottom),
    )
    .unwrap();

    let horizontal_ids: Vec<_> = horizontal
        .glyphs
        .iter()
        .map(|glyph| glyph.glyph_id)
        .collect();
    let vertical_ids: Vec<_> = vertical.glyphs.iter().map(|glyph| glyph.glyph_id).collect();
    assert_ne!(horizontal_ids, vertical_ids);
    // HarfRust reports vertical advances in the font's design-unit convention.
    // Canvas-space Y conversion belongs to layout, not shaping.
    assert!(vertical.advance().y < 0.0);
    assert!(
        vertical
            .glyphs
            .iter()
            .all(|glyph| glyph.advance.x == 0.0 && glyph.advance.y < 0.0)
    );
    assert!(
        vertical
            .glyphs
            .iter()
            .any(|glyph| glyph.offset != Offset::ZERO)
    );
}
#[test]
fn rejects_non_finite_shaped_positions() {
    let glyph = ShapedGlyph::new(
        GlyphId::new(1),
        0,
        Offset::ZERO,
        Offset::new(500.0, 0.0),
        false,
    );
    assert_eq!(validate_placements(&[glyph]), Ok(()));
    assert_eq!(
        validate_placements(&[ShapedGlyph::new(
            GlyphId::new(1),
            0,
            Offset::new(f32::INFINITY, 0.0),
            Offset::ZERO,
            false,
        )]),
        Err(GlyphRunError::NonFinitePlacement)
    );
}

#[cfg(target_os = "windows")]
#[test]
fn shaping_can_disable_the_standard_ligature_feature() {
    let candidates = [
        r"C:\Windows\Fonts\segoeui.ttf",
        r"C:\Windows\Fonts\calibri.ttf",
        r"C:\Windows\Fonts\times.ttf",
        r"C:\Windows\Fonts\cambria.ttc",
        r"C:\Windows\Fonts\georgia.ttf",
    ];
    let found = candidates.into_iter().find_map(|path| {
        let font = Font::from_bytes(std::fs::read(path).ok()?).ok()?;
        let default = GlyphRun::shape(font.clone(), "ffi").ok()?;
        let without_ligatures = GlyphRun::shape_with(
            font,
            "ffi",
            &ShapingOptions::new().feature(OpenTypeFeatureTag::STANDARD_LIGATURES.disabled()),
        )
        .ok()?;
        (default.glyph_count() < without_ligatures.glyph_count()).then_some(without_ligatures)
    });

    let Some(without_ligatures) = found else {
        return;
    };
    assert_eq!(without_ligatures.glyph_count(), 3);
}

#[test]
fn spaces_advance_without_requiring_an_outline() {
    let font = bundled_latin_font();
    let run = GlyphRun::shape(font, " ").unwrap();

    assert_eq!(run.glyph_count(), 1);
    assert!(run.advance().x > 0.0);
    assert_eq!(run.advance().y, 0.0);
}

#[cfg(target_os = "windows")]
#[test]
fn clusters_are_utf8_byte_offsets_in_the_input_segment() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let run = GlyphRun::shape(font, "Aé").unwrap();

    assert_eq!(run.clusters().collect::<Vec<_>>(), vec![0, 1]);
}

#[cfg(target_os = "windows")]
#[test]
fn shaping_uses_the_fonts_selected_variation_location() {
    let Ok(data) = std::fs::read(r"C:\Windows\Fonts\SegUIVar.ttf") else {
        return;
    };
    let data: std::sync::Arc<[u8]> = data.into();
    let default = Font::from_bytes(data.clone()).unwrap();
    let heavy = default
        .clone()
        .with_variation(crate::font::VariationAxisTag::WEIGHT, 700.0)
        .unwrap();
    let default_run = GlyphRun::shape(default, "Theoria").unwrap();
    let heavy_run = GlyphRun::shape(heavy, "Theoria").unwrap();

    assert_ne!(default_run.advance(), heavy_run.advance());
}

#[cfg(target_os = "windows")]
#[test]
fn fallback_selects_fonts_without_splitting_grapheme_clusters() {
    let Some(primary) = windows_test_font() else {
        return;
    };
    let Ok(fallback_data) = std::fs::read(r"C:\Windows\Fonts\meiryo.ttc") else {
        return;
    };
    let fallback = Font::from_bytes(fallback_data).unwrap();

    let fonts = [primary, fallback];
    let primary_only =
        shape_directional_segment(&fonts, "Theoria", &ShapingOptions::default(), 0).unwrap();
    assert_eq!(primary_only.len(), 1);

    let runs = shape_directional_segment(&fonts, "Á日B", &ShapingOptions::default(), 0).unwrap();
    assert_eq!(runs.len(), 3);
}

#[cfg(target_os = "windows")]
#[test]
fn fallback_reports_missing_clusters() {
    let Some(font) = windows_test_font() else {
        return;
    };

    assert_eq!(
        shape_directional_segment(
            std::slice::from_ref(&font),
            "A\u{10ffff}",
            &ShapingOptions::default().missing_glyph_policy(MissingGlyphPolicy::Error),
            0,
        )
        .unwrap_err(),
        ShapingError::MissingGlyph { byte_index: 1 }
    );
}

#[cfg(target_os = "windows")]
#[test]
fn fallback_replaces_missing_clusters_without_losing_their_logical_range() {
    let Some(font) = windows_test_font() else {
        return;
    };

    let runs =
        shape_directional_segment(&[font], "A\u{10ffff}B", &ShapingOptions::default(), 0).unwrap();
    let replacement = runs
        .iter()
        .find(|run| run.source_range == (1..5))
        .expect("the unsupported grapheme remains a distinct logical run");
    assert!(replacement.glyphs.iter().all(|glyph| glyph.cluster == 1));
}

#[cfg(target_os = "windows")]
#[test]
fn replacement_character_uses_the_first_supporting_fallback_font() {
    let primary = bundled_latin_font();
    let Some(fallback) = windows_test_font() else {
        return;
    };
    if primary.resolve().has_nominal_glyph('\u{fffd}')
        || !fallback.resolve().has_nominal_glyph('\u{fffd}')
    {
        return;
    }

    let runs = shape_directional_segment(
        &[primary, fallback.clone()],
        "\u{10ffff}",
        &ShapingOptions::default(),
        0,
    )
    .unwrap();

    assert_eq!(runs.len(), 1);
    assert!(runs[0].font.same_instance(&fallback.resolve()));
    assert_eq!(runs[0].source_range, 0..4);
    assert!(
        runs[0]
            .glyphs
            .iter()
            .all(|glyph| glyph.glyph_id.to_u32() != 0)
    );
}

#[cfg(target_os = "windows")]
#[test]
fn arabic_shaping_preserves_safe_tatweel_insertion_points() {
    let Some(font) = windows_test_font() else {
        return;
    };
    let runs = shape_directional_segment(
        &[font],
        "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}",
        &ShapingOptions::default(),
        0,
    )
    .unwrap();

    let run = &runs[0];
    let positions = run.tatweel_insertion_positions();
    assert!(!positions.is_empty());

    let elongated = run
        .with_tatweels("\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}", &positions[..1])
        .unwrap();
    assert!(elongated.glyphs.len() > run.glyphs.len());
    assert_eq!(elongated.source_range, run.source_range);
    assert!(elongated.glyphs.iter().all(|glyph| {
        glyph.cluster <= "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}".len()
            && "\u{0645}\u{0631}\u{062d}\u{0628}\u{0627}".is_char_boundary(glyph.cluster)
    }));
}

#[test]
fn backward_directions_are_identified_for_run_ordering() {
    assert!(!direction_is_backward(ShapingDirection::LeftToRight));
    assert!(direction_is_backward(ShapingDirection::RightToLeft));
    assert!(!direction_is_backward(ShapingDirection::TopToBottom));
    assert!(direction_is_backward(ShapingDirection::BottomToTop));
}

#[test]
fn script_itemization_keeps_neutral_text_with_a_neighboring_script() {
    assert_eq!(
        script_segments("A\u{0301} \u{65e5}\u{672c}"),
        [
            ScriptSegment {
                range: 0..4,
                script: Some(*b"Latn"),
            },
            ScriptSegment {
                range: 4..10,
                script: Some(*b"Hani"),
            },
        ]
    );
    assert_eq!(
        script_segments("\u{300c}\u{65e5}\u{672c}"),
        [ScriptSegment {
            range: 0..9,
            script: Some(*b"Hani"),
        }]
    );
    assert_eq!(
        script_segments(" 123 "),
        [ScriptSegment {
            range: 0..5,
            script: None,
        }]
    );
}

#[test]
fn script_itemization_uses_extensions_and_keeps_bracket_pairs_with_the_outer_script() {
    assert_eq!(
        script_segments("\u{11305}\u{0beb}\u{11306}"),
        [ScriptSegment {
            range: 0..11,
            script: Some(*b"Gran"),
        }]
    );

    let segments = script_segments("a(\u{03b1})b");
    assert_eq!(
        segments
            .iter()
            .map(|segment| segment.script)
            .collect::<Vec<_>>(),
        [Some(*b"Latn"), Some(*b"Grek"), Some(*b"Latn")]
    );
    assert_eq!(segments[0].range, 0..2);
    assert_eq!(segments[2].range, 4..6);
}

#[cfg(target_os = "windows")]
#[test]
fn a_text_line_shapes_distinct_scripts_as_distinct_runs() {
    let Some(font) = windows_test_font() else {
        return;
    };

    let runs =
        shape_directional_segment(&[font], "abc\u{0416}", &ShapingOptions::default(), 0).unwrap();

    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].script(), Some(*b"Latn"));
    assert_eq!(runs[1].script(), Some(*b"Cyrl"));
}
