#[cfg(test)]
use theoria_renderer::color_space::ColorRendererExt;

use skrifa::{
    GlyphId, MetadataProvider,
    color::{
        Brush, ColorGlyphFormat, ColorPainter, ColorPalette, ColorStop, PaintCachedColorGlyph,
        PaintError, Transform,
    },
    raw::{
        tables::colr::{CompositeMode, Extend},
        types::BoundingBox,
    },
};

use crate::{
    canvas::Color,
    canvas::{
        BlendMode, Canvas, ColorField, Composite, Composition, ConicGradient, Coverage,
        GradientExtend, GradientStop, LinearGradient, Opacity, RadialGradient,
    },
    color::{LinearSrgb, Srgb},
    geometry::Point,
    path::Path,
    transform::Affine2,
};

use super::{GlyphPresentationError, RealizedGlyph, ResolvedFont};

pub(super) fn canvas(
    font: &ResolvedFont,
    glyph_id: GlyphId,
    palette_index: u16,
    foreground: &ColorField,
) -> Result<Option<RealizedGlyph>, GlyphPresentationError> {
    let face = font.face();
    let glyphs = face.color_glyphs();
    let Some(glyph) = glyphs
        .get_with_format(glyph_id, ColorGlyphFormat::ColrV1)
        .or_else(|| glyphs.get_with_format(glyph_id, ColorGlyphFormat::ColrV0))
    else {
        return Ok(None);
    };
    let palettes = face.color_palettes();
    let palette = palettes.get(palette_index).or_else(|| palettes.get(0));
    let mut painter = CanvasColorPainter::new(font, foreground.clone(), palette);
    let first = glyph
        .paint(font.location(), &mut painter)
        .map_err(|_| GlyphPresentationError::InvalidColorGlyph)
        .and_then(|_| painter.finish());
    match first {
        Ok(canvas) => Ok(Some(realized(canvas))),
        Err(_) if matches!(glyph.format(), ColorGlyphFormat::ColrV1) => {
            let Some(v0) = glyphs.get_with_format(glyph_id, ColorGlyphFormat::ColrV0) else {
                return Err(GlyphPresentationError::InvalidColorGlyph);
            };
            let palette = palettes.get(palette_index).or_else(|| palettes.get(0));
            let mut painter = CanvasColorPainter::new(font, foreground.clone(), palette);
            v0.paint(font.location(), &mut painter)
                .map_err(|_| GlyphPresentationError::InvalidColorGlyph)?;
            painter.finish().map(|canvas| Some(realized(canvas)))
        }
        Err(_) => Err(GlyphPresentationError::InvalidColorGlyph),
    }
}

struct Layer {
    backdrop: Option<Canvas>,
    blend_mode: BlendMode,
    composite: Composite,
}

fn realized(canvas: Option<Canvas>) -> RealizedGlyph {
    canvas.map_or(RealizedGlyph::Empty, RealizedGlyph::Intrinsic)
}

// A missing operand still matters for COLR composition modes other than SourceOver.
fn transparent_operand() -> Canvas {
    Canvas::draw(
        Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::from(0)),
        Coverage::all(),
    )
}

struct CanvasColorPainter<'font, 'palette> {
    font: &'font ResolvedFont,
    foreground: ColorField,
    palette: Option<ColorPalette<'palette>>,
    canvas: Option<Canvas>,
    transforms: Vec<Affine2>,
    clips: Vec<Coverage>,
    layers: Vec<Layer>,
    error: Option<GlyphPresentationError>,
}

impl<'font, 'palette> CanvasColorPainter<'font, 'palette> {
    fn new(
        font: &'font ResolvedFont,
        foreground: ColorField,
        palette: Option<ColorPalette<'palette>>,
    ) -> Self {
        Self {
            font,
            foreground,
            palette,
            canvas: None,
            transforms: vec![Affine2::IDENTITY],
            clips: Vec::new(),
            layers: Vec::new(),
            error: None,
        }
    }

    fn finish(self) -> Result<Option<Canvas>, GlyphPresentationError> {
        self.error.map_or(Ok(self.canvas), Err)
    }

    fn transform(&self) -> Affine2 {
        *self.transforms.last().expect("transform stack has a root")
    }

    fn fail(&mut self, error: GlyphPresentationError) {
        self.error.get_or_insert(error);
    }

    fn glyph_coverage(&self, glyph_id: GlyphId) -> Result<Coverage, GlyphPresentationError> {
        let path = self.font.glyph_ref(glyph_id).outline_path()?;
        Ok(transform_coverage(Coverage::fill(path), self.transform()))
    }

    fn current_clip(&self) -> Coverage {
        self.clips.last().cloned().unwrap_or_else(Coverage::all)
    }

    fn push_clip_coverage(&mut self, coverage: Coverage) {
        let combined = self
            .clips
            .last()
            .cloned()
            .map_or(coverage.clone(), |current| intersect(current, coverage));
        self.clips.push(combined);
    }

    fn brush_field(&self, brush: Brush<'_>) -> Result<ColorField, GlyphPresentationError> {
        match brush {
            Brush::Solid {
                palette_index,
                alpha,
            } => self.solid_field(palette_index, alpha),
            Brush::LinearGradient {
                p0,
                p1,
                color_stops,
                extend,
            } => {
                let extend = gradient_extend(extend)?;
                let stops = self.gradient_stops(color_stops)?;
                if stops.len() == 1 {
                    return Ok(stops[0].color().into());
                }
                LinearGradient::new(
                    Point::new(p0.x, p0.y),
                    Point::new(p1.x, p1.y),
                    stops,
                    crate::color::LinearSrgb,
                )
                .map(|gradient| gradient.with_extend(extend))
                .map(Into::into)
                .map_err(|_| GlyphPresentationError::InvalidColorGlyph)
            }
            Brush::RadialGradient {
                c0,
                r0,
                c1,
                r1,
                color_stops,
                extend,
            } => {
                let extend = gradient_extend(extend)?;
                let stops = self.gradient_stops(color_stops)?;
                if stops.len() == 1 {
                    return Ok(stops[0].color().into());
                }
                RadialGradient::between_circles(
                    Point::new(c0.x, c0.y),
                    r0,
                    Point::new(c1.x, c1.y),
                    r1,
                    stops,
                    crate::color::LinearSrgb,
                )
                .map(|gradient| gradient.with_extend(extend))
                .map(Into::into)
                .map_err(|_| GlyphPresentationError::InvalidColorGlyph)
            }
            Brush::SweepGradient {
                c0,
                start_angle,
                end_angle,
                color_stops,
                extend,
            } => {
                let extend = gradient_extend(extend)?;
                let stops = self.gradient_stops(color_stops)?;
                if stops.len() == 1 {
                    return Ok(stops[0].color().into());
                }
                ConicGradient::sweeping(
                    Point::new(c0.x, c0.y),
                    -start_angle.to_radians(),
                    -end_angle.to_radians(),
                    stops,
                    crate::color::LinearSrgb,
                )
                .map(|gradient| gradient.with_extend(extend))
                .map(Into::into)
                .map_err(|_| GlyphPresentationError::InvalidColorGlyph)
            }
        }
    }

    fn solid_field(
        &self,
        palette_index: u16,
        alpha: f32,
    ) -> Result<ColorField, GlyphPresentationError> {
        if !alpha.is_finite() {
            return Err(GlyphPresentationError::InvalidColorGlyph);
        }
        if palette_index == 0xFFFF {
            if alpha >= 1.0 {
                return Ok(self.foreground.clone());
            }
            let opacity = Color::new(
                Srgb,
                [1.0, 1.0, 1.0],
                Opacity::<f32>::new(alpha.clamp(0.0, 1.0))
                    .map_err(|_| GlyphPresentationError::InvalidColorGlyph)?,
            );
            return Ok(Canvas::draw(self.foreground.clone(), Coverage::from_alpha(opacity)).into());
        }
        let color = self
            .palette
            .as_ref()
            .and_then(|palette| palette.colors().get(usize::from(palette_index)))
            .ok_or(GlyphPresentationError::InvalidColorGlyph)?;
        Ok(Color::new(
            Srgb,
            [color.red(), color.green(), color.blue()],
            Opacity::<u8>::from(((f32::from(color.alpha()) * alpha.clamp(0.0, 1.0)).round()) as u8),
        )
        .into())
    }

    fn gradient_stops(
        &self,
        stops: &[ColorStop],
    ) -> Result<Vec<GradientStop>, GlyphPresentationError> {
        stops
            .iter()
            .map(|stop| {
                if !stop.alpha.is_finite() {
                    return Err(GlyphPresentationError::InvalidColorGlyph);
                }
                let color = if stop.palette_index == 0xFFFF {
                    let ColorField::Solid(color) = &self.foreground else {
                        return Err(GlyphPresentationError::UnsupportedColorPaint);
                    };
                    color.with_alpha_factor(stop.alpha)
                } else {
                    let ColorField::Solid(color) =
                        self.solid_field(stop.palette_index, stop.alpha)?
                    else {
                        unreachable!("CPAL entries always resolve to solid colors")
                    };
                    color
                };
                GradientStop::new(stop.offset, color)
                    .map_err(|_| GlyphPresentationError::InvalidColorGlyph)
            })
            .collect()
    }

    fn append(&mut self, source: Canvas) {
        self.canvas = Some(match self.canvas.take() {
            Some(backdrop) => {
                backdrop.composite(source, Composition::new(Composite::SourceOver, LinearSrgb))
            }
            None => source,
        });
    }
}

impl ColorPainter for CanvasColorPainter<'_, '_> {
    fn push_transform(&mut self, transform: Transform) {
        match affine(transform).and_then(|local| local.then(self.transform())) {
            Some(transform) => self.transforms.push(transform),
            None => {
                self.fail(GlyphPresentationError::InvalidColorGlyph);
                self.transforms.push(Affine2::IDENTITY);
            }
        }
    }

    fn pop_transform(&mut self) {
        if self.transforms.len() > 1 {
            self.transforms.pop();
        } else {
            self.fail(GlyphPresentationError::InvalidColorGlyph);
        }
    }

    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        match self.glyph_coverage(glyph_id) {
            Ok(coverage) => self.push_clip_coverage(coverage),
            Err(error) => self.fail(error),
        }
    }

    fn push_clip_box(&mut self, clip_box: BoundingBox<f32>) {
        let path = Path::polygon([
            Point::new(clip_box.x_min, clip_box.y_min),
            Point::new(clip_box.x_max, clip_box.y_min),
            Point::new(clip_box.x_max, clip_box.y_max),
            Point::new(clip_box.x_min, clip_box.y_max),
        ]);
        self.push_clip_coverage(transform_coverage(Coverage::fill(path), self.transform()));
    }

    fn pop_clip(&mut self) {
        if self.clips.pop().is_none() {
            self.fail(GlyphPresentationError::InvalidColorGlyph);
        }
    }

    fn fill(&mut self, brush: Brush<'_>) {
        match self.brush_field(brush) {
            Ok(field) => {
                let transformed =
                    Canvas::draw(field, Coverage::all()).transformed(self.transform());
                self.append(Canvas::draw(transformed, self.current_clip()));
            }
            Err(error) => self.fail(error),
        }
    }

    fn paint_cached_color_glyph(
        &mut self,
        _glyph: GlyphId,
    ) -> Result<PaintCachedColorGlyph, PaintError> {
        Ok(PaintCachedColorGlyph::Unimplemented)
    }

    fn push_layer(&mut self, mode: CompositeMode) {
        match composition(mode) {
            Ok((blend_mode, composite)) => {
                self.layers.push(Layer {
                    backdrop: self.canvas.take(),
                    blend_mode,
                    composite,
                });
            }
            Err(error) => self.fail(error),
        }
    }

    fn pop_layer_with_mode(&mut self, _mode: CompositeMode) {
        let Some(layer) = self.layers.pop() else {
            self.fail(GlyphPresentationError::InvalidColorGlyph);
            return;
        };
        let source = self.canvas.take();
        self.canvas = match (layer.backdrop, source) {
            (None, None) => None,
            (backdrop, source) => Some(
                backdrop.unwrap_or_else(transparent_operand).composite(
                    source.unwrap_or_else(transparent_operand),
                    Composition::new(layer.composite, LinearSrgb)
                        .with_blend(layer.blend_mode, LinearSrgb),
                ),
            ),
        };
    }
}

fn affine(transform: Transform) -> Option<Affine2> {
    Affine2::from_rows([
        [transform.xx, transform.xy, transform.dx],
        [transform.yx, transform.yy, transform.dy],
    ])
}

fn intersect(left: Coverage, right: Coverage) -> Coverage {
    let left = Canvas::draw(
        Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
        left,
    );
    Coverage::from_alpha(Canvas::draw(left, right))
}

fn transform_coverage(coverage: Coverage, transform: Affine2) -> Coverage {
    Coverage::from_alpha(
        Canvas::draw(
            Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ONE),
            coverage,
        )
        .transformed(transform),
    )
}

fn gradient_extend(extend: Extend) -> Result<GradientExtend, GlyphPresentationError> {
    match extend {
        Extend::Pad => Ok(GradientExtend::Pad),
        Extend::Repeat => Ok(GradientExtend::Repeat),
        Extend::Reflect => Ok(GradientExtend::Reflect),
        Extend::Unknown => Err(GlyphPresentationError::UnsupportedColorPaint),
    }
}

fn composition(mode: CompositeMode) -> Result<(BlendMode, Composite), GlyphPresentationError> {
    let normal = BlendMode::Normal;
    let source_over = Composite::SourceOver;
    Ok(match mode {
        CompositeMode::Clear => (normal, Composite::Clear),
        CompositeMode::Src => (normal, Composite::Copy),
        CompositeMode::Dest => (normal, Composite::Destination),
        CompositeMode::SrcOver => (normal, source_over),
        CompositeMode::DestOver => (normal, Composite::DestinationOver),
        CompositeMode::SrcIn => (normal, Composite::SourceIn),
        CompositeMode::DestIn => (normal, Composite::DestinationIn),
        CompositeMode::SrcOut => (normal, Composite::SourceOut),
        CompositeMode::DestOut => (normal, Composite::DestinationOut),
        CompositeMode::SrcAtop => (normal, Composite::SourceAtop),
        CompositeMode::DestAtop => (normal, Composite::DestinationAtop),
        CompositeMode::Xor => (normal, Composite::Xor),
        CompositeMode::Plus => (normal, Composite::Plus),
        CompositeMode::Screen => (BlendMode::Screen, source_over),
        CompositeMode::Overlay => (BlendMode::Overlay, source_over),
        CompositeMode::Darken => (BlendMode::Darken, source_over),
        CompositeMode::Lighten => (BlendMode::Lighten, source_over),
        CompositeMode::ColorDodge => (BlendMode::ColorDodge, source_over),
        CompositeMode::ColorBurn => (BlendMode::ColorBurn, source_over),
        CompositeMode::HardLight => (BlendMode::HardLight, source_over),
        CompositeMode::SoftLight => (BlendMode::SoftLight, source_over),
        CompositeMode::Difference => (BlendMode::Difference, source_over),
        CompositeMode::Exclusion => (BlendMode::Exclusion, source_over),
        CompositeMode::Multiply => (BlendMode::Multiply, source_over),
        CompositeMode::HslHue => (BlendMode::Hue, source_over),
        CompositeMode::HslSaturation => (BlendMode::Saturation, source_over),
        CompositeMode::HslColor => (BlendMode::Color, source_over),
        CompositeMode::HslLuminosity => (BlendMode::Luminosity, source_over),
        // OpenType requires unknown COLRv1 composite values to use Clear.
        CompositeMode::Unknown => (normal, Composite::Clear),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first_color_glyph(font: &ResolvedFont, format: ColorGlyphFormat) -> GlyphId {
        let glyphs = font.face().color_glyphs();
        (0..4096)
            .map(GlyphId::new)
            .find(|glyph| glyphs.get_with_format(*glyph, format).is_some())
            .expect("test font contains the requested color glyph format")
    }

    #[test]
    fn unknown_colr_composite_mode_falls_back_to_clear() {
        assert_eq!(
            composition(CompositeMode::Unknown),
            Ok((BlendMode::Normal, Composite::Clear))
        );
    }

    #[test]
    fn gradient_current_color_multiplies_its_stop_alpha() {
        let font = crate::font::Font::from_bytes(font_test_data::COLRV0V1)
            .unwrap()
            .resolve();
        let foreground = Color::new(Srgb, [100_u8, 120, 140], Opacity::<u8>::from(128));
        let painter = CanvasColorPainter::new(&font, foreground.into(), None);
        let stops = painter
            .gradient_stops(&[
                ColorStop {
                    offset: 0.0,
                    palette_index: 0xFFFF,
                    alpha: 0.5,
                },
                ColorStop {
                    offset: 1.0,
                    palette_index: 0xFFFF,
                    alpha: 1.0,
                },
            ])
            .unwrap();

        let first = stops[0].color().straight_rgba_in(crate::color::Srgb);
        let last = stops[1].color().straight_rgba_in(crate::color::Srgb);
        assert!((first[3] - 64.0 / 255.0).abs() < 0.000_001);
        assert!((last[3] - 128.0 / 255.0).abs() < 0.000_001);
        for component in 0..3 {
            assert_eq!(first[component], last[component]);
        }
    }

    #[test]
    fn colr_v0_layers_lower_to_canvas_composition() {
        let font = crate::font::Font::from_bytes(font_test_data::COLRV0V1)
            .unwrap()
            .resolve();
        let face = font.face();
        let glyphs = face.color_glyphs();
        let glyph_id = (0..4096)
            .map(GlyphId::new)
            .find(|glyph| {
                glyphs
                    .get_with_format(*glyph, ColorGlyphFormat::ColrV0)
                    .is_some()
                    && glyphs
                        .get_with_format(*glyph, ColorGlyphFormat::ColrV1)
                        .is_none()
            })
            .or_else(|| {
                (0..4096).map(GlyphId::new).find(|glyph| {
                    glyphs
                        .get_with_format(*glyph, ColorGlyphFormat::ColrV0)
                        .is_some()
                })
            })
            .expect("test font contains a COLRv0 glyph");

        let result = canvas(
            &font,
            glyph_id,
            0,
            &Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE).into(),
        );
        assert!(matches!(result, Ok(Some(_))));
    }

    #[test]
    fn colr_v1_paint_graph_lowers_to_canvas_semantics() {
        let font = crate::font::Font::from_bytes(font_test_data::COLRV0V1)
            .unwrap()
            .resolve();
        let glyph = first_color_glyph(&font, ColorGlyphFormat::ColrV1);
        let result = canvas(
            &font,
            glyph,
            0,
            &Color::new(Srgb, [20_u8, 40, 60], Opacity::<u8>::ONE).into(),
        );

        assert!(matches!(result, Ok(Some(_))), "COLRv1 result: {result:?}");
    }

    #[test]
    fn colr_v1_without_a_clip_list_still_lowers_to_canvas_semantics() {
        let font = crate::font::Font::from_bytes(font_test_data::COLRV1_NO_CLIPLIST)
            .unwrap()
            .resolve();
        let glyph = first_color_glyph(&font, ColorGlyphFormat::ColrV1);

        assert!(matches!(
            canvas(
                &font,
                glyph,
                0,
                &Color::new(Srgb, [20_u8, 40, 60], Opacity::<u8>::ONE).into()
            ),
            Ok(Some(_))
        ));
    }

    #[test]
    fn variable_colr_location_changes_the_lowered_canvas_semantics() {
        let default = crate::font::Font::from_bytes(font_test_data::COLRV0V1_VARIABLE).unwrap();
        let resolved_default = default.resolve();
        let foreground = Color::new(Srgb, [20_u8, 40, 60], Opacity::<u8>::ONE).into();
        let changed = default.variation_axes().into_iter().any(|axis| {
            let varied = default
                .clone()
                .with_variation(axis.tag(), axis.maximum())
                .unwrap();

            (0..4096).map(GlyphId::new).any(|glyph| {
                if resolved_default
                    .face()
                    .color_glyphs()
                    .get_with_format(glyph, ColorGlyphFormat::ColrV1)
                    .is_none()
                {
                    return false;
                }

                let Ok(Some(at_default)) = canvas(&resolved_default, glyph, 0, &foreground) else {
                    return false;
                };
                let Ok(Some(at_maximum)) = canvas(&varied.resolve(), glyph, 0, &foreground) else {
                    return false;
                };
                format!("{at_default:#?}") != format!("{at_maximum:#?}")
            })
        });

        assert!(
            changed,
            "a variable COLRv1 axis must affect the lowered Canvas semantics"
        );
    }
}
