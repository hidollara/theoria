//! Organon's initial preview scene, using its generated drawings.
//!
//! The import macro compiles each XML file; Rust chooses module names and owns layout
//! calculations, state, evaluation timing, and presentation.

use theoria_canvas::{Canvas, CanvasView, Color, Opacity, color::Srgb, geometry::Point};
use theoria_renderer::SurfaceContent;
use theoria_text::font::Font;

mod label {
    organon::import!("src/preview/label.organon.xml");
}

mod panel {
    organon::import!("src/preview/panel.organon.xml");
}

/// A compiled multi-output element, available to ordinary Rust callers.
pub mod badge {
    organon::import!("src/preview/badge.organon.xml");
}

/// An interpreted preview using current authored data and explicit sample values.
pub fn element_content(
    element: &crate::Element,
    inputs: &crate::evaluation::Values,
    output: &str,
    width: f32,
    height: f32,
) -> Result<SurfaceContent, crate::evaluation::Error> {
    use crate::evaluation::{Error, Value};
    let mut outputs = element.evaluate(inputs)?;
    let canvas = match outputs.remove(output) {
        Some(Value::Canvas(canvas)) => canvas,
        Some(_) => return Err(Error(format!("output {output:?} is not a Canvas"))),
        None => return Err(Error(format!("unknown output {output:?}"))),
    };
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
        .ok_or_else(|| Error("invalid view dimensions".into()))?;
    Ok(SurfaceContent::new(
        canvas,
        view,
        Color::new(Srgb, [10_u8, 16, 26], Opacity::<u8>::ONE),
    ))
}

/// Rust owns layout and uses fixed output accessors for this snapshot preview.
pub fn drawing(width: f32, fonts: &[Font]) -> Result<Canvas, Box<dyn std::error::Error>> {
    let reactive = theoria_reactive::Reactive::new();
    let label = label::Label::builder()
        .fonts(fonts.to_vec())
        .label("Organon — Text from XML\nResize to see words wrap.")
        .width((width - 136.0).max(0.0))
        .font_size(22.0)
        .ink(Color::new(Srgb, [230_u8, 240, 255], Opacity::<u8>::ONE))
        .build(&reactive)?;
    let panel = panel::Panel::builder()
        .width((width - 64.0).max(0.0))
        .fill(Color::new(Srgb, [73_u8, 220, 177], Opacity::<u8>::ONE))
        .mark(label.drawing_fixed().expect("fixed inputs").clone()?)
        .build(&reactive)?;
    Ok(panel.drawing_fixed().expect("fixed inputs").clone()?)
}

pub fn content(
    width: f32,
    height: f32,
    fonts: &[Font],
) -> Result<SurfaceContent, Box<dyn std::error::Error>> {
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
        .ok_or("invalid view dimensions")?;
    Ok(SurfaceContent::new(
        drawing(width, fonts)?,
        view,
        Color::new(Srgb, [10_u8, 16, 26], Opacity::<u8>::ONE),
    ))
}
