use std::f32::consts::PI;

use theoria::{
    canvas::{Canvas, CanvasView, Composite, Coverage},
    color::Color,
    font::Font,
    geometry::Point,
    gesture::{Click, ClickArea, ClickGesture, ClickRecognizer},
    input::pointer::PointerButton,
    interaction::HitRegion,
    math::Vector2,
    path::{FillRule, LineCap, LineJoin, Path, StrokeStyle},
    surface::SurfaceContent,
    text::{Text, TextStyle},
    transform::Affine2,
    windows::{self, WindowGroup},
};

const WIDTH: u32 = 640;
const HEIGHT: u32 = 480;
const STAR_CENTER: Point = Point::new(320.0, 205.0);
const HIT_TOLERANCE: f32 = 0.25;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let star = star_path(125.0, 54.0);
    let star_hit_region = HitRegion::fill(star.clone(), FillRule::NonZero);

    let windows = WindowGroup::new(&event_loop_handle, move |group| {
        let count = group.create_state(0_u32);
        let controller_open = group.create_state(true);

        let controller_count = count.clone();
        let controller_value = count.value();
        let controller_lifecycle = controller_open.clone();
        let controller_font = font.clone();
        let controller_star = star.clone();
        group.mount_window(
            "Theoria — Controller",
            WIDTH,
            HEIGHT,
            move |context, window| {
                let close = context.receive(move |_: theoria::window::CloseRequested, context| {
                    context.set(&controller_lifecycle, false);
                });
                assert!(context.connect(window.close_requested().cloned(), close));

                let increment = context.receive(move |click: Click, context| {
                    if click.button == PointerButton::Primary {
                        context.modify(&controller_count, |count| {
                            *count = count.saturating_add(1);
                        });
                    }
                });
                let area = ClickArea::new(increment, star_hit_region).transformed(
                    Affine2::from_translation(Vector2::new(STAR_CENTER.x, STAR_CENTER.y)),
                );
                let route_click = context.receive(move |click: Click, context| {
                    let _ = area.try_forward(&click, HIT_TOLERANCE, context);
                });
                context.mount(ClickGesture::new(
                    ClickRecognizer::new(window.movement_threshold())
                        .expect("the Window provides a valid movement threshold"),
                    window.pointer().clone(),
                    route_click,
                ));

                let canvas = context.create_computed(move |context| {
                    let count = *context.read(&controller_value);
                    let color = match count % 3 {
                        0 => Color::rgb8(255, 188, 54),
                        1 => Color::rgb8(83, 220, 255),
                        _ => Color::rgb8(255, 103, 181),
                    };
                    let fill = Canvas::draw(color, Coverage::fill(controller_star.clone()));
                    let outline = Canvas::draw(
                        Color::rgb8(255, 246, 214),
                        Coverage::stroke(
                            controller_star.clone(),
                            StrokeStyle::new(7.0)
                                .expect("the fixed outline width is valid")
                                .with_cap(LineCap::Round)
                                .with_join(LineJoin::Round),
                        ),
                    );
                    let fonts = [controller_font.clone()];
                    let label = Text::horizontal(TextStyle::new(&fonts, 30.0))
                        .push(format!("Shared count: {count}"))
                        .layout()
                        .expect("the counter text is valid")
                        .to_canvas(Color::rgb8(232, 240, 255))
                        .expect("the counter glyphs can be realized")
                        .transformed(Affine2::from_translation(Vector2::new(210.0, 390.0)));

                    fill.composite(outline, Composite::SourceOver)
                        .transformed(Affine2::from_translation(Vector2::new(
                            STAR_CENTER.x,
                            STAR_CENTER.y,
                        )))
                        .composite(label, Composite::SourceOver)
                });
                window_content(context, window.surface().size(), canvas)
            },
        )?;

        let preview_value = count.value();
        let preview_controller_open = controller_open.value();
        group.mount_window(
            "Theoria — Shared Preview",
            WIDTH,
            HEIGHT,
            move |context, window| {
                let canvas = context.create_computed(move |context| {
                    let count = *context.read(&preview_value);
                    let controller_open = *context.read(&preview_controller_open);
                    let fonts = [font.clone()];
                    let heading = Text::horizontal(TextStyle::new(&fonts, 26.0))
                        .push("The other Window changed this value")
                        .layout()
                        .expect("the heading text is valid")
                        .to_canvas(Color::rgb8(145, 168, 205))
                        .expect("the heading glyphs can be realized")
                        .transformed(Affine2::from_translation(Vector2::new(90.0, 145.0)));
                    let value = Text::horizontal(TextStyle::new(&fonts, 96.0))
                        .push(count.to_string())
                        .layout()
                        .expect("the shared value text is valid")
                        .to_canvas(Color::rgb8(90, 220, 255))
                        .expect("the shared value glyphs can be realized")
                        .transformed(Affine2::from_translation(Vector2::new(285.0, 300.0)));
                    let status = if controller_open {
                        "Controller Window is open"
                    } else {
                        "Controller Window closed; this Window remains"
                    };
                    let status = Text::horizontal(TextStyle::new(&fonts, 22.0))
                        .push(status)
                        .layout()
                        .expect("the lifecycle status text is valid")
                        .to_canvas(if controller_open {
                            Color::rgb8(116, 221, 147)
                        } else {
                            Color::rgb8(255, 188, 96)
                        })
                        .expect("the lifecycle status glyphs can be realized")
                        .transformed(Affine2::from_translation(Vector2::new(110.0, 380.0)));
                    heading
                        .composite(value, Composite::SourceOver)
                        .composite(status, Composite::SourceOver)
                });
                window_content(context, window.surface().size(), canvas)
            },
        )?;
        Ok(())
    })?;
    windows.show();
    event_loop.run()
}

fn window_content(
    context: &mut theoria::component::MountContext<'_>,
    size: theoria::reactive::Value<theoria::surface::SurfaceSize>,
    canvas: theoria::reactive::Computed<Canvas>,
) -> theoria::reactive::Computed<SurfaceContent> {
    context.create_computed(move |context| {
        let canvas = context.read(&canvas).cloned();
        let size = *context.read(&size);
        let width = size.width.max(1) as f32;
        let height = size.height.max(1) as f32;
        let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
            .expect("a positive surface extent has a non-degenerate view");
        SurfaceContent::new(canvas, view, Color::rgb8(6, 9, 14))
    })
}

fn star_path(outer_radius: f32, inner_radius: f32) -> Path {
    Path::polygon((0..10).map(|index| {
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let angle = -PI * 0.5 + index as f32 * PI / 5.0;
        Point::new(radius * angle.cos(), radius * angle.sin())
    }))
}
