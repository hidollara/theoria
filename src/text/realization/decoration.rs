use crate::{geometry::Point, path::Path};

use super::super::{TextDecoration, TextDecorationKind, TextError, WritingMode};
use crate::canvas::Coverage;

pub(super) fn decoration_coverage(
    points: [Point; 4],
    writing_mode: WritingMode,
    decoration: &TextDecoration,
) -> Result<Coverage, TextError> {
    let left = points
        .iter()
        .map(|point| point.x)
        .fold(f32::INFINITY, f32::min);
    let right = points
        .iter()
        .map(|point| point.x)
        .fold(f32::NEG_INFINITY, f32::max);
    let top = points
        .iter()
        .map(|point| point.y)
        .fold(f32::INFINITY, f32::min);
    let bottom = points
        .iter()
        .map(|point| point.y)
        .fold(f32::NEG_INFINITY, f32::max);
    let cross_extent = match writing_mode {
        WritingMode::Horizontal => bottom - top,
        WritingMode::Vertical => right - left,
    };
    let thickness = decoration
        .thickness()
        .unwrap_or((cross_extent * 0.06).max(1.0));
    let offset = decoration.offset().unwrap_or(0.0);
    if !thickness.is_finite() || !offset.is_finite() {
        return Err(TextError::NonFiniteDecorationMetric);
    }
    if thickness < 0.0 {
        return Err(TextError::NegativeDecorationThickness);
    }
    let (x0, y0, x1, y1) = match (writing_mode, decoration.kind()) {
        (WritingMode::Horizontal, TextDecorationKind::Underline) => {
            (left, bottom - thickness + offset, right, bottom + offset)
        }
        (WritingMode::Horizontal, TextDecorationKind::Overline) => {
            (left, top + offset, right, top + thickness + offset)
        }
        (WritingMode::Horizontal, TextDecorationKind::LineThrough) => {
            let center = (top + bottom) * 0.5 + offset;
            (
                left,
                center - thickness * 0.5,
                right,
                center + thickness * 0.5,
            )
        }
        (WritingMode::Vertical, TextDecorationKind::Underline) => {
            (right - thickness + offset, top, right + offset, bottom)
        }
        (WritingMode::Vertical, TextDecorationKind::Overline) => {
            (left + offset, top, left + thickness + offset, bottom)
        }
        (WritingMode::Vertical, TextDecorationKind::LineThrough) => {
            let center = (left + right) * 0.5 + offset;
            (
                center - thickness * 0.5,
                top,
                center + thickness * 0.5,
                bottom,
            )
        }
    };
    Ok(Coverage::fill(Path::polygon([
        Point::new(x0, y0),
        Point::new(x1, y0),
        Point::new(x1, y1),
        Point::new(x0, y1),
    ])))
}
