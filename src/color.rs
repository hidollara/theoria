mod white_point;

pub use white_point::{WhitePoint, WhitePointError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Color {
    /// Straight-alpha, linear-sRGB components used for rendering calculations.
    pub(super) red: f32,
    pub(super) green: f32,
    pub(super) blue: f32,
    pub(super) alpha: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorError {
    NonFiniteComponent,
    ComponentOutOfRange,
}

impl std::fmt::Display for ColorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteComponent => formatter.write_str("color components must be finite"),
            Self::ComponentOutOfRange => {
                formatter.write_str("sRGB and alpha components must be between 0 and 1")
            }
        }
    }
}

impl std::error::Error for ColorError {}

impl Color {
    /// Creates an opaque color from normalized sRGB components.
    ///
    /// Returns an error for non-finite components or values outside `[0, 1]`.
    pub fn rgb(red: f32, green: f32, blue: f32) -> Result<Self, ColorError> {
        Self::rgba(red, green, blue, 1.0)
    }

    /// Creates a straight-alpha color from normalized sRGB and linear alpha.
    ///
    /// Returns an error for non-finite components or values outside `[0, 1]`.
    pub fn rgba(red: f32, green: f32, blue: f32, alpha: f32) -> Result<Self, ColorError> {
        validate_srgba([red, green, blue, alpha])?;
        Ok(Self::from_valid_srgba(red, green, blue, alpha))
    }

    /// Creates an opaque color from 8-bit sRGB components.
    pub fn rgb8(red: u8, green: u8, blue: u8) -> Self {
        Self::rgba8(red, green, blue, u8::MAX)
    }

    /// Creates a straight-alpha color from 8-bit sRGB and linear alpha.
    pub fn rgba8(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
        Self::from_valid_srgba(
            f32::from(red) * BYTE_TO_UNIT,
            f32::from(green) * BYTE_TO_UNIT,
            f32::from(blue) * BYTE_TO_UNIT,
            f32::from(alpha) * BYTE_TO_UNIT,
        )
    }

    fn from_valid_srgba(red: f32, green: f32, blue: f32, alpha: f32) -> Self {
        Self {
            red: srgb_to_linear(red),
            green: srgb_to_linear(green),
            blue: srgb_to_linear(blue),
            alpha,
        }
    }

    pub(crate) const fn to_premultiplied_array(self) -> [f32; 4] {
        [
            self.red * self.alpha,
            self.green * self.alpha,
            self.blue * self.alpha,
            self.alpha,
        ]
    }

    pub(crate) fn to_srgba(self) -> [f32; 4] {
        let encode = |component: f32| {
            if component <= 0.003_130_8 {
                component * 12.92
            } else {
                1.055 * component.powf(1.0 / 2.4) - 0.055
            }
            .clamp(0.0, 1.0)
        };
        [
            encode(self.red),
            encode(self.green),
            encode(self.blue),
            self.alpha.clamp(0.0, 1.0),
        ]
    }

    pub(crate) fn with_alpha_factor(mut self, factor: f32) -> Self {
        self.alpha *= factor.clamp(0.0, 1.0);
        self
    }
}

fn srgb_to_linear(component: f32) -> f32 {
    if component <= 0.04045 {
        component / 12.92
    } else {
        ((component + 0.055) / 1.055).powf(2.4)
    }
}

fn validate_srgba(components: [f32; 4]) -> Result<(), ColorError> {
    if components.iter().any(|component| !component.is_finite()) {
        return Err(ColorError::NonFiniteComponent);
    }
    if components
        .iter()
        .any(|component| !(0.0..=1.0).contains(component))
    {
        return Err(ColorError::ComponentOutOfRange);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_straight_color_to_premultiplied_for_rendering() {
        let color = Color {
            red: 0.5,
            green: 0.25,
            blue: 1.0,
            alpha: 0.5,
        };
        assert_eq!(color.to_premultiplied_array(), [0.25, 0.125, 0.5, 0.5]);
    }

    #[test]
    fn converts_srgb_to_linear_for_rendering() {
        let linear = Color::rgb(0.5, 0.5, 0.5).unwrap().to_premultiplied_array();
        for component in &linear[..3] {
            assert!((*component - 0.214_041_14).abs() < 0.000_001);
        }
        assert_eq!(linear[3], 1.0);
        assert_eq!(
            Color::rgb8(128, 128, 128),
            Color::rgb(128.0 / 255.0, 128.0 / 255.0, 128.0 / 255.0).unwrap()
        );
    }

    #[test]
    fn rejects_invalid_color_components() {
        assert_eq!(
            Color::rgba(f32::NAN, 0.0, 0.0, 1.0),
            Err(ColorError::NonFiniteComponent)
        );
        assert_eq!(
            Color::rgba(1.1, 0.0, 0.0, 1.0),
            Err(ColorError::ComponentOutOfRange)
        );
        assert_eq!(
            Color::rgba(1.0, 0.0, 0.0, -0.1),
            Err(ColorError::ComponentOutOfRange)
        );
    }
}
