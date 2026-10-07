use crate::{
    geometry::{Offset, Point},
    math::{Matrix2, Matrix3, Vector3},
};

/// A coordinate-system-neutral 2D affine transformation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Affine2 {
    linear: Matrix2,
    translation: Offset,
}

impl Affine2 {
    pub const IDENTITY: Self = Self::new(Matrix2::IDENTITY, Offset::ZERO);

    const fn new(linear: Matrix2, translation: Offset) -> Self {
        Self {
            linear,
            translation,
        }
    }

    /// Constructs the mapping `(x, y) -> (a*x + b*y + tx, c*x + d*y + ty)`.
    /// All coefficients must be finite. Singular affine mappings are valid.
    pub fn from_rows(rows: [[f32; 3]; 2]) -> Option<Self> {
        if !rows.iter().flatten().all(|value| value.is_finite()) {
            return None;
        }
        let [[a, b, tx], [c, d, ty]] = rows;
        Some(Self::new(
            Matrix2::from_columns(Offset::new(a, c), Offset::new(b, d)),
            Offset::new(tx, ty),
        ))
    }

    pub fn from_translation(offset: Offset) -> Option<Self> {
        Self::from_rows([[1.0, 0.0, offset.x], [0.0, 1.0, offset.y]])
    }

    /// Negative factors reflect an axis; zero factors collapse an axis.
    pub fn from_scale(x: f32, y: f32) -> Option<Self> {
        Self::from_rows([[x, 0.0, 0.0], [0.0, y, 0.0]])
    }

    /// Rotates by an angle in radians, from the positive x axis toward positive y.
    pub fn from_rotation(radians: f32) -> Option<Self> {
        let (sin, cos) = radians.sin_cos();
        Self::from_rows([[cos, -sin, 0.0], [sin, cos, 0.0]])
    }

    /// Applies `x' = x + x_by_y*y` and `y' = y + y_by_x*x`.
    pub fn from_shear(x_by_y: f32, y_by_x: f32) -> Option<Self> {
        Self::from_rows([[1.0, x_by_y, 0.0], [y_by_x, 1.0, 0.0]])
    }

    pub const fn rows(self) -> [[f32; 3]; 2] {
        let x = self.linear.x_axis();
        let y = self.linear.y_axis();
        [
            [x.x, y.x, self.translation.x],
            [x.y, y.y, self.translation.y],
        ]
    }

    pub const fn offset(self) -> Offset {
        self.translation
    }

    pub fn transform_point(self, point: Point) -> Point {
        let transformed = self.linear.transform_vector(Offset::new(point.x, point.y));
        Point::new(
            transformed.x + self.translation.x,
            transformed.y + self.translation.y,
        )
    }

    pub const fn transform_offset(self, vector: Offset) -> Offset {
        self.linear.transform_vector(vector)
    }

    /// Applies `self` first and `next` second; returns `None` on overflow.
    pub fn then(self, next: Self) -> Option<Self> {
        let result = Self::new(
            self.linear.then(next.linear),
            next.linear.transform_vector(self.translation) + next.translation,
        );
        Self::from_rows(result.rows())
    }

    pub fn try_inverse(self) -> Option<Self> {
        let linear = self.linear.try_inverse()?;
        Self::from_rows(Self::new(linear, linear.transform_vector(-self.translation)).rows())
    }
}

/// A coordinate-system-neutral projective transformation of a 2D plane.
///
/// The internal matrix acts on homogeneous `(x, y, 1)` vectors. Matrices that
/// differ only by a non-zero scalar represent the same projective mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Projective2 {
    matrix: Matrix3,
}

impl Projective2 {
    pub const IDENTITY: Self = Self::new(Matrix3::IDENTITY);

    const fn new(matrix: Matrix3) -> Self {
        Self { matrix }
    }

    /// All coefficients must be finite, and the homogeneous denominator must
    /// not be identically zero. A particular point may still map to infinity.
    pub fn from_rows(rows: [[f32; 3]; 3]) -> Option<Self> {
        if !rows.iter().flatten().all(|value| value.is_finite())
            || rows[2].iter().all(|value| *value == 0.0)
        {
            return None;
        }
        Some(Self::new(Matrix3::from_rows(rows)))
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
        ])?;
        (transform.determinant() != 0.0).then_some(transform)
    }

    /// Returns the homogeneous `(x, y, w)` result without dividing by `w`.
    pub const fn transform_homogeneous(self, point: Point) -> HomogeneousPoint {
        let result = self
            .matrix
            .transform_vector(Vector3::new(point.x, point.y, 1.0));
        HomogeneousPoint {
            x: result.x,
            y: result.y,
            w: result.z,
        }
    }

    /// Projects a finite point, returning `None` when it maps to infinity or
    /// produces a non-finite coordinate.
    pub fn try_transform_point(self, point: Point) -> Option<Point> {
        let homogeneous = self.transform_homogeneous(point);
        if !(homogeneous.x.is_finite() && homogeneous.y.is_finite() && homogeneous.w.is_finite())
            || homogeneous.w == 0.0
        {
            return None;
        }

        let projected = Point::new(homogeneous.x / homogeneous.w, homogeneous.y / homogeneous.w);
        (projected.x.is_finite() && projected.y.is_finite()).then_some(projected)
    }

    /// Applies `self` first and `next` second; returns `None` on overflow or when
    /// the composed mapping has no projectable points.
    pub fn then(self, next: Self) -> Option<Self> {
        Self::from_rows(self.matrix.then(next.matrix).rows())
    }

    const fn determinant(self) -> f32 {
        self.matrix.determinant()
    }

    pub fn try_inverse(self) -> Option<Self> {
        Self::from_rows(self.matrix.try_inverse()?.rows())
    }

    /// Recovers an Affine2 when the projective terms are exactly zero.
    pub fn try_into_affine(self) -> Option<Affine2> {
        let [[a, b, tx], [c, d, ty], [projective_x, projective_y, scale]] = self.rows();
        if projective_x != 0.0 || projective_y != 0.0 || scale == 0.0 {
            return None;
        }

        let inverse_scale = scale.recip();
        Affine2::from_rows([
            [a * inverse_scale, b * inverse_scale, tx * inverse_scale],
            [c * inverse_scale, d * inverse_scale, ty * inverse_scale],
        ])
    }
}

impl From<Affine2> for Projective2 {
    fn from(affine: Affine2) -> Self {
        let linear = affine.linear;
        let x_axis = linear.x_axis();
        let y_axis = linear.y_axis();
        let translation = affine.translation;
        Self::new(Matrix3::from_columns(
            Vector3::new(x_axis.x, x_axis.y, 0.0),
            Vector3::new(y_axis.x, y_axis.y, 0.0),
            Vector3::new(translation.x, translation.y, 1.0),
        ))
    }
}

/// A projected point before perspective division.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HomogeneousPoint {
    pub x: f32,
    pub y: f32,
    pub w: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_finite_coefficients_and_parameters() {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for index in 0..6 {
                let mut rows = Affine2::IDENTITY.rows();
                rows[index / 3][index % 3] = invalid;
                assert!(Affine2::from_rows(rows).is_none());
            }
            for index in 0..9 {
                let mut rows = Projective2::IDENTITY.rows();
                rows[index / 3][index % 3] = invalid;
                assert!(Projective2::from_rows(rows).is_none());
            }
            assert!(Affine2::from_translation(Offset::new(invalid, 0.0)).is_none());
            assert!(Affine2::from_scale(1.0, invalid).is_none());
            assert!(Affine2::from_rotation(invalid).is_none());
            assert!(Affine2::from_shear(invalid, 0.0).is_none());
        }
    }

    #[test]
    fn rotation_shear_and_reflection_have_domain_semantics() {
        assert_point_near(
            Affine2::from_rotation(std::f32::consts::FRAC_PI_2)
                .unwrap()
                .transform_point(Point::new(1.0, 0.0)),
            Point::new(0.0, 1.0),
        );
        assert_eq!(
            Affine2::from_shear(0.5, 0.0)
                .unwrap()
                .transform_point(Point::new(2.0, 10.0)),
            Point::new(7.0, 10.0)
        );
        assert_eq!(
            Affine2::from_scale(-1.0, 2.0)
                .unwrap()
                .transform_point(Point::new(3.0, 4.0)),
            Point::new(-3.0, 8.0)
        );
    }

    #[test]
    fn rejects_unprojectable_mappings_but_allows_collapsed_axes() {
        assert!(Projective2::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0; 3]]).is_none());
        let collapsed = Affine2::from_scale(0.0, 1.0).unwrap();
        assert_eq!(
            Projective2::from(collapsed).try_transform_point(Point::new(2.0, 3.0)),
            Some(Point::new(0.0, 3.0))
        );
        assert!(Projective2::from(collapsed).try_inverse().is_none());
    }

    #[test]
    fn composition_and_inverse_reject_non_finite_results() {
        let large = Affine2::from_scale(f32::MAX, 1.0).unwrap();
        let twice = Affine2::from_scale(2.0, 1.0).unwrap();
        assert!(large.then(twice).is_none());
        assert!(Projective2::from(large).then(twice.into()).is_none());
        let tiny = Affine2::from_scale(f32::from_bits(1), 1.0).unwrap();
        assert!(tiny.try_inverse().is_none());
        assert!(Projective2::from(tiny).try_inverse().is_none());
    }

    fn assert_point_near(actual: Point, expected: Point) {
        assert!((actual.x - expected.x).abs() < 0.001);
        assert!((actual.y - expected.y).abs() < 0.001);
    }

    #[test]
    fn affine_round_trips_through_its_inverse() {
        let transform = Affine2::from_translation(Offset::new(-100.0, 25.0))
            .expect("finite translation")
            .then(Affine2::from_scale(2.0, 0.5).expect("finite scale"))
            .unwrap();
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
            Affine2::from_scale(0.0, 1.0)
                .expect("finite scale")
                .try_inverse()
                .is_none()
        );
    }

    #[test]
    fn affine_embeds_losslessly_in_projective_space() {
        let affine = Affine2::from_rows([[2.0, -0.5, 20.0], [1.0, 3.0, -10.0]]).unwrap();
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
            Projective2::from_rows([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.5, 0.0, 1.0]])
                .expect("valid projective coefficients");

        assert_eq!(
            projective.try_transform_point(Point::new(2.0, 0.0)),
            Some(Point::new(1.0, 0.0))
        );
        assert!(projective.try_into_affine().is_none());
    }

    #[test]
    fn projective_round_trips_through_its_inverse() {
        let projective =
            Projective2::from_rows([[1.0, 0.2, 10.0], [-0.1, 2.0, -5.0], [0.001, -0.002, 1.0]])
                .expect("valid projective coefficients");
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
