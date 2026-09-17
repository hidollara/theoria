use std::fmt;

/// The chromaticity treated as neutral white by a color space or transform.
///
/// White points are stored as CIE 1931 `xy` chromaticity coordinates. They do
/// not describe the complete spectral power distribution of an illuminant.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WhitePoint {
    x: f64,
    y: f64,
}

impl WhitePoint {
    /// CIE standard illuminant D50.
    ///
    /// This is the standardized chromaticity, rather than a value recomputed
    /// from its nominal correlated color temperature.
    pub const D50: Self = Self {
        x: 0.3457,
        y: 0.3585,
    };

    /// CIE standard illuminant D65.
    ///
    /// This is the standardized chromaticity, rather than a value recomputed
    /// from its nominal correlated color temperature.
    pub const D65: Self = Self {
        x: 0.3127,
        y: 0.3290,
    };

    /// Creates a white point from CIE 1931 `xy` chromaticity coordinates.
    pub const fn from_xy(x: f64, y: f64) -> Result<Self, WhitePointError> {
        if !x.is_finite() || !y.is_finite() {
            return Err(WhitePointError::NonFiniteChromaticity);
        }
        if x < 0.0 || y <= 0.0 || x + y > 1.0 {
            return Err(WhitePointError::InvalidChromaticity);
        }
        Ok(Self { x, y })
    }

    /// Computes a white point on the CIE D-series daylight locus.
    ///
    /// The correlated color temperature must be between 4,000 K and 25,000 K,
    /// the range defined for this daylight-locus approximation. This does not
    /// compute a point on the Planckian locus.
    pub const fn daylight(cct_kelvin: f64) -> Result<Self, WhitePointError> {
        if !cct_kelvin.is_finite() {
            return Err(WhitePointError::NonFiniteTemperature);
        }
        if cct_kelvin < 4_000.0 || cct_kelvin > 25_000.0 {
            return Err(WhitePointError::DaylightTemperatureOutOfRange);
        }

        let squared = cct_kelvin * cct_kelvin;
        let cubed = squared * cct_kelvin;
        let x = if cct_kelvin <= 7_000.0 {
            -4.607_0e9 / cubed + 2.967_8e6 / squared + 99.11 / cct_kelvin + 0.244_063
        } else {
            -2.006_4e9 / cubed + 1.901_8e6 / squared + 247.48 / cct_kelvin + 0.237_040
        };
        let y = -3.0 * x * x + 2.87 * x - 0.275;

        Ok(Self { x, y })
    }

    pub const fn x(self) -> f64 {
        self.x
    }

    pub const fn y(self) -> f64 {
        self.y
    }

    /// Returns CIE XYZ tristimulus values normalized so that `Y` is 1.
    pub const fn normalized_xyz(self) -> [f64; 3] {
        [self.x / self.y, 1.0, (1.0 - self.x - self.y) / self.y]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WhitePointError {
    NonFiniteChromaticity,
    InvalidChromaticity,
    NonFiniteTemperature,
    DaylightTemperatureOutOfRange,
}

impl fmt::Display for WhitePointError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteChromaticity => {
                formatter.write_str("white-point chromaticity must be finite")
            }
            Self::InvalidChromaticity => formatter
                .write_str("white-point chromaticity must satisfy x >= 0, y > 0, and x + y <= 1"),
            Self::NonFiniteTemperature => {
                formatter.write_str("daylight color temperature must be finite")
            }
            Self::DaylightTemperatureOutOfRange => formatter
                .write_str("daylight color temperature must be between 4,000 K and 25,000 K"),
        }
    }
}

impl std::error::Error for WhitePointError {}

#[cfg(test)]
mod tests {
    use super::*;

    const DAYLIGHT_6500: WhitePoint = match WhitePoint::daylight(6_500.0) {
        Ok(white_point) => white_point,
        Err(_) => panic!("6,500 K is in the CIE daylight range"),
    };

    #[test]
    fn standard_illuminants_use_standardized_chromaticities() {
        assert_eq!((WhitePoint::D50.x(), WhitePoint::D50.y()), (0.3457, 0.3585));
        assert_eq!((WhitePoint::D65.x(), WhitePoint::D65.y()), (0.3127, 0.3290));
    }

    #[test]
    fn daylight_can_be_computed_at_compile_time() {
        assert!((DAYLIGHT_6500.x() - WhitePoint::D65.x()).abs() < 0.000_2);
        assert!((DAYLIGHT_6500.y() - WhitePoint::D65.y()).abs() < 0.000_2);
        assert_ne!(DAYLIGHT_6500, WhitePoint::D65);
    }

    #[test]
    fn normalized_xyz_sets_luminance_to_one() {
        let [x, y, z] = WhitePoint::D65.normalized_xyz();
        assert!((x - 0.950_455_927).abs() < 0.000_000_001);
        assert_eq!(y, 1.0);
        assert!((z - 1.089_057_751).abs() < 0.000_000_001);
    }

    #[test]
    fn rejects_invalid_chromaticities() {
        assert_eq!(
            WhitePoint::from_xy(f64::NAN, 0.3),
            Err(WhitePointError::NonFiniteChromaticity)
        );
        assert_eq!(
            WhitePoint::from_xy(0.8, 0.3),
            Err(WhitePointError::InvalidChromaticity)
        );
        assert_eq!(
            WhitePoint::from_xy(0.3, 0.0),
            Err(WhitePointError::InvalidChromaticity)
        );
    }

    #[test]
    fn rejects_temperatures_outside_the_daylight_model() {
        assert_eq!(
            WhitePoint::daylight(f64::INFINITY),
            Err(WhitePointError::NonFiniteTemperature)
        );
        assert_eq!(
            WhitePoint::daylight(3_999.0),
            Err(WhitePointError::DaylightTemperatureOutOfRange)
        );
        assert_eq!(
            WhitePoint::daylight(25_001.0),
            Err(WhitePointError::DaylightTemperatureOutOfRange)
        );
    }
}
