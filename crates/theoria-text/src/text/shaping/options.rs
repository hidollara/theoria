/// The direction of one already-itemized horizontal text segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HorizontalDirection {
    LeftToRight,
    RightToLeft,
}

impl From<HorizontalDirection> for ShapingDirection {
    fn from(direction: HorizontalDirection) -> Self {
        match direction {
            HorizontalDirection::LeftToRight => Self::LeftToRight,
            HorizontalDirection::RightToLeft => Self::RightToLeft,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::text) enum ShapingDirection {
    LeftToRight,
    RightToLeft,
    TopToBottom,
    BottomToTop,
}

impl ShapingDirection {
    pub(in crate::text) const fn is_backward(self) -> bool {
        matches!(self, Self::RightToLeft | Self::BottomToTop)
    }
}

/// The four-byte identity of an OpenType Layout feature.
///
/// Feature tags and variation-axis tags share the same binary representation
/// in OpenType, but belong to different semantic namespaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpenTypeFeatureTag([u8; 4]);

impl OpenTypeFeatureTag {
    pub const STANDARD_LIGATURES: Self = Self(*b"liga");
    pub const CONTEXTUAL_LIGATURES: Self = Self(*b"clig");
    pub const CONTEXTUAL_ALTERNATES: Self = Self(*b"calt");
    pub const KERNING: Self = Self(*b"kern");
    pub const SMALL_CAPS: Self = Self(*b"smcp");
    pub const CAPITALS_TO_SMALL_CAPS: Self = Self(*b"c2sc");
    pub const OLDSTYLE_FIGURES: Self = Self(*b"onum");
    pub const LINING_FIGURES: Self = Self(*b"lnum");
    pub const PROPORTIONAL_FIGURES: Self = Self(*b"pnum");
    pub const TABULAR_FIGURES: Self = Self(*b"tnum");
    pub const SUPERSCRIPT: Self = Self(*b"sups");
    pub const SUBSCRIPT: Self = Self(*b"subs");
    pub const NUMERATOR: Self = Self(*b"numr");
    pub const DENOMINATOR: Self = Self(*b"dnom");
    pub const FRACTIONS: Self = Self(*b"frac");

    pub const fn new(tag: [u8; 4]) -> Self {
        Self(tag)
    }

    pub const fn bytes(self) -> [u8; 4] {
        self.0
    }

    pub const fn with_value(self, value: u32) -> OpenTypeFeature {
        OpenTypeFeature::new(self, value)
    }

    pub const fn enabled(self) -> OpenTypeFeature {
        self.with_value(1)
    }

    pub const fn disabled(self) -> OpenTypeFeature {
        self.with_value(0)
    }
}

/// One global OpenType feature setting for a shaped text segment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OpenTypeFeature {
    tag: OpenTypeFeatureTag,
    value: u32,
}

impl OpenTypeFeature {
    pub const fn new(tag: OpenTypeFeatureTag, value: u32) -> Self {
        Self { tag, value }
    }

    pub const fn tag(self) -> OpenTypeFeatureTag {
        self.tag
    }

    pub const fn value(self) -> u32 {
        self.value
    }
}

/// How shaping proceeds when no font in a span supports one extended
/// grapheme cluster.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MissingGlyphPolicy {
    /// Display U+FFFD from the first supporting font in the span's ordered
    /// fallback chain while retaining the original cluster's logical range.
    ///
    /// If none of the fonts supports U+FFFD, the first font's `.notdef` glyph
    /// is used as the final visible missing-character indication.
    #[default]
    Replace,
    /// Stop layout and report the first unsupported UTF-8 byte position.
    Error,
}

/// Optional shaping properties for one already-itemized text segment.
///
/// Unspecified direction and script are inferred by the shaping
/// implementation. This type does not perform bidi resolution, script
/// itemization, font fallback, line breaking or paragraph layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShapingOptions {
    pub(in crate::text) direction: Option<ShapingDirection>,
    pub(in crate::text) script: Option<[u8; 4]>,
    pub(in crate::text) language: Option<String>,
    pub(in crate::text) features: Vec<OpenTypeFeature>,
    pub(in crate::text) missing_glyph_policy: MissingGlyphPolicy,
}

impl ShapingOptions {
    pub const fn new() -> Self {
        Self {
            direction: None,
            script: None,
            language: None,
            features: Vec::new(),
            missing_glyph_policy: MissingGlyphPolicy::Replace,
        }
    }

    pub fn direction(mut self, direction: HorizontalDirection) -> Self {
        self.direction = Some(direction.into());
        self
    }

    pub(in crate::text) fn exact_direction(mut self, direction: ShapingDirection) -> Self {
        self.direction = Some(direction);
        self
    }

    /// Sets an ISO 15924 script tag such as `b"Latn"` or `b"Arab"`.
    pub fn script(mut self, script: [u8; 4]) -> Self {
        self.script = Some(script);
        self
    }

    /// Sets a BCP 47 language tag such as `"en"` or `"ja"`.
    ///
    /// An empty tag is treated as unspecified.
    pub fn language(mut self, language: impl Into<String>) -> Self {
        let language = language.into();
        self.language = (!language.is_empty()).then_some(language);
        self
    }

    pub fn feature(mut self, feature: OpenTypeFeature) -> Self {
        self.features.push(feature);
        self
    }

    pub const fn missing_glyph_policy(mut self, policy: MissingGlyphPolicy) -> Self {
        self.missing_glyph_policy = policy;
        self
    }
}

impl Default for ShapingOptions {
    fn default() -> Self {
        Self::new()
    }
}
