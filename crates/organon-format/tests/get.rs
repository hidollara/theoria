use organon_format::ElementDefinition;

#[test]
fn selections_round_trip_and_collect_only_target_dependencies() {
    let source = r##"<element name="Host" version="1">
      <let name="drawing" export="true"><Translate x="0" y="0">
        <get name="drawing"><Panel width="{width}"/></get>
      </Translate></let>
      <let name="mask" export="true"><Draw source="#ffffff"><Fill>
        <get name="path"><ref name="panel"/></get>
      </Fill></Draw></let>
      <let name="text" export="true" inline="true"><get name="text"><ref name="panel"/></get></let>
      <let name="panel"><Panel width="{width}"/></let>
      <input name="width" type="f32"/>
    </element>"##;
    let definition = ElementDefinition::from_xml(source).unwrap();
    assert_eq!(definition.definitions[0].references().unwrap(), ["width"]);
    assert_eq!(definition.definitions[1].references().unwrap(), ["panel"]);
    assert_eq!(definition.definitions[2].references().unwrap(), ["panel"]);
    definition.ordered_bindings().unwrap();
    assert_eq!(
        ElementDefinition::from_xml(&definition.to_xml().unwrap()).unwrap(),
        definition
    );
}

#[test]
fn get_requires_a_fixed_name_and_exactly_one_expression() {
    for expression in [
        r#"<get><Panel/></get>"#,
        r#"<get name="{output}"><Panel/></get>"#,
        r#"<get name="a.b"><Panel/></get>"#,
        r#"<get name="drawing"/>"#,
        r#"<get name="drawing"><Panel/><Panel/></get>"#,
        r#"<get name="drawing" extra="true"><Panel/></get>"#,
        r#"<get name="drawing">text<Panel/></get>"#,
        r#"<get name="drawing"><let name="panel"><Panel/></let></get>"#,
    ] {
        let xml = format!(
            r#"<element name="Host" version="1"><let name="out" export="true">{expression}</let></element>"#
        );
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{expression}");
    }
}

#[test]
fn manually_constructed_selections_obey_the_expression_depth_limit() {
    use organon_format::{Get, Let, ValueExpr};
    let mut value = ValueExpr::Reference("{input}".into());
    for _ in 0..130 {
        value = ValueExpr::Get(Get {
            name: "output".into(),
            target: Box::new(value),
        });
    }
    let definition = ElementDefinition {
        name: "Host".into(),
        inputs: vec![],
        definitions: vec![Let {
            name: "out".into(),
            export: true,
            inline: false,
            value,
        }],
    };
    assert!(
        definition
            .to_xml()
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
}
