use organon_format::{ElementDefinition, Segment, Template};

#[test]
fn parses_interpolation_and_escaped_braces_without_expanding_inputs() {
    assert_eq!(
        Template::parse("こんにちは、{name}さん {{{count}}} $price")
            .unwrap()
            .segments(),
        &[
            Segment::Literal("こんにちは、".into()),
            Segment::Reference("name".into()),
            Segment::Literal("さん {".into()),
            Segment::Reference("count".into()),
            Segment::Literal("} $price".into()),
        ]
    );
    assert_eq!(Template::parse("").unwrap().literal(), Some(""));
    assert_eq!(
        Template::parse("{{name}}").unwrap().literal(),
        Some("{name}")
    );
    assert_eq!(Template::parse("{name}").unwrap().reference(), Some("name"));
    assert_eq!(Template::parse("{a}{b}").unwrap().reference(), None);
}

#[test]
fn rejects_incomplete_nested_and_unsupported_expressions() {
    for invalid in [
        "{", "}", "{name", "name}", "{}", "{a{b}}", "{ name }", "{x:02}", "{x..y}", "{x + 1}",
        "{0}",
    ] {
        assert!(Template::parse(invalid).is_err(), "{invalid}");
        let xml = format!(
            r##"<element name="Example" version="1"><let name="drawing" export="true"><Draw source="#ffffff"><Fill><Rect x="0" y="0" width="{invalid}" height="1"/></Fill></Draw></let></element>"##
        );
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{invalid}");
    }
}

#[test]
fn named_outputs_are_references_but_escaped_text_stays_literal() {
    assert_eq!(
        Template::parse("{panel.path}").unwrap().reference(),
        Some("panel.path")
    );
    assert_eq!(
        Template::parse("{{panel.path}}").unwrap().literal(),
        Some("{panel.path}")
    );
    for text in ["{.path}", "{panel.}", "{panel..path}"] {
        assert!(Template::parse(text).is_err());
    }
}

#[test]
fn fixed_attributes_and_resource_references_have_distinct_policies() {
    for xml in [
        r#"<element name="Example" version="1"><input name="{name}" type="f32"/><let name="drawing" export="true"><ref name="canvas"/></let></element>"#,
        r#"<element name="Example" version="1"><let name="drawing" export="true"><SourceOver space="{space}"><ref name="a"/><ref name="b"/></SourceOver></let></element>"#,
        r#"<element name="Example" version="1"><let name="drawing" export="true"><ref name="{canvas}"/></let></element>"#,
        r#"<element name="Example" version="1"><let name="drawing" export="true"><ref name="prefix {canvas}"/></let></element>"#,
        r##"<element name="Example" version="1"><let name="drawing" export="true"><Text fonts="{a}{b}" font_size="20" foreground="#ffffff">
      <s t="hello"/></Text></let></element>"##,
    ] {
        assert!(ElementDefinition::from_xml(xml).is_err(), "{xml}");
    }
}
