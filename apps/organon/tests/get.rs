use organon_app::{Element, evaluation::Values};

#[test]
fn editing_preserves_selections_and_preview_reports_missing_component_support() {
    for expression in [
        r#"<get name="text"><Label/></get>"#,
        r#"<Translate x="0" y="0"><get name="drawing"><Panel/></get></Translate>"#,
        r##"<Draw source="#ffffff"><Fill><get name="path"><Panel/></get></Fill></Draw>"##,
    ] {
        let xml = format!(
            r#"<element name="Host" version="1"><let name="out" export="true">{expression}</let></element>"#
        );
        let element = Element::from_xml(&xml).unwrap();
        assert_eq!(
            Element::from_xml(&element.to_xml().unwrap()).unwrap(),
            element
        );
        let error = element.evaluate(&Values::new()).unwrap_err();
        assert!(
            error.to_string().contains("preview registration"),
            "{error}"
        );
    }
}

#[test]
fn element_ports_round_trip_and_report_preview_limit_before_missing_inputs() {
    let xml = r#"<element name="Host" version="1"><input name="panel" type="Panel"/><let name="out" export="true"><ref name="panel"/></let></element>"#;
    let element = Element::from_xml(xml).unwrap();
    assert_eq!(
        Element::from_xml(&element.to_xml().unwrap()).unwrap(),
        element
    );
    assert!(
        element
            .evaluate(&Values::new())
            .unwrap_err()
            .to_string()
            .contains("preview registration")
    );
}
