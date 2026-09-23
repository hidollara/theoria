use crate::color::RgbColorSpace;

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

/// A blend-mode evaluation and the RGB space in which it is performed.
///
/// Canvas blend modes are deliberately defined for RGB coordinates. A color
/// using another model is converted to this space by the renderer before the
/// blend function is evaluated.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    mode: BlendMode,
    space: RgbColorSpace,
}

impl Blend {
    pub fn new(mode: BlendMode, space: impl Into<RgbColorSpace>) -> Self {
        Self {
            mode,
            space: space.into(),
        }
    }

    pub const fn mode(self) -> BlendMode {
        self.mode
    }

    pub const fn space(self) -> RgbColorSpace {
        self.space
    }
}

/// Determines how source and destination RGBA values become a candidate result.
///
/// The region-selecting variants are the standard Porter-Duff operators.
/// Porter-Duff coefficients are alpha-derived and mathematically apply to any
/// component vector; Canvas deliberately supplies RGB coordinates in the
/// selected composition space so the result remains an RGBA color field.
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

/// One binary RGBA Canvas composition with independently selected blend and
/// Porter-Duff evaluation spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Composition {
    composite: Composite,
    composite_space: RgbColorSpace,
    blend: Option<Blend>,
}

impl Composition {
    pub fn new(composite: Composite, space: impl Into<RgbColorSpace>) -> Self {
        Self {
            composite,
            composite_space: space.into(),
            blend: None,
        }
    }

    pub fn with_blend(mut self, mode: BlendMode, space: impl Into<RgbColorSpace>) -> Self {
        self.blend = (mode != BlendMode::Normal).then(|| Blend::new(mode, space));
        self
    }

    pub const fn composite(self) -> Composite {
        self.composite
    }

    pub const fn composite_space(self) -> RgbColorSpace {
        self.composite_space
    }

    pub const fn blend(self) -> Option<Blend> {
        self.blend
    }

    pub(crate) const fn requires_destination_read(self) -> bool {
        self.blend.is_some() || !self.composite_space.is_linear()
    }
}
