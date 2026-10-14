use organon_format::{Definition, DefinitionFile, InputType, ValueExpr};

#[test]
fn named_types_nested_get_and_template_paths_round_trip_without_loading_dependencies() {
    let xml = r#"<organon version="1">
      <import path="button.xml"/>
      <element name="Host">
        <input name="button" type="Button"/>
        <let name="alias" export="true" inline="true"><ref name="button"/></let>
        <let name="out" export="true"><get name="path"><get name="icon"><ref name="button"/></get></get></let>
        <let name="copy" export="true" inline="true"><Button label="{button.icon.text}"/></let>
      </element>
      <interface name="Native"><input name="item" type="Button"/><output name="item" type="Result&lt;Button&gt;"/></interface>
    </organon>"#;
    let file = DefinitionFile::from_xml(xml).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    let Definition::Element(element) = &file.definitions[0] else {
        unreachable!()
    };
    assert_eq!(element.inputs[0].kind, InputType::Element("Button".into()));
    element.ordered_bindings().unwrap();
    let mut invalid = element.clone();
    invalid.definitions[0].value = ValueExpr::Reference("{button.icon}".into());
    assert!(invalid.to_xml().is_err());
    let mut invalid = element.clone();
    invalid.inputs[0].kind = InputType::Element("f32".into());
    assert!(invalid.to_xml().is_err());
    for name in ["button.path", "{button}", "button..path"] {
        assert!(
            DefinitionFile::from_xml(
                &xml.replace("<ref name=\"button\"/>", &format!("<ref name=\"{name}\"/>"))
            )
            .is_err()
        );
    }
}

#[test]
fn custom_tags_are_instances_but_builtin_names_cannot_be_redefined() {
    for tag in ["Unknown", "Ref", "Use"] {
        let xml = format!(
            r#"<organon version="1"><element name="Host"><let name="value" export="true"><{tag}/></let></element></organon>"#
        );
        let file = DefinitionFile::from_xml(&xml).unwrap();
        let Definition::Element(element) = &file.definitions[0] else {
            unreachable!()
        };
        assert!(matches!(
            element.definitions[0].value,
            ValueExpr::Instance(_)
        ));
    }
    for name in [
        "Path", "Canvas", "Rect", "Fill", "All", "Point", "Text", "String",
    ] {
        let xml = format!(
            r#"<organon version="1"><interface name="{name}"><output name="value" type="f32"/></interface></organon>"#
        );
        assert!(DefinitionFile::from_xml(&xml).is_err());
        let xml = format!(
            r#"<organon version="1"><element name="{name}"><input name="value" type="f32"/><let name="out" export="true"><ref name="value"/></let></element></organon>"#
        );
        assert!(DefinitionFile::from_xml(&xml).is_err());
    }
}
