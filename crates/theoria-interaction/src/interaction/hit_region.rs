use crate::{
    geometry::Point,
    path::{FillRule, Path},
};

use theoria_renderer::path::flatten_subpaths;

use super::geometry::point_segment_distance_squared;

#[derive(Clone, Debug, PartialEq)]
enum Shape {
    Fill { path: Path, rule: FillRule },
    RoundStroke { path: Path, radius: f32 },
}

/// A local-coordinate region that can participate in interaction hit testing.
///
/// A HitRegion has no component target, transform, or stacking order. A
/// gesture-specific binding supplies those delivery semantics when it places
/// this reusable geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct HitRegion {
    shape: Shape,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HitRegionError {
    NonFiniteWidth,
    NegativeWidth,
}

impl std::fmt::Display for HitRegionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteWidth => formatter.write_str("a hit stroke width must be finite"),
            Self::NegativeWidth => formatter.write_str("a hit stroke width must not be negative"),
        }
    }
}

impl std::error::Error for HitRegionError {}

impl HitRegion {
    /// Uses Path fill semantics, including the implicit closure of open Subpaths.
    pub fn fill(path: Path, rule: FillRule) -> Self {
        Self {
            shape: Shape::Fill { path, rule },
        }
    }

    /// Creates a round-capped, round-joined corridor centered on a Path.
    ///
    /// This is interaction geometry rather than a promise to mirror every
    /// Canvas StrokeStyle option. `width` is measured in the Path coordinate
    /// space and the maximum hit distance is half of it.
    pub fn round_stroke(path: Path, width: f32) -> Result<Self, HitRegionError> {
        if !width.is_finite() {
            return Err(HitRegionError::NonFiniteWidth);
        }
        if width < 0.0 {
            return Err(HitRegionError::NegativeWidth);
        }

        Ok(Self {
            shape: Shape::RoundStroke {
                path,
                radius: width * 0.5,
            },
        })
    }

    /// Evaluates a local point using the requested curve-flattening tolerance.
    ///
    /// Invalid points and nonpositive or nonfinite tolerances do not hit. A
    /// smaller tolerance follows curved Path segments more precisely.
    pub fn contains(&self, point: Point, tolerance: f32) -> bool {
        if !point.is_finite() || !tolerance.is_finite() || tolerance <= 0.0 {
            return false;
        }

        match &self.shape {
            Shape::Fill { path, rule } => fill_contains(path, *rule, point, tolerance),
            Shape::RoundStroke { path, radius } => {
                round_stroke_contains(path, *radius, point, tolerance)
            }
        }
    }
}

fn fill_contains(path: &Path, rule: FillRule, point: Point, tolerance: f32) -> bool {
    let mut winding = 0_i32;
    for points in flatten_subpaths(path, tolerance) {
        if points.len() < 3 {
            continue;
        }

        for (start, end) in closed_edges(&points) {
            if point_segment_distance_squared(point, start, end) <= tolerance * tolerance {
                return true;
            }

            if start.y <= point.y {
                if end.y > point.y && signed_side(start, end, point) > 0.0 {
                    winding += 1;
                }
            } else if end.y <= point.y && signed_side(start, end, point) < 0.0 {
                winding -= 1;
            }
        }
    }

    match rule {
        FillRule::NonZero => winding != 0,
        FillRule::EvenOdd => winding.rem_euclid(2) != 0,
    }
}

fn round_stroke_contains(path: &Path, radius: f32, point: Point, tolerance: f32) -> bool {
    let maximum_distance = radius + tolerance;
    let maximum_squared = maximum_distance * maximum_distance;
    flatten_subpaths(path, tolerance).iter().any(|points| {
        points
            .windows(2)
            .any(|edge| point_segment_distance_squared(point, edge[0], edge[1]) <= maximum_squared)
    })
}

fn closed_edges(points: &[Point]) -> impl Iterator<Item = (Point, Point)> + '_ {
    points
        .windows(2)
        .map(|edge| (edge[0], edge[1]))
        .chain(std::iter::once((*points.last().unwrap(), points[0])))
}

fn signed_side(start: Point, end: Point, point: Point) -> f64 {
    (f64::from(end.x) - f64::from(start.x)) * (f64::from(point.y) - f64::from(start.y))
        - (f64::from(end.y) - f64::from(start.y)) * (f64::from(point.x) - f64::from(start.x))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::path::Subpath;

    fn square(min: f32, max: f32) -> Subpath {
        Subpath::new(Point::new(min, min))
            .line_to(Point::new(max, min))
            .line_to(Point::new(max, max))
            .line_to(Point::new(min, max))
            .close()
    }

    #[test]
    fn fill_tests_open_subpaths_as_implicitly_closed() {
        let triangle = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .line_to(Point::new(10.0, 0.0))
                .line_to(Point::new(0.0, 10.0)),
        );
        let region = HitRegion::fill(triangle, FillRule::NonZero);

        assert!(region.contains(Point::new(2.0, 2.0), 0.01));
        assert!(!region.contains(Point::new(8.0, 8.0), 0.01));
    }

    #[test]
    fn fill_rules_distinguish_same_winding_nested_subpaths() {
        let path = Path::new()
            .add_subpath(square(0.0, 10.0))
            .add_subpath(square(2.0, 8.0));

        assert!(
            HitRegion::fill(path.clone(), FillRule::NonZero).contains(Point::new(5.0, 5.0), 0.01)
        );
        assert!(!HitRegion::fill(path, FillRule::EvenOdd).contains(Point::new(5.0, 5.0), 0.01));
    }

    #[test]
    fn round_stroke_uses_half_width_and_round_endpoints() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(10.0, 0.0)]);
        let region = HitRegion::round_stroke(path, 4.0).unwrap();

        assert!(region.contains(Point::new(5.0, 1.9), 0.01));
        assert!(!region.contains(Point::new(5.0, 2.1), 0.01));
        assert!(region.contains(Point::new(-1.9, 0.0), 0.01));
        assert!(!region.contains(Point::new(-2.1, 0.0), 0.01));
    }

    #[test]
    fn curved_strokes_are_evaluated_without_a_triangle_mesh() {
        let path = Path::new().add_subpath(
            Subpath::new(Point::new(0.0, 0.0))
                .quadratic_to(Point::new(5.0, 10.0), Point::new(10.0, 0.0)),
        );
        let region = HitRegion::round_stroke(path, 1.0).unwrap();

        assert!(region.contains(Point::new(5.0, 5.0), 0.01));
        assert!(!region.contains(Point::new(5.0, 7.0), 0.01));
    }

    #[test]
    fn invalid_numeric_inputs_do_not_enter_hit_evaluation() {
        let path = Path::polyline([Point::new(0.0, 0.0), Point::new(1.0, 0.0)]);
        assert_eq!(
            HitRegion::round_stroke(path.clone(), -1.0),
            Err(HitRegionError::NegativeWidth)
        );
        assert_eq!(
            HitRegion::round_stroke(path.clone(), f32::NAN),
            Err(HitRegionError::NonFiniteWidth)
        );

        let region = HitRegion::fill(path, FillRule::NonZero);
        assert!(!region.contains(Point::new(f32::NAN, 0.0), 0.01));
        assert!(!region.contains(Point::new(0.0, 0.0), 0.0));
    }
}
