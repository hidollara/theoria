use std::ops::{Deref, DerefMut};

use crate::{math::Vector2, transform::Affine2};

use super::GlyphRun;

/// A size-independent shaping result interpreted at one em size for layout.
#[derive(Clone, Debug)]
pub(in crate::text) struct LayoutRun {
    shaped: GlyphRun,
    pub(in crate::text) font_size: f32,
}

impl LayoutRun {
    pub(super) const fn new(shaped: GlyphRun, font_size: f32) -> Self {
        Self { shaped, font_size }
    }

    pub(super) fn advance(&self) -> Vector2 {
        let design = self.shaped.advance();
        let scale = self.font_size / f32::from(self.shaped.font.metrics().units_per_em());
        Vector2::new(design.x * scale, -design.y * scale)
    }

    pub(super) const fn font_size(&self) -> f32 {
        self.font_size
    }

    pub(super) fn replace_shaped(&mut self, shaped: GlyphRun) {
        self.shaped = shaped;
    }
}

impl Deref for LayoutRun {
    type Target = GlyphRun;

    fn deref(&self) -> &Self::Target {
        &self.shaped
    }
}

impl DerefMut for LayoutRun {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.shaped
    }
}

#[derive(Clone, Debug)]
pub(in crate::text) struct PositionedRun {
    pub(in crate::text) run: LayoutRun,
    /// Maps the run's ordinary horizontal/upright Canvas realization into its
    /// final position within this line.
    pub(in crate::text) placement: Affine2,
    /// Start of this run along the line's physical inline axis.
    pub(in crate::text) inline_start: f32,
    /// Maps the run's native advance onto the line's inline axis.
    pub(in crate::text) inline_scale: f32,
    pub(in crate::text) orientation: RunOrientation,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::text) enum RunOrientation {
    #[default]
    Upright,
    Sideways,
    Combined,
}
