use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct VerticalColumnMetrics {
    pub(super) left: f32,
    pub(super) right: f32,
}

impl VerticalColumnMetrics {
    const fn width(self) -> f32 {
        self.left + self.right
    }
}

pub(super) fn vertical_layout_width(
    columns: &[VerticalColumnMetrics],
    fixed_column_width: Option<f32>,
) -> f32 {
    let Some(first) = columns.first() else {
        return 0.0;
    };
    let Some(last) = columns.last() else {
        return 0.0;
    };
    match fixed_column_width {
        Some(column_width) => {
            first.right + column_width * columns.len().saturating_sub(1) as f32 + last.left
        }
        None => columns
            .iter()
            .copied()
            .map(VerticalColumnMetrics::width)
            .sum(),
    }
}

pub(super) fn vertical_column_metrics(
    line: &TextLine,
    default_style: &TextStyle,
) -> VerticalColumnMetrics {
    line.runs
        .iter()
        .map(|positioned| {
            let metrics = font_line_metrics(positioned.run.font(), positioned.run.font_size());
            let half_leading = metrics.leading * 0.5;
            match positioned.orientation {
                RunOrientation::Upright => {
                    upright_column_metrics(positioned.run.font(), positioned.run.font_size())
                }
                RunOrientation::Sideways => VerticalColumnMetrics {
                    left: (metrics.descent + half_leading - positioned.placement.translation().x)
                        .max(0.0),
                    right: (metrics.ascent + half_leading + positioned.placement.translation().x)
                        .max(0.0),
                },
                RunOrientation::Combined => {
                    let half_em = positioned.run.font_size() * 0.5;
                    VerticalColumnMetrics {
                        left: half_em,
                        right: half_em,
                    }
                }
            }
        })
        .reduce(|left, right| VerticalColumnMetrics {
            left: left.left.max(right.left),
            right: left.right.max(right.right),
        })
        .unwrap_or_else(|| {
            let Some(font) = default_style.fonts().first() else {
                return VerticalColumnMetrics::default();
            };
            upright_column_metrics(&font.resolve(), default_style.font_size())
        })
}

fn upright_column_metrics(font: &ResolvedFont, font_size: f32) -> VerticalColumnMetrics {
    let Some(metrics) = font.vertical_metrics() else {
        // HarfRust's fallback for a face without vertical metrics advances one
        // em and centers the glyph's horizontal advance on the vertical axis.
        // Mirroring that convention keeps layout and shaping in agreement.
        let half_em = font_size * 0.5;
        return VerticalColumnMetrics {
            left: half_em,
            right: half_em,
        };
    };
    match metrics {
        crate::font::VerticalFontMetrics::Typographic(metrics) => {
            let scale = font_size / f32::from(metrics.units_per_em());
            let half_gap = (metrics.line_gap() * scale).max(0.0) * 0.5;
            VerticalColumnMetrics {
                left: (-metrics.descender() * scale).max(0.0) + half_gap,
                right: (metrics.ascender() * scale).max(0.0) + half_gap,
            }
        }
        crate::font::VerticalFontMetrics::Legacy {
            units_per_em,
            column_advance,
        } => {
            let half_advance = column_advance * font_size / f32::from(units_per_em) * 0.5;
            VerticalColumnMetrics {
                left: half_advance,
                right: half_advance,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::test_support::windows_vertical_fonts;

    #[cfg(target_os = "windows")]
    #[test]
    fn vertical_column_metrics_follow_declared_metrics_or_the_em_fallback() {
        let Some(fonts) = windows_vertical_fonts() else {
            return;
        };
        let font = fonts[0].clone();
        let upright = GlyphRun::shape_with(
            font.clone(),
            "本",
            &ShapingOptions::default().exact_direction(ShapingDirection::TopToBottom),
        )
        .unwrap();
        let sideways = GlyphRun::shape_with(font, "A", &ShapingOptions::default()).unwrap();
        let style = TextStyle::new(&fonts, 32.0);

        let upright_metrics = vertical_column_metrics(
            &TextLine::from_vertical_items(
                [VerticalItem::Oriented(
                    InlineRun::unspaced(upright, 32.0),
                    RunOrientation::Upright,
                )],
                "本",
            ),
            &style,
        );
        let sideways_metrics = vertical_column_metrics(
            &TextLine::from_vertical_items(
                [VerticalItem::Oriented(
                    InlineRun::unspaced(sideways, 32.0),
                    RunOrientation::Sideways,
                )],
                "A",
            ),
            &style,
        );

        match fonts[0].resolve().vertical_metrics() {
            Some(crate::font::VerticalFontMetrics::Typographic(metrics)) => {
                let scale = 32.0 / f32::from(metrics.units_per_em());
                let half_gap = (metrics.line_gap() * scale).max(0.0) * 0.5;
                assert_eq!(
                    upright_metrics,
                    VerticalColumnMetrics {
                        left: (-metrics.descender() * scale).max(0.0) + half_gap,
                        right: (metrics.ascender() * scale).max(0.0) + half_gap,
                    }
                );
            }
            Some(crate::font::VerticalFontMetrics::Legacy {
                units_per_em,
                column_advance,
            }) => {
                let half_advance = column_advance * 32.0 / f32::from(units_per_em) * 0.5;
                assert_eq!(upright_metrics.left, half_advance);
                assert_eq!(upright_metrics.right, half_advance);
            }
            None => assert_eq!(
                upright_metrics,
                VerticalColumnMetrics {
                    left: 16.0,
                    right: 16.0,
                }
            ),
        }
        assert!(sideways_metrics.left > 0.0);
        assert!(sideways_metrics.right > 0.0);
    }

    #[test]
    fn differently_sized_vertical_columns_touch_their_typographic_outer_edges() {
        let columns = [
            VerticalColumnMetrics {
                left: 8.0,
                right: 12.0,
            },
            VerticalColumnMetrics {
                left: 20.0,
                right: 10.0,
            },
        ];

        assert_eq!(vertical_layout_width(&columns, None), 50.0);
    }
}
