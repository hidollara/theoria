/// Determines the RGB color in regions where source and destination overlap.
///
/// Blend modes do not define alpha, compositing, or geometric coverage.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    /// All blend modes defined by W3C Compositing and Blending Level 1.
    pub const ALL: [Self; 16] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];
}

/// Determines how source and destination RGBA values become a candidate result.
///
/// The region-selecting variants are the standard Porter-Duff operators.
/// Coverage is applied after this operation. `Plus` adds premultiplied
/// compositing values, including alpha, without imposing a semantic clamp.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Composite {
    /// Produces transparent black regardless of either operand.
    Clear,
    /// Replaces the backdrop with the source.
    Copy,
    /// Keeps the backdrop and ignores the source.
    Destination,
    /// Places the source over the backdrop using premultiplied alpha.
    #[default]
    SourceOver,
    /// Places the backdrop over the source.
    DestinationOver,
    /// Keeps the source only where the backdrop has coverage.
    SourceIn,
    /// Keeps the backdrop only where the source has coverage.
    DestinationIn,
    /// Keeps the source only where the backdrop is transparent.
    SourceOut,
    /// Keeps the backdrop only where the source is transparent.
    DestinationOut,
    /// Places the source inside the backdrop while preserving the backdrop outside it.
    SourceAtop,
    /// Places the backdrop inside the source while preserving the source outside it.
    DestinationAtop,
    /// Keeps portions covered by exactly one operand.
    Xor,
    /// Adds source and destination premultiplied RGBA values.
    Plus,
}

impl Composite {
    pub(crate) const ALL: [Self; 13] = [
        Self::Clear,
        Self::Copy,
        Self::Destination,
        Self::SourceOver,
        Self::DestinationOver,
        Self::SourceIn,
        Self::DestinationIn,
        Self::SourceOut,
        Self::DestinationOut,
        Self::SourceAtop,
        Self::DestinationAtop,
        Self::Xor,
        Self::Plus,
    ];
}
