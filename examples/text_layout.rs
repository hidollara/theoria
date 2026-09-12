use theoria::{
    canvas::{CanvasView, Composite},
    color::Color,
    event_loop::EventLoop,
    font::Font,
    geometry::Point,
    math::Vector2,
    surface::SurfaceContent,
    text::{OpenTypeFeatureTag, Span, Text, TextStyle},
    transform::Affine2,
    window::Window,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = EventLoop::new();
    let event_loop_handle = event_loop.handle();
    let fonts = [
        Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?,
        Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\meiryo.ttc")?)?,
    ];
    let window = Window::new(
        &event_loop_handle,
        "Theoria — Text Layout Sample",
        960,
        640,
        move |context, events| {
            let size = events.surface().size();
            context.create_computed(move |context| {
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let font_size = width.min(height) * 0.052;

                let body_style = TextStyle::new(&fonts, font_size).with_foreground(
                    Color::rgba(0.90, 0.94, 1.0, 0.96).expect("the fixed body color is valid"),
                );
                let emphasized_style = body_style
                    .clone()
                    .with_font_size(font_size * 1.45)
                    .with_feature(OpenTypeFeatureTag::STANDARD_LIGATURES.disabled())
                    .with_foreground(
                        Color::rgba(1.0, 0.68, 0.24, 1.0)
                            .expect("the fixed emphasis color is valid"),
                    );
                let rtl_style = body_style.clone().with_language("he");

                let horizontal = Text::horizontal(body_style.clone())
                    .push("Theoria ")
                    .push(Span::new("ffi").with_style(emphasized_style))
                    .push(" · 日本語 · ")
                    .push(Span::new("עברית").with_style(rtl_style))
                    .push(" — styled spans wrap across their boundaries.\n")
                    .push("Explicit line breaks and fallback fonts participate in layout.")
                    .wrap_width(width * 0.55)
                    .layout()
                    .expect("the horizontal sample text is valid")
                    .to_canvas(
                        Color::rgba(0.90, 0.94, 1.0, 0.96)
                            .expect("the fixed horizontal color is valid"),
                    )
                    .expect("the horizontal glyphs can be realized")
                    .transformed(Affine2::from_translation(Vector2::new(
                        width * 0.08,
                        height * 0.16,
                    )));

                let vertical = Text::vertical(body_style.with_font_size(font_size * 0.9))
                    .push("縦書き Theoria 2026\n日本語とABCを組む")
                    .wrap_height(height * 0.64)
                    .layout()
                    .expect("the vertical sample text is valid")
                    .to_canvas(
                        Color::rgba(0.54, 0.90, 1.0, 0.96)
                            .expect("the fixed vertical color is valid"),
                    )
                    .expect("the vertical glyphs can be realized")
                    .transformed(Affine2::from_translation(Vector2::new(
                        width * 0.76,
                        height * 0.12,
                    )));

                let canvas = horizontal.composite(vertical, Composite::SourceOver);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive surface extent has a non-degenerate view");
                SurfaceContent::new(
                    canvas,
                    view,
                    Color::rgba(0.025, 0.035, 0.055, 1.0).expect("the fixed clear color is valid"),
                )
            })
        },
    )?;
    window.show();
    event_loop.run()
}
