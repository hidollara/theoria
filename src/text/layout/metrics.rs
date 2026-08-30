use super::*;

#[derive(Clone, Copy, Debug, Default)]
pub(in crate::text) struct LineMetrics {
    pub(in crate::text) ascent: f32,
    pub(in crate::text) descent: f32,
    pub(in crate::text) leading: f32,
}

pub(super) fn vertical_alignment_offset(
    height: f32,
    content_height: f32,
    alignment: TextVerticalAlignment,
) -> f32 {
    let remaining = height - content_height;
    match alignment {
        TextVerticalAlignment::Top => 0.0,
        TextVerticalAlignment::Center => remaining * 0.5,
        TextVerticalAlignment::Bottom => remaining,
    }
}

pub(super) fn inline_alignment_offset(
    height: f32,
    content_height: f32,
    alignment: InlineAlignment,
) -> f32 {
    let remaining = height - content_height;
    match alignment {
        InlineAlignment::Start | InlineAlignment::Justify => 0.0,
        InlineAlignment::Center => remaining * 0.5,
        InlineAlignment::End => remaining,
    }
}

pub(super) fn font_line_metrics(font: &ResolvedFont, font_size: f32) -> LineMetrics {
    let metrics = font.metrics();
    let scale = font_size / f32::from(metrics.units_per_em());
    LineMetrics {
        ascent: (metrics.ascender() * scale).max(0.0),
        descent: (-metrics.descender() * scale).max(0.0),
        leading: (metrics.line_gap() * scale).max(0.0),
    }
}

pub(super) fn validate_letter_spacing(letter_spacing: f32) -> Result<(), TextError> {
    if letter_spacing.is_finite() {
        Ok(())
    } else {
        Err(TextError::NonFiniteLetterSpacing)
    }
}

pub(super) fn validate_word_spacing(word_spacing: f32) -> Result<(), TextError> {
    if word_spacing.is_finite() {
        Ok(())
    } else {
        Err(TextError::NonFiniteWordSpacing)
    }
}

pub(super) fn validate_baseline_shift(shift: f32) -> Result<(), TextError> {
    if shift.is_finite() {
        Ok(())
    } else {
        Err(TextError::NonFiniteBaselineShift)
    }
}

pub(super) fn validate_tab_size(tab_size: f32) -> Result<(), TextError> {
    if !tab_size.is_finite() {
        return Err(TextError::NonFiniteTabSize);
    }
    if tab_size < 0.0 {
        return Err(TextError::NegativeTabSize);
    }
    Ok(())
}

pub(super) fn validate_paragraph_style(style: ParagraphStyle) -> Result<(), TextError> {
    if [
        style.first_line_indent(),
        style.start_indent(),
        style.end_indent(),
        style.spacing_before(),
        style.spacing_after(),
    ]
    .into_iter()
    .all(f32::is_finite)
    {
        Ok(())
    } else {
        Err(TextError::NonFiniteParagraphMetric)
    }
}

pub(super) fn validate_justification_options(
    options: JustificationOptions,
) -> Result<(), TextError> {
    if let Some(value) = options.maximum_spacing_expansion() {
        if !value.is_finite() {
            return Err(TextError::NonFiniteJustificationExpansion);
        }
        if value < 0.0 {
            return Err(TextError::NegativeJustificationExpansion);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vertical_alignment_uses_signed_remaining_height_without_clipping() {
        assert_eq!(
            vertical_alignment_offset(100.0, 40.0, TextVerticalAlignment::Top),
            0.0
        );
        assert_eq!(
            vertical_alignment_offset(100.0, 40.0, TextVerticalAlignment::Center),
            30.0
        );
        assert_eq!(
            vertical_alignment_offset(100.0, 40.0, TextVerticalAlignment::Bottom),
            60.0
        );
        assert_eq!(
            vertical_alignment_offset(20.0, 40.0, TextVerticalAlignment::Bottom),
            -20.0
        );
    }
}
