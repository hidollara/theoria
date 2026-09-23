use crate::color::{Color, Srgb, UnitF32};

/// A logical color whose concrete representation has been erased at the
/// Canvas boundary without converting its coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CanvasColor {
    representation: Representation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Representation {
    SrgbF32(Color<Srgb, f32, UnitF32>),
    Srgb8(Color<Srgb, u8, u8>),
}

impl CanvasColor {
    pub(crate) fn to_straight_srgba(self) -> [f32; 4] {
        match self.representation {
            Representation::SrgbF32(color) => color.to_straight_srgba(),
            Representation::Srgb8(color) => {
                const BYTE_TO_UNIT: f32 = 1.0 / 255.0;
                let coordinates = color.coordinates();
                [
                    f32::from(coordinates[0]) * BYTE_TO_UNIT,
                    f32::from(coordinates[1]) * BYTE_TO_UNIT,
                    f32::from(coordinates[2]) * BYTE_TO_UNIT,
                    f32::from(color.opacity().sample()) * BYTE_TO_UNIT,
                ]
            }
        }
    }

    pub(crate) fn with_alpha_factor(&self, factor: f32) -> Self {
        let [red, green, blue, alpha] = self.to_straight_srgba();
        Color::rgba(red, green, blue, alpha * factor.clamp(0.0, 1.0))
            .expect("a finite unit opacity remains valid after multiplication")
            .into()
    }
}

impl From<Color<Srgb, f32, UnitF32>> for CanvasColor {
    fn from(color: Color<Srgb, f32, UnitF32>) -> Self {
        Self {
            representation: Representation::SrgbF32(color),
        }
    }
}

impl From<Color<Srgb, u8, u8>> for CanvasColor {
    fn from(color: Color<Srgb, u8, u8>) -> Self {
        Self {
            representation: Representation::Srgb8(color),
        }
    }
}

impl From<&CanvasColor> for CanvasColor {
    fn from(color: &CanvasColor) -> Self {
        *color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn erasing_a_color_does_not_normalize_its_storage() {
        let color = CanvasColor::from(Color::rgba8(255, 128, 64, 128));

        assert!(matches!(color.representation, Representation::Srgb8(_)));
        assert_eq!(
            color.to_straight_srgba(),
            [1.0, 128.0 / 255.0, 64.0 / 255.0, 128.0 / 255.0]
        );
    }
}
