use std::f32::consts::TAU;

use theoria::{
    canvas::{Canvas, CanvasView, Composite, Coverage},
    color::Color,
    geometry::Point,
    math::{Matrix2, Vector2},
    path::{LineJoin, Path, StrokeStyle},
    transform::Affine2,
    window::{Window, WindowContent},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let star = star_path(150.0, 65.0);
    let window = Window::new(
        "Theoria — Request-driven Animation",
        960,
        640,
        move |context, events| {
            let clock = events.frame_clock();
            let frame = clock.frame();
            let next_frame = clock.clone();
            let continuous_frame = frame.clone();
            let continuous = context.create_effect(move |context| {
                let _elapsed = context.read(&continuous_frame).elapsed();
                let _ = next_frame.request_frame();
            });
            context.sync(&continuous);

            let size = events.size();
            context.create_computed(move |context| {
                let elapsed = context.read(&frame).elapsed().as_secs_f32();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let center = Vector2::new(width * 0.5, height * 0.5);
                let pulse = 0.88 + 0.12 * (elapsed * TAU * 0.6).sin();
                let transform = Affine2::from_scale(Vector2::new(pulse, pulse))
                    .then(Affine2::from_matrix(Matrix2::from_angle(elapsed * 0.8)))
                    .then(Affine2::from_translation(center));

                let glow = Canvas::draw(
                    Color::rgba(0.14, 0.68, 1.0, 0.20).expect("the sample color is valid"),
                    Coverage::stroke(
                        star.clone(),
                        StrokeStyle::new(30.0)
                            .expect("the sample stroke is valid")
                            .with_join(LineJoin::Round),
                    ),
                );
                let fill = Canvas::draw(Color::rgb8(255, 184, 48), Coverage::fill(star.clone()));
                let outline = Canvas::draw(
                    Color::rgb8(255, 247, 210),
                    Coverage::stroke(
                        star.clone(),
                        StrokeStyle::new(7.0)
                            .expect("the sample stroke is valid")
                            .with_join(LineJoin::Round),
                    ),
                );
                let canvas = glow
                    .composite(fill, Composite::SourceOver)
                    .composite(outline, Composite::SourceOver)
                    .transformed(transform);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive surface extent has a non-degenerate view");
                WindowContent::new(canvas, view, Color::rgb8(7, 10, 18))
            })
        },
    )?;
    window.run()
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
