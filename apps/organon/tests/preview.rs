use organon_app::preview;
use theoria_canvas::{CanvasNode, CanvasView, CoverageNode, geometry::Point};
use theoria_text::font::Font;

#[test]
fn preview_builds_with_test_fonts_at_normal_and_small_window_sizes() {
    let fonts = [Font::from_bytes(font_test_data::AHEM).unwrap()];
    for width in [480.0, 200.0, 64.0, 1.0] {
        let content = preview::content(width, 256.0, &fonts).unwrap();
        assert_eq!(
            content.view(),
            CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, 256.0)).unwrap()
        );
        let CanvasNode::Transform { content, .. } = content.canvas().node() else {
            panic!("preview must place its panel")
        };
        let CanvasNode::Composite { backdrop, .. } = content.node() else {
            panic!("panel must composite its content over a background")
        };
        let CanvasNode::Draw(draw) = backdrop.node() else {
            panic!("panel background must be a drawing")
        };
        let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
            panic!("panel background must have a filled path")
        };
        assert_eq!(
            path.subpaths()[0].segments()[0].end().x,
            (width - 64.0).max(0.0)
        );
    }
}

#[test]
fn preview_reports_missing_fonts_and_invalid_views() {
    assert!(preview::content(480.0, 256.0, &[]).is_err());
    let fonts = [Font::from_bytes(font_test_data::AHEM).unwrap()];
    for width in [f32::NAN, f32::INFINITY] {
        assert!(preview::content(width, 256.0, &fonts).is_err());
    }
}
