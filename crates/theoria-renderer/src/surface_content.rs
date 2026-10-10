use theoria_canvas::canvas::{Canvas, CanvasView, Color};

/// Backend-independent Canvas content to present on a drawable surface.
///
/// The renderer determines its physical GPU execution after a terminal Effect
/// snapshots it.
#[derive(Clone, Debug)]
pub struct SurfaceContent {
    canvas: Canvas,
    view: CanvasView,
    clear_color: Color,
}

impl SurfaceContent {
    pub fn new(canvas: Canvas, view: CanvasView, clear_color: impl Into<Color>) -> Self {
        Self {
            canvas,
            view,
            clear_color: clear_color.into(),
        }
    }

    pub const fn canvas(&self) -> &Canvas {
        &self.canvas
    }

    pub const fn view(&self) -> CanvasView {
        self.view
    }

    pub fn clear_color(&self) -> Color {
        self.clear_color.clone()
    }
}

#[cfg(test)]
mod tests {
    use theoria_canvas::{Coverage, Opacity, geometry::Point};
    use theoria_color::Srgb;

    use super::*;

    #[test]
    fn content_preserves_its_declarations_without_a_host_or_device() {
        let color = Color::new(Srgb, [12_u8, 34, 56], Opacity::<u8>::ONE);
        let canvas = Canvas::draw(color.clone(), Coverage::all());
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(1.0, 1.0)).unwrap();
        let content = SurfaceContent::new(canvas.clone(), view, color.clone());

        assert!(std::ptr::eq(content.canvas().node(), canvas.node()));
        assert_eq!(content.view(), view);
        assert_eq!(content.clear_color(), color);
    }
}
