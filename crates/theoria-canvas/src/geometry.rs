use std::ops::{Add, Mul, Neg, Sub};

/// A displacement between two points in Canvas coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Offset {
    pub x: f32,
    pub y: f32,
}

impl Offset {
    pub const ZERO: Self = Self::new(0.0, 0.0);

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub const fn dot(self, other: Self) -> f32 {
        self.x * other.x + self.y * other.y
    }

    pub fn length(self) -> f32 {
        self.dot(self).sqrt()
    }
}

impl Add for Offset {
    type Output = Self;

    fn add(self, other: Self) -> Self::Output {
        Self::new(self.x + other.x, self.y + other.y)
    }
}

impl Sub for Offset {
    type Output = Self;

    fn sub(self, other: Self) -> Self::Output {
        Self::new(self.x - other.x, self.y - other.y)
    }
}

impl Neg for Offset {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self::new(-self.x, -self.y)
    }
}

impl Mul<f32> for Offset {
    type Output = Self;

    fn mul(self, scalar: f32) -> Self::Output {
        Self::new(self.x * scalar, self.y * scalar)
    }
}

/// The x- and y-axis radii of an ellipse, independent of its rotation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Radii {
    pub x: f32,
    pub y: f32,
}

impl Radii {
    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

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

impl Add<Offset> for Point {
    type Output = Self;

    fn add(self, vector: Offset) -> Self::Output {
        Self::new(self.x + vector.x, self.y + vector.y)
    }
}

impl Sub<Offset> for Point {
    type Output = Self;

    fn sub(self, vector: Offset) -> Self::Output {
        Self::new(self.x - vector.x, self.y - vector.y)
    }
}

impl Sub for Point {
    type Output = Offset;

    fn sub(self, other: Self) -> Offset {
        Offset::new(self.x - other.x, self.y - other.y)
    }
}
