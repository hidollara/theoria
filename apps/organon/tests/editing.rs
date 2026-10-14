use organon_app::element::{Declaration, Definition, DefinitionFile, ValueExpr};

#[test]
fn editing_preserves_imports_order_and_attribute_text() {
    let source = include_str!("../src/preview/label.organon.xml");
    let mut file = DefinitionFile::from_xml(source).unwrap();
    let element = file
        .declarations
        .iter_mut()
        .find_map(|d| match d {
            organon_app::element::FileDeclaration::Definition(Definition::Element(e)) => Some(e),
            _ => None,
        })
        .unwrap();
    element.declarations.reverse();
    let saved = file.to_xml().unwrap();
    assert_eq!(DefinitionFile::from_xml(&saved).unwrap(), file);
    assert!(saved.contains("import"));
}

#[test]
fn arbitrary_library_elements_are_editable_without_drawing_specific_types() {
    let source = r#"<organon version="1">
  <element name="View">
    <let name="value">
      <ThirdParty number="024.00">
        <s t="こんにちは、{name}さん&#10;&lt;&amp;&quot;"/>
      </ThirdParty>
    </let>
    <provide name="value" ref="value"/>
  </element>
</organon>"#;
    let mut file = DefinitionFile::from_xml(source).unwrap();
    let organon_app::element::FileDeclaration::Definition(Definition::Element(element)) =
        &mut file.declarations[0]
    else {
        panic!()
    };
    let Declaration::Let(binding) = &mut element.declarations[0] else {
        panic!()
    };
    let ValueExpr::Instance(instance) = &mut binding.value else {
        panic!()
    };
    instance.inputs.insert("number".into(), "048.00".into());
    let saved = file.to_xml().unwrap();
    assert!(saved.contains("048.00"));
    assert_eq!(DefinitionFile::from_xml(&saved).unwrap(), file);
}
