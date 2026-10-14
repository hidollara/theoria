//! Text content, style overrides, and text layout.

use crate::implement;
use theoria_canvas::{Canvas, Color, ColorField, Coverage, Opacity, color::Srgb};
use theoria_text::{
    font::Font,
    text::{Span, Text, TextStyle},
};

#[implement]
impl Fonts {
    type Value = Vec<Font>;
}

#[derive(Clone, Debug)]
pub enum TextData {
    String(String, StyleOverride),
    Span(Vec<TextData>, StyleOverride),
    Break,
}

impl TextData {
    fn append_spans(self, style: &TextStyle, out: &mut Vec<Span>) -> Result<(), &'static str> {
        match self {
            TextData::String(text, overrides) => {
                out.push(Span::new(text).with_style(overrides.apply(style)?))
            }
            TextData::Span(nodes, overrides) => {
                let style = overrides.apply(style)?;
                for node in nodes {
                    node.append_spans(&style, out)?;
                }
            }
            TextData::Break => out.push(Span::new("\n").with_style(style.clone())),
        }
        Ok(())
    }
}

#[implement]
impl TextData {
    type Value = TextData;
}

#[derive(Clone, Debug, Default)]
pub struct StyleOverride {
    pub fonts: Option<Vec<Font>>,
    pub font_size: Option<f32>,
    pub foreground: Option<ColorField>,
}

impl StyleOverride {
    fn apply(&self, parent: &TextStyle) -> Result<TextStyle, &'static str> {
        let mut out = parent.clone();
        if let Some(fonts) = &self.fonts {
            out = out.with_fonts(fonts);
        }
        if let Some(size) = self.font_size {
            if !size.is_finite() || size < 0.0 {
                return Err("font size must be finite and nonnegative");
            }
            out = out.with_font_size(size);
        }
        if let Some(fill) = &self.foreground {
            out = out.with_foreground(fill.clone());
        }
        Ok(out)
    }
}

#[implement]
impl StyleOverride {
    type Value = StyleOverride;
}

#[implement]
impl s {
    forward!(t => text);

    #[output(name = "content")]
    fn string(input: Input<'_, Self>, cx: &crate::ReadContext<'_, '_>) -> TextData {
        let t = input.t().into_cloned();
        let fonts = input.fonts().into_cloned();
        let font_size = input.font_size().into_cloned();
        let foreground = input.foreground().into_cloned();

        TextData::String(
            t,
            StyleOverride {
                fonts,
                font_size,
                foreground: foreground.map(|f| f.field().read(cx).into_cloned()),
            },
        )
    }

    #[output(name = "style")]
    fn string_style(input: Input<'_, Self>, cx: &crate::ReadContext<'_, '_>) -> StyleOverride {
        let fonts = input.fonts().into_cloned();
        let font_size = input.font_size().into_cloned();
        let foreground = input.foreground().into_cloned();

        StyleOverride {
            fonts,
            font_size,
            foreground: foreground.map(|f| f.field().read(cx).into_cloned()),
        }
    }
}

#[implement]
impl span {
    #[output(name = "content")]
    fn span(input: Input<'_, Self>, cx: &crate::ReadContext<'_, '_>) -> TextData {
        let fonts = input.fonts().into_cloned();
        let font_size = input.font_size().into_cloned();
        let foreground = input.foreground().into_cloned();
        let children = input.children().into_cloned();

        TextData::Span(
            children
                .iter()
                .map(|c| (*c.content().read(cx)).clone())
                .collect(),
            StyleOverride {
                fonts,
                font_size,
                foreground: foreground.map(|f| f.field().read(cx).into_cloned()),
            },
        )
    }

    #[output(name = "text")]
    fn span_text(input: Input<'_, Self>, cx: &crate::ReadContext<'_, '_>) -> String {
        let children = input.children().into_cloned();

        children
            .iter()
            .map(|c| (*c.text().read(cx)).clone())
            .collect()
    }

    #[output(name = "style")]
    fn span_style(input: Input<'_, Self>, cx: &crate::ReadContext<'_, '_>) -> StyleOverride {
        let fonts = input.fonts().into_cloned();
        let font_size = input.font_size().into_cloned();
        let foreground = input.foreground().into_cloned();

        StyleOverride {
            fonts,
            font_size,
            foreground: foreground.map(|f| f.field().read(cx).into_cloned()),
        }
    }
}

#[implement]
impl br {
    #[output(name = "content")]
    fn br() -> TextData {
        TextData::Break
    }

    #[output(name = "text")]
    fn break_text() -> String {
        "\n".into()
    }

    #[output(name = "style")]
    fn break_style() -> StyleOverride {
        StyleOverride::default()
    }
}

#[implement]
impl Text {
    #[output(name = "drawing")]
    fn text(
        input: Input<'_, Self>,
        cx: &crate::ReadContext<'_, '_>,
    ) -> Result<Canvas, &'static str> {
        let fonts = input.fonts().into_cloned();
        let font_size = input.font_size().into_cloned();
        let foreground = input.foreground().into_cloned();
        let wrap_width = input.wrap_width().into_cloned();
        let children = input.children().into_cloned();

        if !font_size.is_finite() || font_size < 0.0 {
            return Err("font size must be finite and nonnegative");
        }
        let foreground = foreground.field().read(cx).into_cloned();
        let style = TextStyle::new(&fonts, font_size).with_foreground(foreground.clone());
        let mut parts = vec![];
        let content: Vec<_> = children
            .iter()
            .map(|child| child.content().read(cx).into_cloned())
            .collect();
        for node in content {
            node.append_spans(&style, &mut parts)?;
        }
        let mut text = Text::horizontal(style);
        for span in parts {
            text = text.push(span);
        }
        if let Some(width) = wrap_width {
            text = text.wrap_width(width);
        }
        let text = text
            .layout()
            .map_err(|_| "Text layout failed: check fonts, font size, and wrap width")?;
        Ok(text
            .to_canvas(foreground)
            .map_err(|_| "Text glyph realization failed")?
            .unwrap_or_else(|| {
                Canvas::draw(
                    Color::new(Srgb, [0u8; 3], Opacity::<u8>::from(0)),
                    Coverage::all(),
                )
            }))
    }
}
