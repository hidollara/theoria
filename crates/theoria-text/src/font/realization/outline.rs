use skrifa::outline::OutlinePen;

use crate::{
    geometry::Point,
    path::{Path, Subpath},
};

#[derive(Default)]
pub(super) struct GlyphPathBuilder {
    path: Path,
    current: Option<Subpath>,
}

impl GlyphPathBuilder {
    pub(super) fn finish(mut self) -> Path {
        self.finish_current(false);
        self.path
    }

    fn finish_current(&mut self, closed: bool) {
        let Some(mut subpath) = self.current.take() else {
            return;
        };
        if closed {
            subpath = subpath.close();
        }
        self.path = std::mem::take(&mut self.path).add_subpath(subpath);
    }
}

impl OutlinePen for GlyphPathBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.finish_current(false);
        self.current = Some(Subpath::new(Point::new(x, y)));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if let Some(subpath) = self.current.take() {
            self.current = Some(subpath.line_to(Point::new(x, y)));
        }
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if let Some(subpath) = self.current.take() {
            self.current = Some(subpath.quadratic_to(Point::new(x1, y1), Point::new(x, y)));
        }
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if let Some(subpath) = self.current.take() {
            self.current =
                Some(subpath.cubic_to(Point::new(x1, y1), Point::new(x2, y2), Point::new(x, y)));
        }
    }

    fn close(&mut self) {
        self.finish_current(true);
    }
}

#[cfg(test)]
mod tests {
    use skrifa::outline::OutlinePen;

    use super::*;
    use crate::path::PathSegment;

    #[test]
    fn preserves_contours_and_bezier_segments() {
        let mut builder = GlyphPathBuilder::default();
        builder.move_to(1.0, 2.0);
        builder.line_to(3.0, 4.0);
        builder.quad_to(5.0, 6.0, 7.0, 8.0);
        builder.curve_to(9.0, 10.0, 11.0, 12.0, 13.0, 14.0);
        builder.close();
        builder.move_to(20.0, 21.0);
        builder.line_to(22.0, 23.0);

        let path = builder.finish();
        assert_eq!(path.subpaths().len(), 2);
        assert!(path.subpaths()[0].is_closed());
        assert!(!path.subpaths()[1].is_closed());
        assert_eq!(
            path.subpaths()[0].segments(),
            [
                PathSegment::LineTo(Point::new(3.0, 4.0)),
                PathSegment::QuadraticTo {
                    control: Point::new(5.0, 6.0),
                    end: Point::new(7.0, 8.0),
                },
                PathSegment::CubicTo {
                    control1: Point::new(9.0, 10.0),
                    control2: Point::new(11.0, 12.0),
                    end: Point::new(13.0, 14.0),
                },
            ]
        );
    }
}
