use std::f32::consts::TAU;

use theoria::{
    canvas::{Canvas, CanvasView, Color, Composite, Composition, Coverage, Opacity},
    color::{LinearSrgb, Srgb},
    event::OwningSink,
    geometry::Point,
    input::pointer::{PointerButton, PointerEvent, PointerPhase},
    path::{LineJoin, Path, StrokeStyle},
    surface::SurfaceContent,
    transform::Affine2,
    windows::{self, Window},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let star = star_path(150.0, 65.0);
    let window = Window::new(
        &event_loop_handle,
        "Theoria — Request-driven Animation",
        960,
        640,
        move |context, window| {
            let pointer = context.create_state(None::<PointerVisual>);
            let pointer_value = pointer.value();
            let pointer_events = context.receive(move |event: PointerEvent, context| {
                let visual = if matches!(event.phase, PointerPhase::Leave | PointerPhase::Cancel) {
                    None
                } else {
                    Some(PointerVisual {
                        position: event.position,
                        pressed: event.buttons.contains(PointerButton::Primary),
                    })
                };
                context.set(&pointer, visual);
            });
            assert!(context.connect(
                window.pointer().clone(),
                OwningSink::new(pointer_events.clone())
            ));

            let clock = window.frame_clock();
            let frame = clock.frame();
            let next_frame = clock.clone();
            let continuous_frame = frame.clone();
            let continuous = context.create_effect(move |context| {
                let _elapsed = context.read(&continuous_frame).elapsed();
                let _ = next_frame.request_frame();
            });
            context.sync(&continuous);

            let size = window.surface().size();
            context.create_computed(move |context| {
                let elapsed = context.read(&frame).elapsed().as_secs_f32();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let pointer = *context.read(&pointer_value);
                let center = pointer.map_or(Point::new(width * 0.5, height * 0.5), |pointer| {
                    pointer.position
                });
                let pressed_scale = if pointer.is_some_and(|pointer| pointer.pressed) {
                    0.82
                } else {
                    1.0
                };
                let pulse = pressed_scale * (0.88 + 0.12 * (elapsed * TAU * 0.6).sin());
                let transform = Affine2::from_scale(pulse, pulse)
                    .expect("finite scale")
                    .then(Affine2::from_rotation(elapsed * 0.8).expect("finite rotation"))
                    .expect("finite composed transform")
                    .then(
                        Affine2::from_translation(center - Point::new(0.0, 0.0))
                            .expect("finite translation"),
                    )
                    .expect("finite composed transform");

                let glow = Canvas::draw(
                    Color::new(
                        Srgb,
                        [0.14_f32, 0.68, 1.0],
                        Opacity::<f32>::new(0.20).expect("the sample opacity is valid"),
                    ),
                    Coverage::stroke(
                        star.clone(),
                        StrokeStyle::new(30.0)
                            .expect("the sample stroke is valid")
                            .with_join(LineJoin::Round),
                    ),
                );
                let fill = Canvas::draw(
                    Color::new(Srgb, [255_u8, 184, 48], Opacity::<u8>::ONE),
                    Coverage::fill(star.clone()),
                );
                let outline = Canvas::draw(
                    Color::new(Srgb, [255_u8, 247, 210], Opacity::<u8>::ONE),
                    Coverage::stroke(
                        star.clone(),
                        StrokeStyle::new(7.0)
                            .expect("the sample stroke is valid")
                            .with_join(LineJoin::Round),
                    ),
                );
                let canvas = glow
                    .composite(fill, Composition::new(Composite::SourceOver, LinearSrgb))
                    .composite(outline, Composition::new(Composite::SourceOver, LinearSrgb))
                    .transformed(transform);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive surface extent has a non-degenerate view");
                SurfaceContent::new(
                    canvas,
                    view,
                    Color::new(Srgb, [7_u8, 10, 18], Opacity::<u8>::ONE),
                )
            })
        },
    )?;
    window.show();
    event_loop.run()
}

#[derive(Clone, Copy)]
struct PointerVisual {
    position: Point,
    pressed: bool,
}

fn star_path(outer_radius: f32, inner_radius: f32) -> Path {
    Path::polygon((0..10).map(|index| {
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let angle = -TAU * 0.25 + index as f32 * TAU / 10.0;
        Point::new(radius * angle.cos(), radius * angle.sin())
    }))
}
