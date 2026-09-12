use crate::{geometry::Point, math::Vector2};

/// A permitted movement boundary around a pointer-down position.
///
/// Platforms may provide a default derived from their input metrics. Gesture
/// recognizers consume this common value without depending on its origin.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MovementThreshold {
    /// A circular boundary measured from the pointer-down position.
    Radial { radius: f32 },
    /// An axis-aligned rectangle whose values are nonnegative half-extents.
    AxisAligned { horizontal: f32, vertical: f32 },
}

impl MovementThreshold {
    pub(crate) fn is_valid(self) -> bool {
        match self {
            Self::Radial { radius } => radius.is_finite() && radius >= 0.0,
            Self::AxisAligned {
                horizontal,
                vertical,
            } => {
                horizontal.is_finite()
                    && horizontal >= 0.0
                    && vertical.is_finite()
                    && vertical >= 0.0
            }
        }
    }

    pub(crate) fn exceeded(self, origin: Point, position: Point) -> bool {
        let Vector2 { x, y } = position - origin;
        if !x.is_finite() || !y.is_finite() {
            return true;
        }
        match self {
            Self::Radial { radius } => x * x + y * y > radius * radius,
            Self::AxisAligned {
                horizontal,
                vertical,
            } => x.abs() > horizontal || y.abs() > vertical,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boundary_is_part_of_the_permitted_rectangle() {
        let threshold = MovementThreshold::AxisAligned {
            horizontal: 3.0,
            vertical: 4.0,
        };
        let origin = Point::new(0.0, 0.0);
        assert!(!threshold.exceeded(origin, Point::new(3.0, 4.0)));
        assert!(threshold.exceeded(origin, Point::new(3.1, 0.0)));
        assert!(threshold.exceeded(origin, Point::new(0.0, -4.1)));
    }

    #[test]
    fn radial_and_axis_aligned_values_validate_their_parameters() {
        assert!(MovementThreshold::Radial { radius: 1.0 }.is_valid());
        assert!(!MovementThreshold::Radial { radius: -1.0 }.is_valid());
        assert!(
            !MovementThreshold::AxisAligned {
                horizontal: 1.0,
                vertical: f32::NAN,
            }
            .is_valid()
        );
    }
}
