use skrifa::MetadataProvider;

use super::{Font, FontError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct VariationSetting {
    pub(super) axis: VariationAxisTag,
    pub(super) value: f32,
}

/// The four-byte identity of a font variation axis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct VariationAxisTag([u8; 4]);

impl VariationAxisTag {
    pub const WEIGHT: Self = Self(*b"wght");
    pub const WIDTH: Self = Self(*b"wdth");
    pub const OPTICAL_SIZE: Self = Self(*b"opsz");
    pub const SLANT: Self = Self(*b"slnt");
    pub const ITALIC: Self = Self(*b"ital");

    pub const fn new(tag: [u8; 4]) -> Self {
        Self(tag)
    }

    pub const fn bytes(self) -> [u8; 4] {
        self.0
    }
}

/// Metadata for one variation axis declared by a font.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VariationAxis {
    tag: VariationAxisTag,
    minimum: f32,
    default: f32,
    maximum: f32,
    hidden: bool,
}

impl VariationAxis {
    pub const fn tag(self) -> VariationAxisTag {
        self.tag
    }

    pub const fn minimum(self) -> f32 {
        self.minimum
    }

    pub const fn default(self) -> f32 {
        self.default
    }

    pub const fn maximum(self) -> f32 {
        self.maximum
    }

    /// Returns whether the font asks applications to hide this axis from
    /// ordinary user interfaces.
    pub const fn is_hidden(self) -> bool {
        self.hidden
    }
}

impl Font {
    /// Returns a new immutable font instance at the given variable-font axis
    /// coordinate.
    ///
    /// The value uses the axis's user coordinate system and is clamped by the
    /// font when normalized. An axis absent from the selected face is ignored
    /// when the font is resolved. Repeating an axis replaces its previous
    /// value. Face bytes and shaping data remain shared with `self`.
    pub fn with_variation(mut self, axis: VariationAxisTag, value: f32) -> Result<Self, FontError> {
        if !value.is_finite() {
            return Err(FontError::NonFiniteVariation);
        }

        let mut settings = self.variation_settings.to_vec();
        if let Some(existing) = settings.iter_mut().find(|item| item.axis == axis) {
            existing.value = value;
        } else {
            settings.push(VariationSetting { axis, value });
        }
        settings.sort_by_key(|setting| setting.axis.bytes());
        self.variation_settings = settings.into();
        Ok(self)
    }

    /// Returns variation axes in the order declared by the selected face.
    ///
    /// Axis metadata is decoded on demand; it is not retained as part of the
    /// immutable face identity.
    pub fn variation_axes(&self) -> Vec<VariationAxis> {
        self.face
            .skrifa_ref()
            .axes()
            .iter()
            .map(|axis| VariationAxis {
                tag: VariationAxisTag::new(axis.tag().to_be_bytes()),
                minimum: axis.min_value(),
                default: axis.default_value(),
                maximum: axis.max_value(),
                hidden: axis.is_hidden(),
            })
            .collect()
    }
}
