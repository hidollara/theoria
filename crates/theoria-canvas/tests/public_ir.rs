use theoria_canvas::{
    Canvas, CanvasNode, Color, ColorField, Coverage, CoverageNode, Opacity,
    color::Srgb,
    geometry::{Offset, Point, Radii},
    path::Subpath,
    transform::{Affine2, Projective2},
};

#[test]
fn renderer_can_inspect_the_drawing_without_a_backend() {
    let color = Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE);
    let canvas = Canvas::draw(color.clone(), Coverage::all());
    let CanvasNode::Draw(draw) = canvas.node() else {
        panic!("draw node expected");
    };
    assert!(matches!(draw.coverage.node(), CoverageNode::All));
    let ColorField::Solid(stored) = &draw.source else {
        panic!("solid source expected");
    };
    assert_eq!(stored, &color);

    let transformed = canvas.clone().transformed(Projective2::IDENTITY);
    let CanvasNode::Transform { content, transform } = transformed.node() else {
        panic!("transform node expected");
    };
    assert!(matches!(content.node(), CanvasNode::Draw(_)));
    assert_eq!(*transform, Projective2::IDENTITY);
}

#[test]
fn authored_geometry_needs_no_public_math_types() {
    let transform = Affine2::from_rotation(std::f32::consts::FRAC_PI_2)
        .unwrap()
        .then(Affine2::from_translation(Offset::new(3.0, 4.0)).unwrap())
        .unwrap();
    let transformed = transform.transform_point(Point::new(1.0, 0.0));
    assert!((transformed.x - 3.0).abs() < 1.0e-6);
    assert!((transformed.y - 5.0).abs() < 1.0e-6);
    let _arc = Subpath::new(Point::new(0.0, 0.0)).elliptical_arc_to(
        Radii::new(3.0, 2.0),
        0.0,
        theoria_canvas::path::ArcSize::Minor,
        theoria_canvas::path::ArcSweep::Positive,
        Point::new(4.0, 0.0),
    );
    assert!(Affine2::from_rows([[f32::NAN, 0.0, 0.0], [0.0, 1.0, 0.0]]).is_none());
}
