use super::InlineAlignment;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TabAlignment {
    #[default]
    Start,
    End,
    Center,
    Decimal(char),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TabStop {
    position: f32,
    alignment: TabAlignment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabStopError {
    NonFinitePosition,
    NegativePosition,
    UnorderedPositions,
}

impl std::fmt::Display for TabStopError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NonFinitePosition => formatter.write_str("tab stop position must be finite"),
            Self::NegativePosition => formatter.write_str("tab stop position must be non-negative"),
            Self::UnorderedPositions => {
                formatter.write_str("tab stop positions must be strictly increasing")
            }
        }
    }
}

impl std::error::Error for TabStopError {}

impl TabStop {
    pub fn new(position: f32, alignment: TabAlignment) -> Result<Self, TabStopError> {
        if !position.is_finite() {
            return Err(TabStopError::NonFinitePosition);
        }
        if position < 0.0 {
            return Err(TabStopError::NegativePosition);
        }
        Ok(Self {
            position,
            alignment,
        })
    }

    pub const fn position(self) -> f32 {
        self.position
    }

    pub const fn alignment(self) -> TabAlignment {
        self.alignment
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TabStops(Arc<[TabStop]>);

impl TabStops {
    pub fn new(stops: impl IntoIterator<Item = TabStop>) -> Result<Self, TabStopError> {
        let stops: Vec<_> = stops.into_iter().collect();
        if stops
            .windows(2)
            .any(|pair| pair[0].position >= pair[1].position)
        {
            return Err(TabStopError::UnorderedPositions);
        }
        Ok(Self(stops.into()))
    }

    pub fn as_slice(&self) -> &[TabStop] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Paragraph-level geometry, kept separate from per-span shaping style.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ParagraphStyle {
    first_line_indent: f32,
    start_indent: f32,
    end_indent: f32,
    spacing_before: f32,
    spacing_after: f32,
    alignment: Option<InlineAlignment>,
}

impl ParagraphStyle {
    pub const fn new() -> Self {
        Self {
            first_line_indent: 0.0,
            start_indent: 0.0,
            end_indent: 0.0,
            spacing_before: 0.0,
            spacing_after: 0.0,
            alignment: None,
        }
    }

    pub const fn with_first_line_indent(mut self, value: f32) -> Self {
        self.first_line_indent = value;
        self
    }

    pub const fn with_start_indent(mut self, value: f32) -> Self {
        self.start_indent = value;
        self
    }

    pub const fn with_end_indent(mut self, value: f32) -> Self {
        self.end_indent = value;
        self
    }

    pub const fn with_spacing_before(mut self, value: f32) -> Self {
        self.spacing_before = value;
        self
    }

    pub const fn with_spacing_after(mut self, value: f32) -> Self {
        self.spacing_after = value;
        self
    }

    pub const fn with_alignment(mut self, alignment: InlineAlignment) -> Self {
        self.alignment = Some(alignment);
        self
    }

    pub const fn first_line_indent(self) -> f32 {
        self.first_line_indent
    }
    pub const fn start_indent(self) -> f32 {
        self.start_indent
    }
    pub const fn end_indent(self) -> f32 {
        self.end_indent
    }
    pub const fn spacing_before(self) -> f32 {
        self.spacing_before
    }
    pub const fn spacing_after(self) -> f32 {
        self.spacing_after
    }
    pub const fn alignment(self) -> Option<InlineAlignment> {
        self.alignment
    }
}

/// Bounds expansion performed by explicit justified alignment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct JustificationOptions {
    maximum_spacing_expansion: Option<f32>,
    maximum_tatweels_per_line: usize,
}

impl JustificationOptions {
    pub const fn new() -> Self {
        Self {
            maximum_spacing_expansion: None,
            maximum_tatweels_per_line: 64,
        }
    }

    pub const fn with_maximum_spacing_expansion(mut self, value: Option<f32>) -> Self {
        self.maximum_spacing_expansion = value;
        self
    }

    pub const fn with_maximum_tatweels_per_line(mut self, value: usize) -> Self {
        self.maximum_tatweels_per_line = value;
        self
    }

    pub const fn maximum_spacing_expansion(self) -> Option<f32> {
        self.maximum_spacing_expansion
    }
    pub const fn maximum_tatweels_per_line(self) -> usize {
        self.maximum_tatweels_per_line
    }
}

impl Default for JustificationOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// What a bounded UI text component does when a line limit hides content.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextOverflow {
    #[default]
    Visible,
    Ellipsis,
}

/// Whether terminal punctuation may consume ink outside the line box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HangingPunctuation {
    #[default]
    None,
    End,
}
