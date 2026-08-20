use crate::{
    geometry::Point,
    math::{Matrix2, Matrix3, Vector2, Vector3},
};

/// A coordinate-system-neutral 2D affine transformation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    linear: Matrix2,
    translation: Vector2,
}

impl Affine2 {
    pub const IDENTITY: Self = Self::new(Matrix2::IDENTITY, Vector2::ZERO);

    pub const fn new(linear: Matrix2, translation: Vector2) -> Self {
        Self {
            linear,
            translation,
        }
    }

    pub const fn from_matrix(linear: Matrix2) -> Self {
        Self::new(linear, Vector2::ZERO)
    }

    pub const fn from_translation(translation: Vector2) -> Self {
        Self::new(Matrix2::IDENTITY, translation)
    }

    pub const fn from_scale(scale: Vector2) -> Self {
        Self::from_matrix(Matrix2::from_scale(scale))
    }

    pub const fn linear(self) -> Matrix2 {
        self.linear
    }

    pub const fn translation(self) -> Vector2 {
        self.translation
    }

    pub fn transform_point(self, point: Point) -> Point {
        let transformed = self.linear.transform_vector(Vector2::new(point.x, point.y));
        Point::new(
            transformed.x + self.translation.x,
            transformed.y + self.translation.y,
        )
    }

    pub const fn transform_vector(self, vector: Vector2) -> Vector2 {
        self.linear.transform_vector(vector)
    }

    /// Applies `self` first and `next` second.
    pub fn then(self, next: Self) -> Self {
        Self::new(
            self.linear.then(next.linear),
            next.linear.transform_vector(self.translation) + next.translation,
        )
    }

    pub fn try_inverse(self) -> Option<Self> {
        let linear = self.linear.try_inverse()?;
        Some(Self::new(
            linear,
            linear.transform_vector(-self.translation),
        ))
    }
}

/// A coordinate-system-neutral projective transformation of a 2D plane.
///
/// The wrapped Matrix3 acts on homogeneous `(x, y, 1)` vectors. Matrices that
/// differ only by a non-zero scalar represent the same projective mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective2 {
    matrix: Matrix3,
}

impl Projective2 {
    pub const IDENTITY: Self = Self::new(Matrix3::IDENTITY);

    pub const fn new(matrix: Matrix3) -> Self {
        Self { matrix }
    }

    pub const fn from_rows(rows: [[f32; 3]; 3]) -> Self {
        Self::new(Matrix3::from_rows(rows))
    }

    pub const fn matrix(self) -> Matrix3 {
        self.matrix
    }

    pub const fn rows(self) -> [[f32; 3]; 3] {
        self.matrix.rows()
    }

    /// Builds a projective mapping from the unit square corners `(0,0)`,
    /// `(1,0)`, `(1,1)`, `(0,1)` to the corresponding points in `quad`.
    pub fn from_unit_square_to_quad(quad: [Point; 4]) -> Option<Self> {
        if !quad.iter().all(|point| point.is_finite()) {
            return None;
        }

        let [p00, p10, p11, p01] = quad;
        let delta_x1 = p10.x - p11.x;
        let delta_x2 = p01.x - p11.x;
        let delta_x3 = p00.x - p10.x + p11.x - p01.x;
        let delta_y1 = p10.y - p11.y;
        let delta_y2 = p01.y - p11.y;
        let delta_y3 = p00.y - p10.y + p11.y - p01.y;

        let (projective_x, projective_y) =
            if delta_x3.abs() <= f32::EPSILON && delta_y3.abs() <= f32::EPSILON {
                (0.0, 0.0)
            } else {
                let denominator = delta_x1 * delta_y2 - delta_x2 * delta_y1;
                if !denominator.is_finite() || denominator == 0.0 {
                    return None;
                }
                (
                    (delta_x3 * delta_y2 - delta_x2 * delta_y3) / denominator,
                    (delta_x1 * delta_y3 - delta_x3 * delta_y1) / denominator,
                )
            };

        let transform = Self::from_rows([
            [
                p10.x - p00.x + projective_x * p10.x,
                p01.x - p00.x + projective_y * p01.x,
                p00.x,
            ],
            [
                p10.y - p00.y + projective_x * p10.y,
                p01.y - p00.y + projective_y * p01.y,
                p00.y,
            ],
            [projective_x, projective_y, 1.0],
        ]);
        (transform.is_finite() && transform.determinant() != 0.0).then_some(transform)
    }

    /// Returns the homogeneous `(x, y, w)` result without dividing by `w`.
    pub const fn transform_homogeneous(self, point: Point) -> Vector3 {
        self.matrix
            .transform_vector(Vector3::new(point.x, point.y, 1.0))
    }

    /// Projects a finite point, returning `None` when it maps to infinity or
    /// produces a non-finite coordinate.
    pub fn try_transform_point(self, point: Point) -> Option<Point> {
        let homogeneous = self.transform_homogeneous(point);
        if !homogeneous.is_finite() || homogeneous.z == 0.0 {
            return None;
        }

        let projected = Point::new(homogeneous.x / homogeneous.z, homogeneous.y / homogeneous.z);
        (projected.x.is_finite() && projected.y.is_finite()).then_some(projected)
    }

    /// Applies `self` first and `next` second.
    pub fn then(self, next: Self) -> Self {
        Self::new(self.matrix.then(next.matrix))
    }

    pub const fn determinant(self) -> f32 {
        self.matrix.determinant()
    }

    pub fn try_inverse(self) -> Option<Self> {
        Some(Self::new(self.matrix.try_inverse()?))
    }

    /// Recovers an Affine2 when the projective terms are exactly zero.
    pub fn try_into_affine(self) -> Option<Affine2> {
        let [[a, b, tx], [c, d, ty], [projective_x, projective_y, scale]] = self.rows();
        if !self.is_finite() || projective_x != 0.0 || projective_y != 0.0 || scale == 0.0 {
            return None;
        }

        let inverse_scale = scale.recip();
        Some(Affine2::new(
            Matrix2::from_columns(
                Vector2::new(a * inverse_scale, c * inverse_scale),
                Vector2::new(b * inverse_scale, d * inverse_scale),
            ),
            Vector2::new(tx * inverse_scale, ty * inverse_scale),
        ))
    }

    pub fn is_finite(self) -> bool {
        self.matrix.is_finite()
    }
}

impl From<Affine2> for Projective2 {
    fn from(affine: Affine2) -> Self {
        let linear = affine.linear();
        let x_axis = linear.x_axis();
        let y_axis = linear.y_axis();
        let translation = affine.translation();
        Self::new(Matrix3::from_columns(
            Vector3::new(x_axis.x, x_axis.y, 0.0),
            Vector3::new(y_axis.x, y_axis.y, 0.0),
            Vector3::new(translation.x, translation.y, 1.0),
        ))
    }
}

impl From<Matrix2> for Projective2 {
    fn from(matrix: Matrix2) -> Self {
        Self::from(Affine2::from_matrix(matrix))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_point_near(actual: Point, expected: Point) {
        assert!((actual.x - expected.x).abs() < 0.001);
        assert!((actual.y - expected.y).abs() < 0.001);
    }

    #[test]
    fn affine_round_trips_through_its_inverse() {
        let transform = Affine2::from_translation(Vector2::new(-100.0, 25.0))
            .then(Affine2::from_scale(Vector2::new(2.0, 0.5)));
        let inverse = transform.try_inverse().unwrap();
        let point = Point::new(123.0, -45.0);

        assert_point_near(
            inverse.transform_point(transform.transform_point(point)),
            point,
        );
    }

    #[test]
    fn zero_scale_has_no_inverse() {
        assert!(
            Affine2::from_scale(Vector2::new(0.0, 1.0))
                .try_inverse()
                .is_none()
        );
    }

    #[test]
    fn affine_embeds_losslessly_in_projective_space() {
        let affine = Affine2::new(
            Matrix2::from_columns(Vector2::new(2.0, 1.0), Vector2::new(-0.5, 3.0)),
            Vector2::new(20.0, -10.0),
        );
        let projective = Projective2::from(affine);
        let point = Point::new(4.0, 8.0);

        assert_eq!(
            projective.try_transform_point(point),
            Some(affine.transform_point(point))
        );
        assert_eq!(projective.try_into_affine(), Some(affine));
    }

    #[test]
    fn projective_terms_create_position_dependent_scale() {
        let projective =
            Projective2::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 1.0]]);

        assert_eq!(
            projective.try_transform_point(Point::new(2.0, 0.0)),
            Some(Point::new(1.0, 0.0))
        );
        assert!(projective.try_into_affine().is_none());
    }

    #[test]
    fn projective_round_trips_through_its_inverse() {
        let projective =
            Projective2::from_rows([[1.0, 0.2, 10.0], [-0.1, 2.0, -5.0], [0.001, -0.002, 1.0]]);
        let inverse = projective.try_inverse().unwrap();
        let point = Point::new(20.0, 30.0);
        let transformed = projective.try_transform_point(point).unwrap();

        assert_point_near(inverse.try_transform_point(transformed).unwrap(), point);
    }

    #[test]
    fn maps_unit_square_to_a_trapezoid() {
        let quad = [
            Point::new(-2.0, -1.0),
            Point::new(2.0, -1.0),
            Point::new(1.0, 1.0),
            Point::new(-1.0, 1.0),
        ];
        let transform = Projective2::from_unit_square_to_quad(quad).unwrap();
        let unit_corners = [
            Point::new(0.0, 0.0),
            Point::new(1.0, 0.0),
            Point::new(1.0, 1.0),
            Point::new(0.0, 1.0),
        ];

        for (unit, expected) in unit_corners.into_iter().zip(quad) {
            assert_point_near(transform.try_transform_point(unit).unwrap(), expected);
        }
    }
}
