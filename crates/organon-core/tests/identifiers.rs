use organon_core::{identifier::Identifier, syntax::Element};

#[test]
fn identifiers_validate_spelling_without_assuming_a_rust_name() {
    for text in ["_", "_value", "x2", "Panel", "type"] {
        let name: Identifier = text.parse().unwrap();
        assert_eq!(name.as_str(), text);
        assert_eq!(Identifier::try_from(text.to_owned()).unwrap(), name);
        assert_eq!(name.into_string(), text);
    }
    for text in ["", "2x", "-x", "x-y", "a.b", "{name}", "a b", "名前"] {
        assert!(text.parse::<Identifier>().is_err(), "{text:?}");
        assert!(Identifier::try_from(text.to_owned()).is_err(), "{text:?}");
    }
}

#[test]
fn lexical_validity_does_not_make_a_reserved_name_a_definition() {
    let mut element = Element {
        name: "let".parse().unwrap(),
        children: None,
        members: vec![],
    };
    assert!(element.validate().is_err());
    element.name = "Panel".parse().unwrap();
    element.validate().unwrap();
}

#[cfg(feature = "xml")]
#[test]
fn xml_rejects_invalid_names_before_resolution() {
    for content in [
        r#"<element name="bad-name"/>"#,
        r#"<declare-element name="Panel"><input name="bad-name" type="Number"/></declare-element>"#,
        r#"<interface name="Drawable"><output name="bad-name" type="Canvas"/></interface>"#,
        r#"<element name="Panel"><let name="bad-name"><Rect/></let></element>"#,
        r#"<element name="Panel"><bind bad-name="value"><Rect/></bind></element>"#,
        r#"<element name="Panel"><bind width="bad-name"><Rect/></bind></element>"#,
        r#"<element name="Panel"><let name="x"><ref name="bad-name"/></let></element>"#,
        r#"<element name="Panel"><let name="x"><Rect bad-name="100"/></let></element>"#,
        r#"<element name="Panel"><provide name="width" ref="bad-name"/></element>"#,
        r#"<use from="./parts.xml" name="Panel" as="bad-name"/>"#,
        r#"<export name="Panel" as="bad-name"/>"#,
        r#"<declare-element name="Panel"><implements interface="bad-name"/></declare-element>"#,
        r#"<element name="Panel"><let name="x"><try type="Text" result="ok"><catch type="Fault" name="bad-name" result="failed"/></try></let></element>"#,
    ] {
        let xml = format!("<organon version=\"1\">\n{content}\n</organon>");
        let error = organon_core::xml::parse(&xml).unwrap_err().to_string();
        assert!(error.starts_with("2:"), "{content}: {error}");
        assert!(error.contains("identifier"), "{content}: {error}");
    }
}

#[cfg(feature = "xml")]
#[test]
fn xml_argument_text_remains_provider_owned() {
    use organon_core::syntax::ValueExpression;

    let ValueExpression::Construct(value) =
        organon_core::xml::parse_expression(r#"<Text t="こんにちは {user-name} / a.b"/>"#).unwrap()
    else {
        panic!("expected construction");
    };
    assert_eq!(value.arguments["t"], "こんにちは {user-name} / a.b");
}
