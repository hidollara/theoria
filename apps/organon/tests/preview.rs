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

#[test]
fn library_preview_uses_the_same_native_geometry_as_compiled_code() {
    use organon_app::evaluation::Value;
    use theoria_canvas::{Color, Opacity, color::Srgb, path::Path};
    let path =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/preview/badge.organon.xml");
    let preview = preview::LibraryPreview::load(path, preview::registry()).unwrap();
    let fill = Color::new(Srgb, [20_u8, 40, 60], Opacity::<u8>::ONE);
    let inputs = std::collections::BTreeMap::from([
        ("width".into(), Value::new(250.0_f32)),
        ("fill".into(), Value::new(fill.clone())),
    ]);
    let outputs = preview
        .registry
        .evaluate(
            &preview.definition.program,
            &preview.element,
            inputs.clone(),
        )
        .unwrap();
    let interpreted = outputs["outline"].as_ref().unwrap().get::<Path>().unwrap();
    let reactive = theoria_reactive::Reactive::new();
    let compiled = preview::badge::Badge::builder()
        .width(250.0)
        .fill(fill)
        .build(&reactive)
        .unwrap();
    assert_eq!(
        interpreted.subpaths(),
        compiled
            .outline_fixed()
            .unwrap()
            .as_ref()
            .unwrap()
            .subpaths()
    );
    preview
        .content(inputs.clone(), "drawing", 400.0, 300.0)
        .unwrap();
    let mut invalid = inputs;
    invalid.insert("width".into(), Value::new(f32::NAN));
    assert!(preview.content(invalid, "drawing", 400.0, 300.0).is_err());
}

#[test]
fn library_preview_accepts_xml_interface_implementations() {
    use organon_app::evaluation::Value;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/organon-theoria/tests/fixtures/unified.xml");
    let loaded = organon_cargo::load(path).unwrap();
    let mut registry = preview::registry();
    for (local, origin) in &loaded.loaded.origins {
        let symbol = organon_cargo::identity(&origin.file, &origin.name).unwrap();
        let _ = registry.alias(local, &symbol);
    }
    let result = registry
        .evaluate(
            &loaded.program,
            "Scene",
            std::collections::BTreeMap::from([("width".into(), Value::new(15.0_f32))]),
        )
        .unwrap();
    assert!(
        result["drawing"]
            .as_ref()
            .unwrap()
            .get::<theoria_canvas::Canvas>()
            .is_ok()
    );
    let result = registry
        .evaluate(
            &loaded.program,
            "Scene",
            std::collections::BTreeMap::from([("width".into(), Value::new(-1.0_f32))]),
        )
        .unwrap();
    assert!(result["drawing"].is_err());
}
