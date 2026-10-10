use std::ops::{Add, Mul, Neg, Sub};

use crate::geometry::Offset as Vector2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Vector3 {
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

impl Add for Vector3 {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self::new(self.x + other.x, self.y + other.y, self.z + other.z)
    }
}

impl Sub for Vector3 {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self::new(self.x - other.x, self.y - other.y, self.z - other.z)
    }
}

impl Neg for Vector3 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y, -self.z)
    }
}

impl Mul<f32> for Vector3 {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self::Output {
        Self::new(self.x * scalar, self.y * scalar, self.z * scalar)
    }
}

/// A coordinate-system-neutral 2x2 linear transformation, stored by columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix2 {
    x_axis: Vector2,
    y_axis: Vector2,
}

impl Matrix2 {
    pub const IDENTITY: Self = Self::from_columns(Vector2::new(1.0, 0.0), Vector2::new(0.0, 1.0));

    pub const fn from_columns(x_axis: Vector2, y_axis: Vector2) -> Self {
        Self { x_axis, y_axis }
    }

    #[cfg(test)]
    pub const fn from_scale(scale: Vector2) -> Self {
        Self::from_columns(Vector2::new(scale.x, 0.0), Vector2::new(0.0, scale.y))
    }

    #[cfg(test)]
    pub fn from_angle(radians: f32) -> Self {
        let (sin, cos) = radians.sin_cos();
        Self::from_columns(Vector2::new(cos, sin), Vector2::new(-sin, cos))
    }

    pub const fn x_axis(self) -> Vector2 {
        self.x_axis
    }

    pub const fn y_axis(self) -> Vector2 {
        self.y_axis
    }

    pub const fn transform_vector(self, vector: Vector2) -> Vector2 {
        Vector2::new(
            self.x_axis.x * vector.x + self.y_axis.x * vector.y,
            self.x_axis.y * vector.x + self.y_axis.y * vector.y,
        )
    }

    /// Applies `self` first and `next` second.
    pub fn then(self, next: Self) -> Self {
        Self::from_columns(
            next.transform_vector(self.x_axis),
            next.transform_vector(self.y_axis),
        )
    }

    pub const fn determinant(self) -> f32 {
        self.x_axis.x * self.y_axis.y - self.y_axis.x * self.x_axis.y
    }

    pub fn try_inverse(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }

        let inverse_determinant = determinant.recip();
        Some(Self::from_columns(
            Vector2::new(
                self.y_axis.y * inverse_determinant,
                -self.x_axis.y * inverse_determinant,
            ),
            Vector2::new(
                -self.y_axis.x * inverse_determinant,
                self.x_axis.x * inverse_determinant,
            ),
        ))
    }
}

/// A coordinate-system-neutral 3x3 linear transformation, stored by columns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Matrix3 {
    x_axis: Vector3,
    y_axis: Vector3,
    z_axis: Vector3,
}

impl Matrix3 {
    pub const IDENTITY: Self = Self::from_columns(
        Vector3::new(1.0, 0.0, 0.0),
        Vector3::new(0.0, 1.0, 0.0),
        Vector3::new(0.0, 0.0, 1.0),
    );

    pub const fn from_columns(x_axis: Vector3, y_axis: Vector3, z_axis: Vector3) -> Self {
        Self {
            x_axis,
            y_axis,
            z_axis,
        }
    }

    pub const fn from_rows(rows: [[f32; 3]; 3]) -> Self {
        let [[a, b, c], [d, e, f], [g, h, i]] = rows;
        Self::from_columns(
            Vector3::new(a, d, g),
            Vector3::new(b, e, h),
            Vector3::new(c, f, i),
        )
    }

    pub const fn rows(self) -> [[f32; 3]; 3] {
        [
            [self.x_axis.x, self.y_axis.x, self.z_axis.x],
            [self.x_axis.y, self.y_axis.y, self.z_axis.y],
            [self.x_axis.z, self.y_axis.z, self.z_axis.z],
        ]
    }

    pub const fn transform_vector(self, vector: Vector3) -> Vector3 {
        Vector3::new(
            self.x_axis.x * vector.x + self.y_axis.x * vector.y + self.z_axis.x * vector.z,
            self.x_axis.y * vector.x + self.y_axis.y * vector.y + self.z_axis.y * vector.z,
            self.x_axis.z * vector.x + self.y_axis.z * vector.y + self.z_axis.z * vector.z,
        )
    }

    /// Applies `self` first and `next` second.
    pub fn then(self, next: Self) -> Self {
        Self::from_columns(
            next.transform_vector(self.x_axis),
            next.transform_vector(self.y_axis),
            next.transform_vector(self.z_axis),
        )
    }

    pub const fn determinant(self) -> f32 {
        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows();
        a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g)
    }

    pub fn try_inverse(self) -> Option<Self> {
        let determinant = self.determinant();
        if !determinant.is_finite() || determinant == 0.0 {
            return None;
        }

        let [[a, b, c], [d, e, f], [g, h, i]] = self.rows();
        let inverse = Self::from_rows([
            [e * i - f * h, c * h - b * i, b * f - c * e],
            [f * g - d * i, a * i - c * g, c * d - a * f],
            [d * h - e * g, b * g - a * h, a * e - b * d],
        ]);
        Some(inverse.scaled(determinant.recip()))
    }

    fn scaled(self, scalar: f32) -> Self {
        Self::from_columns(
            self.x_axis * scalar,
            self.y_axis * scalar,
            self.z_axis * scalar,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matrix2_composition_applies_in_documented_order() {
        let rotation = Matrix2::from_angle(std::f32::consts::FRAC_PI_2);
        let scale = Matrix2::from_scale(Vector2::new(2.0, 3.0));
        let transformed = rotation
            .then(scale)
            .transform_vector(Vector2::new(1.0, 0.0));

        assert!(transformed.x.abs() < 0.001);
        assert!((transformed.y - 3.0).abs() < 0.001);
    }

    #[test]
    fn matrix3_round_trips_through_its_inverse() {
        let matrix = Matrix3::from_rows([[2.0, 0.5, 10.0], [-1.0, 3.0, -4.0], [0.25, 0.0, 1.0]]);
        let inverse = matrix.try_inverse().unwrap();
        let vector = Vector3::new(4.0, -2.0, 1.0);
        let round_trip = inverse.transform_vector(matrix.transform_vector(vector));

        assert!((round_trip.x - vector.x).abs() < 0.001);
        assert!((round_trip.y - vector.y).abs() < 0.001);
        assert!((round_trip.z - vector.z).abs() < 0.001);
    }

    #[test]
    fn matrix3_composition_applies_in_documented_order() {
        let first = Matrix3::from_rows([[2.0, 0.0, 0.0], [0.0, 3.0, 0.0], [0.0, 0.0, 1.0]]);
        let next = Matrix3::from_rows([[1.0, 0.0, 5.0], [0.0, 1.0, -2.0], [0.0, 0.0, 1.0]]);
        let vector = Vector3::new(4.0, 2.0, 1.0);

        assert_eq!(
            first.then(next).transform_vector(vector),
            Vector3::new(13.0, 4.0, 1.0)
        );
    }
}
