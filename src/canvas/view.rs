use crate::{
    geometry::Point,
    math::{Matrix2, Vector2},
    transform::{Affine2, Projective2},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasView {
    /// Maps Canvas coordinates into the normalized, top-left-origin unit
    /// square whose bottom-right corner is `(1, 1)`.
    pub canvas_to_view: Projective2,
}

impl CanvasView {
    pub const fn new(canvas_to_view: Projective2) -> Self {
        Self { canvas_to_view }
    }

    /// Defines an affine mapping from Canvas points to the normalized View.
    ///
    /// `p00`, `p10`, and `p01` are the Canvas-space images of normalized View
    /// coordinates `(0,0)`, `(1,0)`, and `(0,1)`, respectively.
    pub fn affine(p00: Point, p10: Point, p01: Point) -> Option<Self> {
        if !p00.is_finite() || !p10.is_finite() || !p01.is_finite() {
            return None;
        }

        let x_axis = p10 - p00;
        let y_axis = p01 - p00;

        let view_to_canvas = Affine2::new(
            Matrix2::from_columns(x_axis, y_axis),
            Vector2::new(p00.x, p00.y),
        );
        let canvas_to_view = view_to_canvas.try_inverse()?;
        Some(Self::new(Projective2::from(canvas_to_view)))
    }

    /// Maps two axis-aligned Canvas corners to normalized View coordinates
    /// `(0,0)` and `(1,1)`. Their order determines both View-axis directions.
    pub fn axis_aligned(p00: Point, p11: Point) -> Option<Self> {
        let diagonal = p11 - p00;
        Self::affine(
            p00,
            p00 + Vector2::new(diagonal.x, 0.0),
            p00 + Vector2::new(0.0, diagonal.y),
        )
    }

    /// Maps the four Canvas points corresponding to View coordinates `(0,0)`,
    /// `(1,0)`, `(1,1)`, `(0,1)` onto the normalized View square.
    pub fn projective(p00: Point, p10: Point, p11: Point, p01: Point) -> Option<Self> {
        let view_to_canvas = Projective2::from_unit_square_to_quad([p00, p10, p11, p01])?;
        Some(Self::new(view_to_canvas.try_inverse()?))
    }
}
