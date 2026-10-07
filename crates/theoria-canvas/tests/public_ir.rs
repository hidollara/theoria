use theoria_canvas::{
    Canvas, CanvasNode, Color, ColorField, Coverage, CoverageNode, Opacity, color::Srgb,
    transform::Projective2,
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
