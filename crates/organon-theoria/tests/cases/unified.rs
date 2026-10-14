mod ui {
    organon_theoria::library!(crate::ui);
}
#[test]
fn outputs_have_independent_dependencies() {
    let reactive = theoria_reactive::Reactive::new();
    let size = reactive.create_state(12.0);
    let element = ui::TextParts::builder()
        .label("hello")
        .size(size.value())
        .build(&reactive)
        .unwrap();
    assert_eq!(element.text_fixed().map(String::as_str), Some("hello"));
    assert!(element.style_fixed().is_none());
    size.set(20.0).unwrap();
    assert_eq!(element.text_fixed().map(String::as_str), Some("hello"));
}
#[test]
fn xml_can_supply_the_same_outline_contract_as_rust() {
    let reactive = theoria_reactive::Reactive::new();
    let scene = ui::Scene::builder().width(15.0).build(&reactive).unwrap();
    assert!(scene.drawing_fixed().unwrap().is_ok());
    let bad = ui::Scene::builder().width(-1.0).build(&reactive).unwrap();
    assert!(bad.drawing_fixed().unwrap().is_err());
}
