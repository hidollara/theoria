use std::ops::Range;

use crate::math::Vector2;

use super::{InlineAlignment, TextLine};

/// One top-to-bottom column before it is placed in physical column order.
#[derive(Clone, Debug)]
pub(super) struct VerticalColumn {
    pub(super) line: TextLine,
    pub(super) baseline: Vector2,
    pub(super) source_range: Range<usize>,
    pub(super) justify_allowed: bool,
    pub(super) line_box_start: f32,
    pub(super) line_box_height: f32,
    pub(super) paragraph_first: bool,
    pub(super) paragraph_last: bool,
    pub(super) paragraph_spacing_before: f32,
    pub(super) paragraph_spacing_after: f32,
    pub(super) inline_insets: f32,
    pub(super) alignment: InlineAlignment,
}
