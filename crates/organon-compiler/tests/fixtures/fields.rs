mod fields {
    include!("generated/fields.rs");
}

use theoria_canvas::{
    Canvas, CanvasNode, Color, ColorField, Coverage, CoverageNode, GradientStop, LinearGradient,
    Opacity, color::LinearSrgb, geometry::Point, path::Path,
};

#[test]
fn native_fields_and_exported_paths_preserve_theoria_semantics() {
    let color = Color::new(LinearSrgb, [0.2_f32, 0.4, 0.8], Opacity::<f32>::ONE);
    let gradient = LinearGradient::new(
        Point::new(0.0, 0.0),
        Point::new(100.0, 0.0),
        [
            GradientStop::new(0.0, color.clone()).unwrap(),
            GradientStop::new(1.0, color.clone()).unwrap(),
        ],
        LinearSrgb,
    )
    .unwrap();
    let mut inputs = fields::Inputs {
        brush: gradient.clone().into(),
        canvas: Canvas::draw(color, Coverage::all()),
        host_path: Path::polygon([
            Point::new(0.0, 0.0),
            Point::new(10.0, 0.0),
            Point::new(0.0, 10.0),
        ]),
        width: 30.0,
    };
    let output = fields::build(&inputs).unwrap();
    assert_eq!(output.alias, inputs.host_path);
    assert_eq!(
        output.outline.subpaths()[0].segments()[0].end(),
        Point::new(32.0, 3.0)
    );
    let CanvasNode::Draw(draw) = output.filled.node() else {
        panic!()
    };
    let ColorField::LinearGradient(actual) = &draw.source else {
        panic!()
    };
    assert_eq!(actual, &gradient);
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    assert_eq!(path, &output.outline);
    let CanvasNode::Draw(draw) = output.canvas_fill.node() else {
        panic!()
    };
    let ColorField::Canvas(actual) = &draw.source else {
        panic!()
    };
    assert!(std::ptr::eq(actual.node(), inputs.canvas.node()));
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    assert_eq!(
        path.subpaths()[0].segments()[0].end(),
        Point::new(30.0, 0.0)
    );
    let CanvasNode::Draw(draw) = output.unbounded.node() else {
        panic!()
    };
    assert!(matches!(draw.coverage.node(), CoverageNode::All));
    for invalid in [-1.0, f32::NAN, f32::INFINITY] {
        inputs.width = invalid;
        assert!(fields::build(&inputs).is_err());
    }
}
