use std::ops::{Add, Sub};

use crate::math::Vector2;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

impl Point {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

impl Add<Vector2> for Point {
    type Output = Self;

    fn add(self, vector: Vector2) -> Self::Output {
        Self::new(self.x + vector.x, self.y + vector.y)
    }
}

impl Sub<Vector2> for Point {
    type Output = Self;

    fn sub(self, vector: Vector2) -> Self::Output {
        Self::new(self.x - vector.x, self.y - vector.y)
    }
}

impl Sub for Point {
    type Output = Vector2;

    fn sub(self, other: Self) -> Vector2 {
        Vector2::new(self.x - other.x, self.y - other.y)
    }
}
