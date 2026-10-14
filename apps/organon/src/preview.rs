//! Organon's initial preview scene, using its generated drawings.
//!
//! The crate-root library compiles the XML graph; Rust chooses module names and owns layout
//! calculations, state, evaluation timing, and presentation.

use theoria_canvas::{Canvas, CanvasView, Color, Opacity, color::Srgb, geometry::Point};
use theoria_renderer::SurfaceContent;
use theoria_text::font::Font;

mod label {
    pub use crate::generated::Label;
}

mod panel {
    pub use crate::generated::Panel;
}

/// A compiled multi-output element, available to ordinary Rust callers.
pub mod badge {
    pub use crate::generated::Badge;
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

/// A preview evaluates the same native implementations as generated code.
/// Additional compiled libraries can be supplied by extending the registry.
pub struct LibraryPreview {
    pub definition: organon_cargo::LoadedLibrary,
    pub registry: crate::evaluation::Registry,
    pub element: String,
}
impl LibraryPreview {
    pub fn load(
        path: impl AsRef<std::path::Path>,
        mut registry: crate::evaluation::Registry,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let definition = organon_cargo::load(path)?;
        for (local, id) in &definition.loaded.origins {
            let symbol = organon_cargo::identity(&id.file, &id.name)?;
            // Unused or unavailable provider implementations are diagnosed when evaluated.
            let _ = registry.alias(local, &symbol);
        }
        let mut public = definition
            .program
            .elements
            .iter()
            .filter(|e| definition.loaded.public.values().any(|n| n == &e.name));
        let element = public
            .next()
            .ok_or("preview requires one public element")?
            .name
            .clone();
        if public.next().is_some() {
            return Err("preview requires one public element; select it with pub".into());
        }
        Ok(Self {
            definition,
            registry,
            element,
        })
    }
    pub fn content(
        &self,
        inputs: std::collections::BTreeMap<String, crate::evaluation::Value>,
        output: &str,
        width: f32,
        height: f32,
    ) -> Result<SurfaceContent, Box<dyn std::error::Error>> {
        let mut outputs =
            self.registry
                .evaluate(&self.definition.program, &self.element, inputs)?;
        let canvas = outputs
            .remove(output)
            .ok_or_else(|| format!("unknown output {output:?}"))??
            .get::<Canvas>()?;
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
            .ok_or("invalid view dimensions")?;
        Ok(SurfaceContent::new(
            canvas,
            view,
            Color::new(Srgb, [10_u8, 16, 26], Opacity::<u8>::ONE),
        ))
    }
}

// Adapters belong to this host, not to the libraries it previews.
crate::preview_library!("organon.xml");
