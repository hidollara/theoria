use super::*;

#[test]
fn rejects_data_without_a_font_face() {
    assert_eq!(
        Font::from_bytes(Vec::<u8>::new()).unwrap_err(),
        FontError::InvalidFace
    );
}

#[test]
fn standard_axis_tags_keep_their_open_type_identity() {
    assert_eq!(VariationAxisTag::WEIGHT.bytes(), *b"wght");
    assert_eq!(VariationAxisTag::WIDTH.bytes(), *b"wdth");
    assert_eq!(VariationAxisTag::OPTICAL_SIZE.bytes(), *b"opsz");
    assert_eq!(VariationAxisTag::SLANT.bytes(), *b"slnt");
    assert_eq!(VariationAxisTag::ITALIC.bytes(), *b"ital");
    assert_ne!(VariationAxisTag::WEIGHT, VariationAxisTag::WIDTH);
}

#[test]
fn one_normalized_location_drives_variable_glyph_realization() {
    let data: Arc<[u8]> = font_test_data::VAZIRMATN_VAR.into();
    let default = Font::from_bytes(data.clone()).unwrap();
    let weight = default
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag() == VariationAxisTag::WEIGHT)
        .expect("test variable font declares the standard weight axis");
    assert!(weight.minimum() <= weight.default());
    assert!(weight.default() <= weight.maximum());
    assert!(!weight.is_hidden());

    let heavy = default
        .clone()
        .with_variation(VariationAxisTag::WEIGHT, weight.maximum())
        .unwrap();
    let resolved_default = default.resolve();
    let resolved_heavy = heavy.resolve();

    assert_ne!(
        resolved_default.location().coords(),
        resolved_heavy.location().coords()
    );
    assert!((0..4096).map(GlyphId::new).any(|glyph| {
        matches!(
            (
                resolved_default.glyph_ref(glyph).outline(),
                resolved_heavy.glyph_ref(glyph).outline()
            ),
            (RealizedGlyph::Outline(default), RealizedGlyph::Outline(heavy)) if default != heavy
        )
    }));
}

#[test]
fn variation_settings_are_resolved_by_immutable_method_chaining() {
    let font = Font::from_bytes(font_test_data::VAZIRMATN_VAR).unwrap();
    let maximum = font
        .variation_axes()
        .into_iter()
        .find(|axis| axis.tag() == VariationAxisTag::WEIGHT)
        .unwrap()
        .maximum();
    assert_eq!(
        font.clone()
            .with_variation(VariationAxisTag::WEIGHT, f32::NAN)
            .unwrap_err(),
        FontError::NonFiniteVariation
    );

    let unknown = font
        .clone()
        .with_variation(VariationAxisTag::new(*b"TEST"), 1.0)
        .unwrap();
    assert_eq!(
        font.resolve().location().coords(),
        unknown.resolve().location().coords(),
        "variation settings absent from the selected face are ignored"
    );

    let varied = font
        .clone()
        .with_variation(VariationAxisTag::WEIGHT, 500.0)
        .unwrap()
        .with_variation(VariationAxisTag::WEIGHT, maximum)
        .unwrap();
    let expected = font
        .clone()
        .with_variation(VariationAxisTag::WEIGHT, maximum)
        .unwrap();
    assert_eq!(
        varied.resolve().location().coords(),
        expected.resolve().location().coords()
    );
    assert!(font.resolve().location().coords().is_empty());
}
