use organon_format::{ElementDefinition, ValueExpr};

const SOURCE: &str = r##"<element name="Example" version="1">
  <input name="width" type="f32"/>
  <let name="drawing" export="true"><Draw source="#ffffff"><Fill><ref name="shape"/></Fill></Draw></let>
  <let name="outline" export="true"><ref name="shape"/></let>
  <let name="shape"><Polygon>
    <Point x="0" y="0"/><Point x="{width}" y="0"/><Point x="0" y="20"/>
  </Polygon></let>
</element>"##;

#[test]
fn element_name_is_required_and_survives_round_trip() {
    let mut definition = ElementDefinition::from_xml(SOURCE).unwrap();
    assert_eq!(definition.name, "Example");
    definition.name = "Greeting".into();
    let encoded = definition.to_xml().unwrap();
    assert_eq!(ElementDefinition::from_xml(&encoded).unwrap(), definition);
    assert!(ElementDefinition::from_xml(&SOURCE.replace("name=\"Example\" ", "")).is_err());
    for name in ["", "a-b", "ui.Greeting", "{name}"] {
        assert!(
            ElementDefinition::from_xml(
                &SOURCE.replace("name=\"Example\"", &format!("name=\"{name}\""))
            )
            .is_err()
        );
        definition.name = name.into();
        assert!(definition.to_xml().is_err());
    }
}

#[test]
fn references_are_serialized_as_child_expressions() {
    let definition = ElementDefinition::from_xml(SOURCE).unwrap();
    let saved = definition.to_xml().unwrap();
    assert!(!saved.contains(" path="));
    assert!(!saved.contains(" value="));
    assert_eq!(saved.matches("<ref name=\"shape\"/>").count(), 2);
    assert_eq!(ElementDefinition::from_xml(&saved).unwrap(), definition);
}

#[test]
fn references_require_children_instead_of_attribute_shorthands() {
    for body in [
        r#"<let name="a" export="true" value="{host}"/>"#,
        r#"<let name="a" export="true" value="{host}"><ref name="host"/></let>"#,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill path="{host}"/></Draw></let>"##,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill path="{host}"><ref name="host"/></Fill></Draw></let>"##,
        r#"<let name="a" export="true"/>"#,
        r#"<let name="a" export="true"><ref name="host"/><ref name="host"/></let>"#,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill/></Draw></let>"##,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill><ref name="host"/><ref name="host"/></Fill></Draw></let>"##,
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><input name="host" type="Path"/>{body}</element>"#
        );
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{xml}");
    }
}

#[test]
fn reference_syntax_is_strict_and_language_tags_are_lowercase() {
    for reference in [
        r#"<ref/>"#,
        r#"<ref name=""/>"#,
        r#"<ref name="{source}"/>"#,
        r#"<ref name="source..path"/>"#,
        r#"<ref name="source.path"/>"#,
        r#"<ref name="source" extra="true"/>"#,
        r#"<ref name="source"><Rect x="0" y="0" width="1" height="1"/></ref>"#,
        r#"<ref name="source">text</ref>"#,
        r#"<ref xmlns="urn:other" name="source"/>"#,
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><let name="alias" export="true">{reference}</let></element>"#
        );
        let error = ElementDefinition::from_xml(&xml).unwrap_err().to_string();
        assert!(error.starts_with("1:"), "{error}");
    }
    for (lower, upper) in [("element", "Element"), ("input", "Input"), ("let", "Let")] {
        let xml = SOURCE
            .replace(&format!("<{lower} "), &format!("<{upper} "))
            .replace(&format!("</{lower}>"), &format!("</{upper}>"));
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{xml}");
    }
}

#[test]
fn named_definitions_round_trip_and_do_not_depend_on_declaration_order() {
    let definition = ElementDefinition::from_xml(SOURCE).unwrap();
    assert!(matches!(
        definition.definitions[0].value,
        ValueExpr::Canvas(_)
    ));
    let saved = definition.to_xml().unwrap();
    assert_eq!(ElementDefinition::from_xml(&saved).unwrap(), definition);
}

#[test]
fn shape_and_output_policies_reject_xml_that_is_not_an_organon_definition() {
    for invalid in [
        SOURCE.replace("version=\"1\"", "version=\"2\""),
        SOURCE.replace("<Polygon>", "<Polygon closed=\"false\">"),
        SOURCE.replace("<Point x=\"0\" y=\"20\"/>", ""),
        SOURCE.replace("name=\"shape\"", "name=\"width\""),
        SOURCE.replace("name=\"outline\"", "name=\"drawing\""),
        SOURCE.replace("<ref name=\"shape\"", "<ref name=\"prefix shape\""),
        SOURCE.replace("name=\"drawing\"", "name=\"drawing\" value=\"{shape}\""),
        SOURCE.replace("<element ", "<element xmlns=\"urn:unknown\" "),
        SOURCE.replace(
            "<Point x=\"0\" y=\"20\"/>",
            "<Point x=\"0\" y=\"20\">unexpected</Point>",
        ),
    ] {
        assert!(ElementDefinition::from_xml(&invalid).is_err(), "{invalid}");
    }
    assert!(ElementDefinition::from_xml("<element name=\"Example\" version=\"1\"/>").is_err());
}

#[test]
fn path_only_is_valid_and_does_not_imply_an_empty_canvas() {
    let source = r#"<element name="Example" version="1"><input name="path" type="Path"/><let name="shape" export="true"><ref name="path"/></let></element>"#;
    let definition = ElementDefinition::from_xml(source).unwrap();
    assert_eq!(definition.definitions.len(), 1);
    assert!(matches!(
        definition.definitions[0].value,
        ValueExpr::Reference(_)
    ));
}

#[test]
fn named_paths_use_the_same_path_expressions_as_fill() {
    let source = r#"<element name="Example" version="1">
      <input name="host" type="Path"/>
      <let name="alias"><ref name="host"/></let>
      <let name="rect"><Rect x="0" y="0" width="001.0" height="2"/></let>
      <let name="shape" export="true"><ref name="rect"/></let>
    </element>"#;
    let data = ElementDefinition::from_xml(source).unwrap();
    assert_eq!(
        ElementDefinition::from_xml(&data.to_xml().unwrap()).unwrap(),
        data
    );
    let invalid = source.replace(
        r#"<let name="alias"><ref name="host"/></let>"#,
        r#"<let name="alias" value="{host}"><Rect x="0" y="0" width="1" height="1"/></let>"#,
    );
    assert!(ElementDefinition::from_xml(&invalid).is_err());
}

#[test]
fn let_is_rejected_everywhere_except_directly_under_element() {
    let binding = r#"<let name="inner"><Rect x="0" y="0" width="1" height="2"/></let>"#;
    for expression in [
        binding.to_owned(),
        format!(r##"<Draw source="#ffffff">{binding}</Draw>"##),
        format!(r##"<Draw source="#ffffff"><Fill>{binding}</Fill></Draw>"##),
        format!(r#"<Translate x="0" y="0">{binding}</Translate>"#),
        format!(
            r##"<SourceOver space="Srgb">{binding}<Draw source="#ffffff"><All/></Draw></SourceOver>"##
        ),
    ] {
        let source = format!(
            r#"<element name="Example" version="1"><let name="drawing" export="true">{expression}</let></element>"#
        );
        assert!(
            ElementDefinition::from_xml(&source)
                .unwrap_err()
                .to_string()
                .contains("let is only allowed directly under element"),
            "{source}"
        );
    }
}

#[test]
fn aliases_and_export_policies_round_trip() {
    let definition = ElementDefinition::from_xml(SOURCE).unwrap();
    assert_eq!(
        ElementDefinition::from_xml(&definition.to_xml().unwrap()).unwrap(),
        definition
    );
    let ordered = definition.ordered_bindings().unwrap();
    assert_eq!(ordered[0].name, "shape");
    for invalid in [
        SOURCE.replace("export=\"true\"", "export=\"{flag}\""),
        SOURCE.replace("export=\"true\"", "export=\"yes\""),
        SOURCE.replace("name=\"outline\"", "name=\"shape\""),
    ] {
        assert!(ElementDefinition::from_xml(&invalid).is_err(), "{invalid}");
    }
}

#[test]
fn inline_is_an_independent_fixed_boolean_policy() {
    for export in [false, true] {
        let source = SOURCE.replace(
            "<let name=\"shape\"",
            &format!("<let name=\"shape\" inline=\"true\" export=\"{export}\""),
        );
        let definition = ElementDefinition::from_xml(&source).unwrap();
        assert!(definition.definitions[2].inline);
        assert_eq!(definition.definitions[2].export, export);
        assert_eq!(
            ElementDefinition::from_xml(&definition.to_xml().unwrap()).unwrap(),
            definition
        );
        for invalid in ["{flag}", "yes", "1", ""] {
            assert!(
                ElementDefinition::from_xml(
                    &source.replace("inline=\"true\"", &format!("inline=\"{invalid}\""))
                )
                .is_err()
            );
        }
    }
    assert!(
        ElementDefinition::from_xml(SOURCE)
            .unwrap()
            .definitions
            .iter()
            .all(|d| !d.inline)
    );
}

#[test]
fn binding_dependencies_reject_unknown_names_and_cycles() {
    for (body, diagnostic) in [
        (
            r#"<let name="a" export="true"><ref name="missing"/></let>"#,
            "missing",
        ),
        (
            r#"<let name="a" export="true"><ref name="a"/></let>"#,
            "cyclic",
        ),
        (
            r#"<let name="a" export="true"><ref name="b"/></let><let name="b"><ref name="a"/></let>"#,
            "cyclic",
        ),
    ] {
        let source = format!("<element name=\"Example\" version=\"1\">{body}</element>");
        let definition = ElementDefinition::from_xml(&source).unwrap();
        assert!(
            definition
                .ordered_bindings()
                .unwrap_err()
                .to_string()
                .contains(diagnostic)
        );
    }
}

#[test]
fn excessive_xml_depth_is_rejected_before_parsing_definitions() {
    let mut source = String::from("<element name=\"Example\" version=\"1\">");
    for i in 0..140 {
        source.push_str(&format!("<let name=\"n{i}\" export=\"true\">"));
    }
    source.push_str("<Rect x=\"0\" y=\"0\" width=\"1\" height=\"1\"/>");
    source.push_str(&"</let>".repeat(140));
    source.push_str("</element>");
    assert!(
        ElementDefinition::from_xml(&source)
            .unwrap_err()
            .to_string()
            .contains("nesting")
    );
}

#[test]
fn a_long_dependency_chain_is_flat_and_preserves_declaration_order_when_saved() {
    let mut source = String::from("<element name=\"Example\" version=\"1\">");
    for i in 0..128 {
        source.push_str(&format!(
            "<let name=\"n{i}\" export=\"true\"><ref name=\"n{}\"/></let>",
            i + 1
        ));
    }
    source.push_str(r#"<let name="n128"><Rect x="0" y="0" width="1" height="1"/></let></element>"#);
    let definition = ElementDefinition::from_xml(&source).unwrap();
    let ordered = definition.ordered_bindings().unwrap();
    assert_eq!(ordered.len(), 129);
    assert_eq!(ordered[0].name, "n128");
    assert_eq!(ordered[128].name, "n0");
    assert_eq!(
        ElementDefinition::from_xml(&definition.to_xml().unwrap()).unwrap(),
        definition
    );
}

#[test]
fn depth_preflight_ignores_markup_inside_comments_and_quoted_attributes() {
    let noise = "<let>".repeat(200);
    let source = format!(
        r##"<?test {noise}?><element name="Example" version="1"><!-- {noise} --><![CDATA[ ]]><let name="a" export="true"><Draw source="#ffffff"><All/></Draw></let></element>"##
    );
    assert!(ElementDefinition::from_xml(&source).is_ok());
    let source = r#"<element name="Example" version="1"><input name="x" type="String"/><let name="a" export="true"><ref name="x"/></let></element>"#;
    assert!(
        ElementDefinition::from_xml(
            &source.replace("<ref name=\"x\"", "<ref name=\"invalid > text\"")
        )
        .is_err()
    );
}

#[test]
fn element_is_the_only_document_root() {
    for source in [
        r##"<Canvas version="1"><Draw source="#ffffff"><All/></Draw></Canvas>"##,
        r##"<element name="Example" version="1"><Draw source="#ffffff"><All/></Draw></element>"##,
    ] {
        assert!(ElementDefinition::from_xml(source).is_err());
    }
}
