use skrifa::{MetadataProvider, instance::Size, raw::TableProvider};

use super::{Font, ResolvedFont};

/// Metrics expressed in the font's design-unit coordinate system.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    units_per_em: u16,
    ascender: f32,
    descender: f32,
    line_gap: f32,
}

/// Vertical spacing information, whose meaning changed in `vhea` 1.1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum VerticalFontMetrics {
    /// Distances from the ideographic center baseline to the left and right
    /// edges, as defined by `vhea` 1.1.
    Typographic(FontMetrics),
    /// Baseline-to-baseline spacing derived from legacy `vhea` 1.0 fields.
    Legacy {
        units_per_em: u16,
        column_advance: f32,
    },
}

impl FontMetrics {
    pub const fn units_per_em(self) -> u16 {
        self.units_per_em
    }

    pub const fn ascender(self) -> f32 {
        self.ascender
    }

    pub const fn descender(self) -> f32 {
        self.descender
    }

    pub const fn line_gap(self) -> f32 {
        self.line_gap
    }
}

impl Font {
    pub fn metrics(&self) -> FontMetrics {
        self.resolve().metrics()
    }
}

impl ResolvedFont {
    pub(crate) fn metrics(&self) -> FontMetrics {
        let metrics = self.face().metrics(Size::unscaled(), self.location());
        FontMetrics {
            units_per_em: metrics.units_per_em,
            ascender: metrics.ascent,
            descender: metrics.descent,
            line_gap: metrics.leading,
        }
    }

    /// Returns the selected face's metrics for spacing vertical columns.
    ///
    /// Version 1.1 describes distances from the vertical center baseline to
    /// the ideographic left and right edges. Version 1.0 instead describes
    /// spacing relative to neighboring lines, so it is retained only as a
    /// baseline-to-baseline advance rather than reinterpreted as edges.
    pub(crate) fn vertical_metrics(&self) -> Option<VerticalFontMetrics> {
        use skrifa::raw::tables::mvar::tags::{VASC, VDSC, VLGP};

        let face = self.face();
        let vhea = face.vhea().ok()?;
        let mut ascender = f32::from(vhea.ascender().to_i16());
        let mut descender = f32::from(vhea.descender().to_i16());
        let mut line_gap = f32::from(vhea.line_gap().to_i16());
        let location = self.location();
        let coordinates = location.coords();

        if !coordinates.is_empty()
            && let Ok(mvar) = face.mvar()
        {
            let delta = |tag| {
                mvar.metric_delta(tag, coordinates)
                    .unwrap_or_default()
                    .to_f64() as f32
            };
            ascender += delta(VASC);
            descender += delta(VDSC);
            line_gap += delta(VLGP);
        }

        let metrics = FontMetrics {
            units_per_em: face.head().ok()?.units_per_em(),
            ascender,
            descender,
            line_gap,
        };
        if vhea.version().to_major_minor() >= (1, 1) {
            Some(VerticalFontMetrics::Typographic(metrics))
        } else {
            Some(VerticalFontMetrics::Legacy {
                units_per_em: metrics.units_per_em,
                column_advance: (metrics.ascender - metrics.descender + metrics.line_gap).max(0.0),
            })
        }
    }
}
