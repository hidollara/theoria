use std::io::Read;

use resvg::{tiny_skia, usvg};
use skrifa::{GlyphId, MetadataProvider};

use crate::{
    canvas::{Canvas, ColorField, Coverage, ImagePattern, ImageSampling},
    image::Srgba8Image,
    math::{Matrix2, Vector2},
    transform::Affine2,
};

use super::{GlyphPresentationError, ResolvedFont};

const MAX_RASTER_DIMENSION: f32 = 4096.0;

/// Realizes one OpenType SVG glyph as a logical Image-backed Canvas.
///
/// OpenType SVG coordinates are y-down design units. The returned Canvas uses
/// the font layer's y-up design coordinates so that all glyph presentations
/// share the same placement transform in text realization.
pub(super) fn canvas(
    font: &ResolvedFont,
    data: &[u8],
    glyph_id: GlyphId,
    font_size: f32,
    palette_index: u16,
    foreground: &ColorField,
) -> Result<Canvas, GlyphPresentationError> {
    let units_per_em = font.metrics().units_per_em();
    if font_size == 0.0 {
        return Ok(Canvas::empty());
    }
    if !font_size.is_finite() || font_size < 0.0 || units_per_em == 0 {
        return Err(GlyphPresentationError::UnsupportedSvg);
    }

    let document = prepared_document(font, data, palette_index)?;
    let foreground_color = match foreground {
        ColorField::Solid(color) => Some(css_rgba_f32(color.to_straight_srgba())),
        _ => None,
    };
    if contains_ascii_case_insensitive(&document, "currentColor") && foreground_color.is_none() {
        return Err(GlyphPresentationError::UnsupportedSvg);
    }
    let options = usvg::Options {
        default_size: usvg::Size::from_wh(f32::from(units_per_em), f32::from(units_per_em))
            .ok_or(GlyphPresentationError::UnsupportedSvg)?,
        style_sheet: foreground_color.map(|color| format!("svg {{ color: {color}; }}")),
        ..Default::default()
    };
    let tree = usvg::Tree::from_str(&document, &options)
        .map_err(|_| GlyphPresentationError::UnsupportedSvg)?;
    let glyph_identifier = format!("glyph{}", glyph_id.to_u32());
    let node = tree.node_by_id(&glyph_identifier);
    if node.is_none() && has_glyph_child(tree.root()) {
        return Err(GlyphPresentationError::UnsupportedSvg);
    }
    let bounds = match node {
        Some(node) => node
            .abs_layer_bounding_box()
            .ok_or(GlyphPresentationError::UnsupportedSvg)?,
        // usvg does not retain the root <svg> element's id. A document with
        // no child glyph identifiers is the single-glyph root form allowed by
        // OpenType, so its root group is the glyph description.
        None => tree.root().abs_layer_bounding_box(),
    };

    let requested_scale = font_size / f32::from(units_per_em);
    let raster_scale = requested_scale
        .min(MAX_RASTER_DIMENSION / bounds.width())
        .min(MAX_RASTER_DIMENSION / bounds.height());
    if !raster_scale.is_finite() || raster_scale <= 0.0 {
        return Err(GlyphPresentationError::UnsupportedSvg);
    }
    let width = raster_dimension(bounds.width(), raster_scale)?;
    let height = raster_dimension(bounds.height(), raster_scale)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or(GlyphPresentationError::UnsupportedSvg)?;
    let transform = tiny_skia::Transform::from_scale(raster_scale, raster_scale);
    match node {
        Some(node) => resvg::render_node(node, transform, &mut pixmap.as_mut())
            .ok_or(GlyphPresentationError::UnsupportedSvg)?,
        None => resvg::render(
            &tree,
            transform.pre_translate(-bounds.x(), -bounds.y()),
            &mut pixmap.as_mut(),
        ),
    }

    let image = Srgba8Image::new(width, height, pixmap.take_demultiplied())
        .map_err(|_| GlyphPresentationError::UnsupportedSvg)?;
    let pixel_to_design = raster_scale.recip();
    let image_to_font = Affine2::new(
        Matrix2::from_columns(
            Vector2::new(pixel_to_design, 0.0),
            Vector2::new(0.0, -pixel_to_design),
        ),
        Vector2::new(bounds.x(), -bounds.y()),
    );
    let pattern = ImagePattern::placed(image, image_to_font)
        .ok_or(GlyphPresentationError::UnsupportedSvg)?
        .with_sampling(ImageSampling::Linear);
    Ok(Canvas::draw(pattern, Coverage::all()))
}

fn prepared_document(
    font: &ResolvedFont,
    data: &[u8],
    palette_index: u16,
) -> Result<String, GlyphPresentationError> {
    const MAX_DOCUMENT_SIZE: u64 = 16 * 1024 * 1024;
    let bytes = if data.starts_with(&[0x1f, 0x8b]) {
        let mut decoded = Vec::new();
        flate2::read::GzDecoder::new(data)
            .take(MAX_DOCUMENT_SIZE + 1)
            .read_to_end(&mut decoded)
            .map_err(|_| GlyphPresentationError::UnsupportedSvg)?;
        if decoded.len() as u64 > MAX_DOCUMENT_SIZE {
            return Err(GlyphPresentationError::UnsupportedSvg);
        }
        decoded
    } else {
        data.to_vec()
    };
    let document =
        std::str::from_utf8(&bytes).map_err(|_| GlyphPresentationError::UnsupportedSvg)?;
    let face = font.face();
    let palettes = face.color_palettes();
    let palette = palettes.get(palette_index).or_else(|| palettes.get(0));
    replace_palette_variables(document, |index| {
        palette
            .as_ref()
            .and_then(|palette| palette.colors().get(index))
            .map(|color| css_rgba([color.red(), color.green(), color.blue(), color.alpha()]))
    })
}

fn replace_palette_variables(
    document: &str,
    mut color: impl FnMut(usize) -> Option<String>,
) -> Result<String, GlyphPresentationError> {
    let mut output = String::with_capacity(document.len());
    let mut cursor = 0;
    while let Some(relative) = document[cursor..].find("var(") {
        let start = cursor + relative;
        output.push_str(&document[cursor..start]);
        let arguments_start = start + 4;
        let mut depth = 1_u32;
        let mut end = None;
        for (offset, character) in document[arguments_start..].char_indices() {
            match character {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        end = Some(arguments_start + offset);
                        break;
                    }
                }
                _ => {}
            }
        }
        let end = end.ok_or(GlyphPresentationError::UnsupportedSvg)?;
        let arguments = &document[arguments_start..end];
        let (name, fallback) = split_variable_arguments(arguments);
        let index = name
            .trim()
            .strip_prefix("--color")
            .and_then(|index| index.parse::<usize>().ok());
        let replacement = index
            .and_then(&mut color)
            .or_else(|| {
                fallback
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_owned)
            })
            .ok_or(GlyphPresentationError::UnsupportedSvg)?;
        output.push_str(&replacement);
        cursor = end + 1;
    }
    output.push_str(&document[cursor..]);
    Ok(output)
}

fn split_variable_arguments(arguments: &str) -> (&str, Option<&str>) {
    let mut depth = 0_u32;
    for (index, character) in arguments.char_indices() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => return (&arguments[..index], Some(&arguments[index + 1..])),
            _ => {}
        }
    }
    (arguments, None)
}

fn css_rgba([red, green, blue, alpha]: [u8; 4]) -> String {
    format!("rgba({red},{green},{blue},{})", f32::from(alpha) / 255.0)
}

fn css_rgba_f32([red, green, blue, alpha]: [f32; 4]) -> String {
    format!(
        "rgba({}%,{}%,{}%,{alpha})",
        red * 100.0,
        green * 100.0,
        blue * 100.0
    )
}

fn contains_ascii_case_insensitive(text: &str, needle: &str) -> bool {
    text.as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

fn has_glyph_child(group: &usvg::Group) -> bool {
    group.children().iter().any(|node| {
        is_glyph_identifier(node.id())
            || matches!(node, usvg::Node::Group(group) if has_glyph_child(group))
    })
}

fn is_glyph_identifier(identifier: &str) -> bool {
    identifier.strip_prefix("glyph").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn raster_dimension(
    design_dimension: f32,
    raster_scale: f32,
) -> Result<u32, GlyphPresentationError> {
    let dimension = (design_dimension * raster_scale).ceil();
    if !dimension.is_finite() || !(1.0..=MAX_RASTER_DIMENSION).contains(&dimension) {
        return Err(GlyphPresentationError::UnsupportedSvg);
    }
    Ok(dimension as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_font() -> ResolvedFont {
        crate::font::Font::from_bytes(font_test_data::COLRV0V1)
            .unwrap()
            .resolve()
    }

    #[test]
    fn selects_and_rasterizes_only_the_requested_glyph_node() {
        let document = br#"
            <svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
              <g id="glyph7"><rect x="100" y="-700" width="300" height="700" fill="red"/></g>
              <g id="glyph8"><circle cx="700" cy="-350" r="200" fill="blue"/></g>
            </svg>
        "#;

        let result = canvas(
            &test_font(),
            document,
            GlyphId::new(7),
            20.0,
            0,
            &crate::color::Color::rgb8(0, 0, 0).into(),
        )
        .unwrap();
        let crate::canvas::CanvasNode::Draw(draw) = result.node() else {
            panic!("SVG realization should be an image draw");
        };
        let crate::canvas::ColorField::Image(pattern) = &draw.source else {
            panic!("SVG realization should use an image color field");
        };

        assert_eq!((pattern.image().width(), pattern.image().height()), (6, 14));
        let crate::canvas::image::ImageKind::Srgba8(image) = pattern.image().kind() else {
            panic!("the SVG glyph source must be an sRGBA8 image");
        };
        assert!(image.pixels().chunks_exact(4).any(|pixel| pixel[3] != 0));
    }

    #[test]
    fn rejects_a_document_without_the_requested_glyph_node() {
        let document = br#"
            <svg xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
              <g id="glyph8"><rect width="100" height="100"/></g>
            </svg>
        "#;

        assert!(matches!(
            canvas(
                &test_font(),
                document,
                GlyphId::new(7),
                20.0,
                0,
                &crate::color::Color::rgb8(0, 0, 0).into(),
            ),
            Err(GlyphPresentationError::UnsupportedSvg)
        ));
    }

    #[test]
    fn supports_a_single_glyph_identified_by_the_root_svg_element() {
        let document = br#"
            <svg id="glyph7" xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
              <rect x="100" y="-700" width="300" height="700" fill="red"/>
            </svg>
        "#;

        assert!(
            canvas(
                &test_font(),
                document,
                GlyphId::new(7),
                20.0,
                0,
                &crate::color::Color::rgb8(0, 0, 0).into(),
            )
            .is_ok()
        );
    }

    #[test]
    fn applies_the_text_foreground_to_svg_current_color() {
        let document = br#"
            <svg id="glyph7" xmlns="http://www.w3.org/2000/svg" width="1000" height="1000">
              <rect x="0" y="0" width="1000" height="1000" fill="currentColor"/>
            </svg>
        "#;
        let canvas = canvas(
            &test_font(),
            document,
            GlyphId::new(7),
            20.0,
            0,
            &crate::color::Color::rgb8(210, 30, 90).into(),
        )
        .unwrap();
        let crate::canvas::CanvasNode::Draw(draw) = canvas.node() else {
            panic!("SVG realization should be an image draw");
        };
        let crate::canvas::ColorField::Image(pattern) = &draw.source else {
            panic!("SVG realization should use an image color field");
        };
        let crate::canvas::image::ImageKind::Srgba8(image) = pattern.image().kind() else {
            panic!("the SVG glyph source must be an sRGBA8 image");
        };
        let pixel = &image.pixels()[..4];
        assert_eq!(pixel, [210, 30, 90, 255]);
    }

    #[test]
    fn resolves_palette_variables_without_confusing_commas_in_fallback_colors() {
        let document = r#"<path fill="var(--color1, rgb(1, 2, 3))"/>"#;
        let resolved = replace_palette_variables(document, |index| {
            (index == 1).then(|| "rgba(10,20,30,0.5)".to_owned())
        })
        .unwrap();
        let fallback = replace_palette_variables(document, |_| None).unwrap();

        assert!(resolved.contains("rgba(10,20,30,0.5)"));
        assert!(fallback.contains("rgb(1, 2, 3)"));
    }

    #[test]
    fn current_color_detection_is_ascii_case_insensitive() {
        assert!(contains_ascii_case_insensitive(
            "fill: CURRENTcolor",
            "currentColor"
        ));
        assert!(!contains_ascii_case_insensitive(
            "fill: red",
            "currentColor"
        ));
    }
}
