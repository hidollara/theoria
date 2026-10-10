use std::sync::Arc;

use crate::{
    font::Font,
    text::{HorizontalText, Span, Text, TextStyle},
};

pub(super) fn bundled_latin_font() -> Font {
    Font::from_bytes(font_test_data::AHEM).expect("the bundled Ahem test font is valid")
}

pub(super) fn horizontal(
    fonts: &[Font],
    font_size: f32,
    content: impl Into<Arc<str>>,
) -> HorizontalText {
    Text::horizontal(TextStyle::new(fonts, font_size)).push(Span::new(content))
}

#[cfg(target_os = "windows")]
pub(super) fn windows_test_font() -> Option<Font> {
    load_windows_font("C:\\Windows\\Fonts\\segoeui.ttf")
}

#[cfg(not(target_os = "windows"))]
pub(super) fn windows_test_font() -> Option<Font> {
    None
}

#[cfg(target_os = "windows")]
pub(super) fn windows_vertical_fonts() -> Option<Vec<Font>> {
    let latin = windows_test_font()?;
    let japanese = load_windows_font("C:\\Windows\\Fonts\\YuGothR.ttc")
        .or_else(|| load_windows_font("C:\\Windows\\Fonts\\meiryo.ttc"))?;
    let fallback = load_windows_font("C:\\Windows\\Fonts\\seguiemj.ttf");
    Some(match fallback {
        Some(fallback) => vec![japanese, fallback, latin],
        None => vec![japanese, latin],
    })
}

#[cfg(not(target_os = "windows"))]
pub(super) fn windows_vertical_fonts() -> Option<Vec<Font>> {
    None
}

#[cfg(target_os = "windows")]
fn load_windows_font(path: &str) -> Option<Font> {
    let bytes = std::fs::read(path).ok()?;
    Font::from_bytes(Arc::<[u8]>::from(bytes)).ok()
}
