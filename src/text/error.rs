use super::shaping::{GlyphRunError, ShapingError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextError {
    NonFiniteFontSize,
    NegativeFontSize,
    NonFinitePlacement,
    NoFonts,
    MissingGlyph { byte_index: usize },
    UnsupportedGlyph { glyph_index: u32 },
    NonFiniteMaxWidth,
    NegativeMaxWidth,
    NonFiniteLineHeight,
    NegativeLineHeight,
    NonFiniteLetterSpacing,
    NonFiniteWordSpacing,
    NonFiniteBaselineShift,
    NonFiniteTabSize,
    NegativeTabSize,
    NonFiniteColumnWidth,
    NegativeColumnWidth,
    NonFiniteHeight,
    NegativeHeight,
    NonFiniteParagraphMetric,
    NonFiniteJustificationExpansion,
    NegativeJustificationExpansion,
    NonFiniteDecorationMetric,
    NegativeDecorationThickness,
}

impl std::fmt::Display for TextError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFiniteFontSize => formatter.write_str("font size must be finite"),
            Self::NegativeFontSize => formatter.write_str("font size must not be negative"),
            Self::NonFinitePlacement => {
                formatter.write_str("glyph offsets and advances must be finite")
            }
            Self::NoFonts => formatter.write_str("text requires at least one fallback font"),
            Self::MissingGlyph { byte_index } => write!(
                formatter,
                "no fallback font supports text at byte {byte_index}"
            ),
            Self::UnsupportedGlyph { glyph_index } => {
                write!(
                    formatter,
                    "glyph {glyph_index} has no supported presentation"
                )
            }
            Self::NonFiniteMaxWidth => formatter.write_str("maximum width must be finite"),
            Self::NegativeMaxWidth => formatter.write_str("maximum width must not be negative"),
            Self::NonFiniteLineHeight => formatter.write_str("line height must be finite"),
            Self::NegativeLineHeight => formatter.write_str("line height must not be negative"),
            Self::NonFiniteLetterSpacing => formatter.write_str("letter spacing must be finite"),
            Self::NonFiniteWordSpacing => formatter.write_str("word spacing must be finite"),
            Self::NonFiniteBaselineShift => formatter.write_str("baseline shift must be finite"),
            Self::NonFiniteTabSize => formatter.write_str("tab size must be finite"),
            Self::NegativeTabSize => formatter.write_str("tab size must not be negative"),
            Self::NonFiniteColumnWidth => formatter.write_str("column width must be finite"),
            Self::NegativeColumnWidth => formatter.write_str("column width must not be negative"),
            Self::NonFiniteHeight => formatter.write_str("layout height must be finite"),
            Self::NegativeHeight => formatter.write_str("layout height must not be negative"),
            Self::NonFiniteParagraphMetric => {
                formatter.write_str("paragraph metrics must be finite")
            }
            Self::NonFiniteJustificationExpansion => {
                formatter.write_str("justification expansion must be finite")
            }
            Self::NegativeJustificationExpansion => {
                formatter.write_str("justification expansion must not be negative")
            }
            Self::NonFiniteDecorationMetric => {
                formatter.write_str("text decoration metrics must be finite")
            }
            Self::NegativeDecorationThickness => {
                formatter.write_str("text decoration thickness must not be negative")
            }
        }
    }
}

impl std::error::Error for TextError {}

impl From<ShapingError> for TextError {
    fn from(error: ShapingError) -> Self {
        match error {
            ShapingError::GlyphRun(error) => error.into(),
            ShapingError::NoFonts => Self::NoFonts,
            ShapingError::MissingGlyph { byte_index } => Self::MissingGlyph { byte_index },
        }
    }
}

impl From<GlyphRunError> for TextError {
    fn from(error: GlyphRunError) -> Self {
        match error {
            GlyphRunError::NonFinitePlacement => Self::NonFinitePlacement,
        }
    }
}
