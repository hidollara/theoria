//! Paths, points, and geometric elements.

use crate::implement;
use theoria_canvas::{geometry::Point, path::Path};

#[implement]
impl Path {
    type Value = Path;
}

#[implement]
impl PointValue {
    type Value = Point;
}

#[implement]
impl Rect {
    #[output(name = "path")]
    fn rect(input: Input<'_, Self>) -> Result<Path, &'static str> {
        let x = input.x().into_cloned();
        let y = input.y().into_cloned();
        let width = input.width().into_cloned();
        let height = input.height().into_cloned();

        let (right, bottom) = (x + width, y + height);
        if ![x, y, width, height, right, bottom]
            .iter()
            .all(|v| v.is_finite())
            || width < 0.0
            || height < 0.0
        {
            return Err("Rect requires finite coordinates and nonnegative dimensions");
        }
        Ok(Path::polygon(vec![
            Point::new(x, y),
            Point::new(right, y),
            Point::new(right, bottom),
            Point::new(x, bottom),
        ]))
    }
}

#[implement]
impl Point {
    #[output(name = "point", interface = "Position")]
    fn point(input: Input<'_, Self>) -> Point {
        let x = input.x().into_cloned();
        let y = input.y().into_cloned();

        Point::new(x, y)
    }
}

#[implement]
impl Polygon {
    #[output(name = "path")]
    fn polygon(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Path, &'static str> {
        let path = Path::polygon(input.children().iter().map(|p| *p.point().read(cx)));
        if !path.is_finite() {
            return Err("Path coordinates must be finite");
        }
        Ok(path)
    }
}

#[implement]
impl Shape {
    #[output(name = "path")]
    fn shape(input: Input<'_, Self>) -> Result<Path, &'static str> {
        let path = input.path().into_cloned();

        let path = path?;
        if path.is_finite() {
            Ok(path)
        } else {
            Err("Path coordinates must be finite")
        }
    }
}
