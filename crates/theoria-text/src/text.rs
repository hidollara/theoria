mod error;
mod layout;
mod paint;
mod realization;
mod shaping;
mod style;
#[cfg(test)]
mod test_support;
mod unicode;

pub use error::TextError;
pub use layout::{
    HangingPunctuation, HorizontalText, HyphenationMark, HyphenationOpportunity,
    HyphenationOptions, Hyphenator, InlineAlignment, JustificationOptions, LineBreakStrategy,
    LineBreakStyle, OverflowWrap, ParagraphStyle, TabAlignment, TabStop, TabStopError, TabStops,
    Text, TextAlignment, TextBounds, TextLineInfo, TextOverflow, TextVerticalAlignment,
    VerticalColumnDirection, VerticalText, WritingMode,
};
pub(in crate::text) use paint::TextPaint;
pub use paint::{TextDecoration, TextDecorationKind};
pub use shaping::{
    HorizontalDirection, MissingGlyphPolicy, OpenTypeFeature, OpenTypeFeatureTag, ShapingOptions,
};
pub use style::{Span, TextStyle, VerticalOrientation};
