use theoria::{
    canvas::{
        AcrossPathGradient, AlongPathGradient, BlendMode, Canvas, CanvasView, ColorRamp, Composite,
        ConicGradient, Coverage, GradientPoint, GradientStop, ImageExtend, ImagePattern,
        LinearGradient, MeshGradient, RadialGradient,
    },
    color::Color,
    geometry::Point,
    image::{Image, ImageError},
    math::Vector2,
    path::{
        ArcSize, ArcSweep, DashPattern, FillRule, LineCap, LineJoin, Path, StrokeStyle, Subpath,
    },
    transform::Affine2,
    window::{Window, WindowContent},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let source_image = make_sample_image()?;
    let image_width = source_image.width();
    let image_height = source_image.height();
    let image_canvas = Canvas::draw(source_image, Coverage::all());
    let image = Image::from_canvas(
        image_width,
        image_height,
        image_canvas,
        CanvasView::axis_aligned(
            Point::new(0.0, 0.0),
            Point::new(image_width as f32, image_height as f32),
        )
        .expect("the sample image view is non-degenerate"),
    )?;

    let window = Window::new(
        "Theoria — Canvas Sample",
        960,
        640,
        move |context, events| {
            let size = events.size();
            context.create_computed(move |context| {
                let size = *context.read(&size);
                let image = &image;
                (|| -> Result<WindowContent, Box<dyn std::error::Error>> {
                    let width = size.width.max(1) as f32;
                    let height = size.height.max(1) as f32;
                    let coverage_path = Path::new()
                        .add_subpath(
                            Subpath::new(Point::new(width * 0.18, height * 0.18))
                                .line_to(Point::new(width * 0.82, height * 0.30))
                                .line_to(Point::new(width * 0.48, height * 0.74))
                                .close(),
                        )
                        .add_subpath(
                            Subpath::new(Point::new(width * 0.36, height * 0.34))
                                .line_to(Point::new(width * 0.65, height * 0.39))
                                .line_to(Point::new(width * 0.49, height * 0.59))
                                .close(),
                        );
                    let coverage_canvas = Canvas::draw(
                        Color::rgb8(255, 255, 255),
                        Coverage::fill_with_rule(coverage_path, FillRule::EvenOdd),
                    );
                    let image_layer = Canvas::draw(
                        ImagePattern::placed(
                            (*image).clone(),
                            Affine2::from_scale(Vector2::new(2.0, 2.0)).then(
                                Affine2::from_translation(Vector2::new(
                                    width * 0.24,
                                    height * 0.22,
                                )),
                            ),
                        )
                        .unwrap()
                        .with_extend(ImageExtend::Repeat),
                        Coverage::fill(Path::polygon([
                            Point::new(width * 0.24, height * 0.22),
                            Point::new(width * 0.74, height * 0.22),
                            Point::new(width * 0.74, height * 0.66),
                            Point::new(width * 0.24, height * 0.66),
                        ])),
                    );
                    let linear_layer = Canvas::draw(
                        LinearGradient::new(
                            Point::new(width * 0.10, height * 0.20),
                            Point::new(width * 0.70, height * 0.75),
                            [
                                GradientStop::new(0.0, Color::rgba(0.05, 0.85, 1.0, 0.78)?)?,
                                GradientStop::new(0.45, Color::rgba(0.35, 0.38, 1.0, 0.72)?)?,
                                GradientStop::new(1.0, Color::rgba(1.0, 0.18, 0.72, 0.68)?)?,
                            ],
                        )?,
                        Coverage::fill(Path::polygon([
                            Point::new(width * 0.38, height * 0.14),
                            Point::new(width * 0.72, height * 0.80),
                            Point::new(width * 0.08, height * 0.78),
                        ])),
                    );
                    let masked_content = Canvas::draw(
                        image_layer.composite(linear_layer, Composite::SourceOver),
                        Coverage::from_alpha(coverage_canvas),
                    );
                    let mesh_layer = Canvas::draw(
                        MeshGradient::from_triangle([
                            GradientPoint::new(
                                Point::new(width * 0.62, height * 0.16),
                                Color::rgba(1.0, 0.12, 0.42, 0.78)?,
                            ),
                            GradientPoint::new(
                                Point::new(width * 0.92, height * 0.72),
                                Color::rgba(0.15, 1.0, 0.58, 0.72)?,
                            ),
                            GradientPoint::new(
                                Point::new(width * 0.50, height * 0.88),
                                Color::rgba(0.20, 0.32, 1.0, 0.72)?,
                            ),
                        ])?,
                        Coverage::fill(Path::polygon([
                            Point::new(width * 0.62, height * 0.16),
                            Point::new(width * 0.92, height * 0.72),
                            Point::new(width * 0.50, height * 0.88),
                        ])),
                    );
                    let radial_layer = Canvas::draw(
                        RadialGradient::new(
                            Point::new(width * 0.22, height * 0.23),
                            height * 0.13,
                            [
                                GradientStop::new(0.0, Color::rgba(1.0, 1.0, 1.0, 0.95)?)?,
                                GradientStop::new(0.45, Color::rgba(0.1, 0.9, 1.0, 0.78)?)?,
                                GradientStop::new(1.0, Color::rgba(0.08, 0.2, 0.8, 0.15)?)?,
                            ],
                        )?,
                        Coverage::fill(ellipse_path(
                            Point::new(width * 0.22, height * 0.23),
                            Vector2::new(height * 0.13, height * 0.13),
                        )),
                    );
                    let conic_layer = Canvas::draw(
                        ConicGradient::new(
                            Point::new(width * 0.76, height * 0.70),
                            -std::f32::consts::FRAC_PI_2,
                            [
                                GradientStop::new(0.0, Color::rgba(1.0, 0.15, 0.25, 0.82)?)?,
                                GradientStop::new(0.25, Color::rgba(1.0, 0.85, 0.12, 0.82)?)?,
                                GradientStop::new(0.5, Color::rgba(0.1, 1.0, 0.55, 0.82)?)?,
                                GradientStop::new(0.75, Color::rgba(0.2, 0.4, 1.0, 0.82)?)?,
                                GradientStop::new(1.0, Color::rgba(1.0, 0.15, 0.25, 0.82)?)?,
                            ],
                        )?,
                        Coverage::fill(ellipse_path(
                            Point::new(width * 0.76, height * 0.70),
                            Vector2::new(height * 0.14, height * 0.14),
                        )),
                    );
                    let stroke_path = Path::new().add_subpath(
                        Subpath::new(Point::new(width * 0.18, height * 0.58))
                            .quadratic_to(
                                Point::new(width * 0.28, height * 0.28),
                                Point::new(width * 0.43, height * 0.50),
                            )
                            .cubic_to(
                                Point::new(width * 0.54, height * 0.76),
                                Point::new(width * 0.69, height * 0.20),
                                Point::new(width * 0.80, height * 0.40),
                            )
                            .elliptical_arc_to(
                                Vector2::new(width * 0.10, height * 0.12),
                                0.2,
                                ArcSize::Minor,
                                ArcSweep::Positive,
                                Point::new(width * 0.84, height * 0.62),
                            ),
                    );
                    let ribbon_width = width * 0.034;
                    let across_layer = Canvas::draw(
                        AcrossPathGradient::new(
                            stroke_path.clone(),
                            -ribbon_width * 0.5,
                            ribbon_width * 0.5,
                            ColorRamp::new([
                                GradientStop::new(0.0, Color::rgba(0.35, 0.10, 1.0, 0.48)?)?,
                                GradientStop::new(0.5, Color::rgba(0.15, 0.75, 1.0, 0.28)?)?,
                                GradientStop::new(1.0, Color::rgba(0.10, 1.0, 0.55, 0.48)?)?,
                            ])?,
                        )?,
                        Coverage::stroke(
                            stroke_path.clone(),
                            StrokeStyle::new(ribbon_width)?
                                .with_cap(LineCap::Round)
                                .with_join(LineJoin::Round),
                        ),
                    );
                    let stroke_layer = Canvas::draw(
                        AlongPathGradient::new(
                            stroke_path.clone(),
                            ColorRamp::new([
                                GradientStop::new(0.0, Color::rgba(1.0, 0.25, 0.72, 0.95)?)?,
                                GradientStop::new(0.45, Color::rgba(1.0, 0.90, 0.12, 0.95)?)?,
                                GradientStop::new(1.0, Color::rgba(0.10, 0.90, 1.0, 0.95)?)?,
                            ])?,
                        ),
                        Coverage::stroke(
                            stroke_path,
                            StrokeStyle::new(width * 0.012)?
                                .with_cap(LineCap::Round)
                                .with_join(LineJoin::Round)
                                .with_dash_pattern(DashPattern::new(
                                    [width * 0.035, width * 0.018],
                                    0.0,
                                )?),
                        ),
                    );
                    let content = masked_content
                        .blend_composite(mesh_layer, BlendMode::Multiply, Composite::SourceOver)
                        .composite(radial_layer, Composite::Plus)
                        .composite(conic_layer, Composite::SourceOver)
                        .composite(across_layer, Composite::DestinationOver)
                        .composite(stroke_layer, Composite::SourceOver);
                    // Deliberately use a non-parallelogram Canvas region here so
                    // the sample exercises the projective path all the way to the
                    // GPU's homogeneous divide.
                    let view = CanvasView::projective(
                        Point::new(width * 0.12, height * 0.08),
                        Point::new(width * 0.88, height * 0.18),
                        Point::new(width * 0.98, height * 0.92),
                        Point::new(width * 0.02, height * 0.72),
                    )
                    .expect("the sample view quadrilateral is non-degenerate");
                    let clear_color = Color::rgba(0.025, 0.035, 0.055, 1.0)?;
                    Ok(WindowContent::new(content, view, clear_color))
                })()
                .expect("the Canvas sample uses valid fixed drawing parameters")
            })
        },
    )?;
    window.run()
}

fn ellipse_path(center: Point, radii: Vector2) -> Path {
    let left = Point::new(center.x - radii.x, center.y);
    let right = Point::new(center.x + radii.x, center.y);
    Path::new().add_subpath(
        Subpath::new(left)
            .elliptical_arc_to(radii, 0.0, ArcSize::Minor, ArcSweep::Positive, right)
            .elliptical_arc_to(radii, 0.0, ArcSize::Minor, ArcSweep::Positive, left)
            .close(),
    )
}

fn make_sample_image() -> Result<Image, ImageError> {
    const SIZE: u32 = 64;
    let mut pixels = Vec::with_capacity((SIZE * SIZE * 4) as usize);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let light = ((x / 8) + (y / 8)) % 2 == 0;
            let rgba = if light {
                [255, 214, 92, 220]
            } else {
                [48, 30, 82, 220]
            };
            pixels.extend_from_slice(&rgba);
        }
    }
    Image::from_rgba8(SIZE, SIZE, pixels)
}
