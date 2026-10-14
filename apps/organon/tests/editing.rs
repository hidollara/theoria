use organon_app::{CanvasExpr, Element, element::ValueExpr};
use organon_format::ElementDefinition;

#[test]
fn storage_and_editable_trees_round_trip_without_losing_data() {
    for source in [
        include_str!("../src/preview/panel.organon.xml"),
        include_str!("../src/preview/label.organon.xml"),
    ] {
        let stored = ElementDefinition::from_xml(source).unwrap();
        let editable = Element::from(stored.clone());
        assert_eq!(ElementDefinition::from(editable.clone()), stored);
        assert_eq!(
            Element::from_xml(&editable.to_xml().unwrap()).unwrap(),
            editable
        );
    }
}

#[test]
fn edited_text_preserves_authored_attributes_when_saved_and_reopened() {
    let mut drawing = Element::from_xml(include_str!("../src/preview/label.organon.xml")).unwrap();
    let ValueExpr::Canvas(CanvasExpr::Text {
        content,
        style,
        wrap_width,
    }) = &mut drawing.definitions[0].value
    else {
        panic!()
    };
    *content = vec![organon_format::TextContent::string(
        "こんにちは、{name}さん\n<&\"",
    )];
    style.font_size = "024.00".into();
    *wrap_width = None;
    let saved = drawing.to_xml().unwrap();
    assert_eq!(Element::from_xml(&saved).unwrap(), drawing);
    let stored = ElementDefinition::from_xml(&saved).unwrap();
    let organon_format::ValueExpr::Canvas(organon_format::CanvasExpr::Text {
        content,
        style,
        wrap_width,
    }) = &stored.definitions[0].value
    else {
        panic!()
    };
    assert_eq!(
        content,
        &vec![organon_format::TextContent::string(
            "こんにちは、{name}さん\n<&\""
        )]
    );
    assert_eq!(style.font_size, "024.00");
    assert_eq!(*wrap_width, None);
}
