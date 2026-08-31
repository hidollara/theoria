use theoria::{
    Viewport,
    canvas::{CanvasView, Composite},
    color::Color,
    font::Font,
    geometry::Point,
    math::Vector2,
    surface::SurfaceEvent,
    text::{OpenTypeFeatureTag, Span, Text, TextStyle},
    transform::Affine2,
    vulkan::{CanvasRenderer, Presenter},
    window::{Window, WindowEvent},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut window = Window::new("Theoria — Text Layout Sample", 960, 640)?;
    let mut presenter = Presenter::new(window.presentation_target())?;
    let mut canvas_renderer = CanvasRenderer::new(presenter.device());
    let fonts = [
        Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?,
        Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\meiryo.ttc")?)?,
    ];

    while let Some(event) = window.next_event()? {
        match event {
            WindowEvent::Surface(SurfaceEvent::Resized(size)) => presenter.resize(size)?,
            WindowEvent::Surface(SurfaceEvent::RedrawRequested) => {
                let size = window.size();
                let width = size.width as f32;
                let height = size.height as f32;
                let font_size = width.min(height) * 0.052;

                let body_style = TextStyle::new(&fonts, font_size)
                    .with_foreground(Color::rgba(0.90, 0.94, 1.0, 0.96)?);
                let emphasized_style = body_style
                    .clone()
                    .with_font_size(font_size * 1.45)
                    .with_feature(OpenTypeFeatureTag::STANDARD_LIGATURES.disabled())
                    .with_foreground(Color::rgba(1.0, 0.68, 0.24, 1.0)?);
                let rtl_style = body_style.clone().with_language("he");

                let horizontal = Text::horizontal(body_style.clone())
                    .push("Theoria ")
                    .push(Span::new("ffi").with_style(emphasized_style))
                    .push(" · 日本語 · ")
                    .push(Span::new("עברית").with_style(rtl_style))
                    .push(" — styled spans wrap across their boundaries.\n")
                    .push("Explicit line breaks and fallback fonts participate in layout.")
                    .wrap_width(width * 0.55)
                    .layout()?
                    .to_canvas(Color::rgba(0.90, 0.94, 1.0, 0.96)?)?
                    .transformed(Affine2::from_translation(Vector2::new(
                        width * 0.08,
                        height * 0.16,
                    )));

                let vertical = Text::vertical(body_style.with_font_size(font_size * 0.9))
                    .push("縦書き Theoria 2026\n日本語とABCを組む")
                    .wrap_height(height * 0.64)
                    .layout()?
                    .to_canvas(Color::rgba(0.54, 0.90, 1.0, 0.96)?)?
                    .transformed(Affine2::from_translation(Vector2::new(
                        width * 0.76,
                        height * 0.12,
                    )));

                let canvas = horizontal.composite(vertical, Composite::SourceOver);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("the window-sized Canvas view is non-degenerate");
                let viewport = Viewport::new(0.0, 0.0, width, height);
                let clear_color = Color::rgba(0.025, 0.035, 0.055, 1.0)?;

                if let Some(frame) = presenter.begin_frame()? {
                    let target = frame.render_target(clear_color);
                    canvas_renderer.render(target, viewport, &canvas, view)?;
                    frame.present()?;
                }
            }
            WindowEvent::CloseRequested => break,
            WindowEvent::Pointer(_) => {}
            WindowEvent::Scroll(_) => {}
        }
    }

    Ok(())
}
