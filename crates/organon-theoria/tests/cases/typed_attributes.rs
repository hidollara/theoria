mod elements {
    organon_theoria::library!(crate::elements);
}

#[test]
fn bare_element_references_and_literal_strings_have_distinct_dependencies() {
    let reactive = theoria_reactive::Reactive::new();
    let host = elements::Host::builder().build(&reactive).unwrap();
    let forward = host.forward_fixed().unwrap();
    let token = forward.item_fixed().unwrap();
    assert_eq!(token.text_fixed().unwrap(), "token");
}
