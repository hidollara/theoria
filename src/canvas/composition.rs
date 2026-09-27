use crate::color::{ColorSpace, Rgb, TypedColorSpace};

use super::{Blend, BlendOperation, BlendShader};

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
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Composition {
    composite: Composite,
    composite_space: ColorSpace,
    blend: Option<Blend>,
}

impl Composition {
    pub fn new(composite: Composite, space: impl Into<ColorSpace>) -> Self {
        Self {
            composite,
            composite_space: space.into(),
            blend: None,
        }
    }

    pub fn with_blend<P, O>(mut self, operation: O, space: P) -> Self
    where
        P: TypedColorSpace<Model = Rgb> + Into<ColorSpace>,
        O: BlendOperation<P>,
    {
        self.blend = (!operation.is_identity()).then(|| Blend::new(operation, space));
        self
    }

    pub fn with_blend_shader(mut self, shader: BlendShader, space: impl Into<ColorSpace>) -> Self {
        self.blend = Some(Blend::from_shader(shader, space));
        self
    }

    pub const fn composite(&self) -> Composite {
        self.composite
    }

    pub fn composite_space(&self) -> &ColorSpace {
        &self.composite_space
    }

    pub const fn blend(&self) -> Option<&Blend> {
        self.blend.as_ref()
    }

    pub(crate) fn blend_shader(&self) -> &BlendShader {
        match &self.blend {
            Some(blend) => blend.shader(),
            None => BlendShader::identity(),
        }
    }

    pub(crate) fn requires_destination_read_in(&self, working_profile: &ColorSpace) -> bool {
        self.blend.is_some() || &self.composite_space != working_profile
    }
}
