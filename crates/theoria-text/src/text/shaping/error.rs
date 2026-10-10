#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::text) enum GlyphRunError {
    NonFinitePlacement,
}

impl std::fmt::Display for GlyphRunError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePlacement => {
                formatter.write_str("glyph offsets and advances must be finite")
            }
        }
    }
}

impl std::error::Error for GlyphRunError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::text) enum ShapingError {
    GlyphRun(GlyphRunError),
    NoFonts,
    MissingGlyph { byte_index: usize },
}

impl std::fmt::Display for ShapingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GlyphRun(error) => error.fmt(formatter),
            Self::NoFonts => formatter.write_str("font fallback requires at least one font"),
            Self::MissingGlyph { byte_index } => {
                write!(
                    formatter,
                    "no fallback font supports text at byte {byte_index}"
                )
            }
        }
    }
}

impl std::error::Error for ShapingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::GlyphRun(error) => Some(error),
            _ => None,
        }
    }
}

impl From<GlyphRunError> for ShapingError {
    fn from(error: GlyphRunError) -> Self {
        Self::GlyphRun(error)
    }
}
