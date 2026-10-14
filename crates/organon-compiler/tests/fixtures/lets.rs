mod lets {
    include!("generated/lets.rs");
}

use theoria_canvas::{
    CanvasNode, Color, ColorField, CoverageNode, Opacity, color::Srgb, geometry::Point,
};

#[test]
fn top_level_aliases_share_values_and_export_their_actual_types() {
    let color = Color::new(Srgb, [10_u8, 20, 30], Opacity::<u8>::ONE);
    let values = lets::build(&lets::Inputs {
        width: 40.0,
        label: "hello".into(),
        color: color.clone(),
    })
    .unwrap();
    assert!(std::ptr::eq(values.copy.node(), values.drawing.node()));
    assert!(std::ptr::eq(values.alias.node(), values.drawing.node()));
    let CanvasNode::Transform { content, .. } = values.moved.node() else {
        panic!()
    };
    assert!(std::ptr::eq(content.node(), values.drawing.node()));
    let CanvasNode::Draw(draw) = values.drawing.node() else {
        panic!()
    };
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    assert_eq!(path, &values.shape);
    assert_eq!(
        path.subpaths()[0].segments()[0].end(),
        Point::new(40.0, 0.0)
    );
    let ColorField::Solid(actual) = &draw.source else {
        panic!()
    };
    assert_eq!(actual, &color);
    assert_eq!(values.ink, color);
    assert_eq!(values.extent, 40.0);
    assert_eq!(values.caption, "hello");
}
