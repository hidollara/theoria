use organon_core::syntax::{Element, Identifier, IdentifierRef};

#[test]
fn identifiers_validate_spelling_without_assuming_a_rust_name() {
    for text in ["_", "_value", "x2", "Panel", "type"] {
        let name: Identifier = text.parse().unwrap();
        let borrowed = IdentifierRef::parse(text).unwrap();
        assert_eq!(borrowed, name.as_borrowed());
        assert_eq!(borrowed.to_owned(), name);
        assert_eq!(name.as_str(), text);
        assert_eq!(Identifier::try_from(text.to_owned()).unwrap(), name);
        assert_eq!(name.into_string(), text);
    }
    for text in ["", "2x", "-x", "x-y", "a.b", "{name}", "a b", "名前"] {
        assert!(text.parse::<Identifier>().is_err(), "{text:?}");
        assert!(IdentifierRef::parse(text).is_err(), "{text:?}");
        assert!(IdentifierRef::try_from(text).is_err(), "{text:?}");
        assert!(Identifier::try_from(text.to_owned()).is_err(), "{text:?}");
    }
}

#[test]
fn borrowed_identifiers_support_const_parsing_and_ownership_transfer() {
    const NAME: IdentifierRef<'static> = match IdentifierRef::parse("width") {
        Ok(name) => name,
        Err(_) => panic!("valid identifier rejected"),
    };
    const INVALID: Result<IdentifierRef<'static>, &str> = IdentifierRef::parse("bad-name");
    const SPELLING: &str = NAME.as_str();
    assert_eq!(SPELLING, "width");
    assert_eq!(INVALID.unwrap_err(), "invalid identifier");

    let owned = {
        let text = String::from("height");
        let borrowed = IdentifierRef::try_from(text.as_str()).unwrap();
        assert_eq!(borrowed.as_str().as_ptr(), text.as_ptr());
        borrowed.to_owned()
    };
    assert_eq!(owned.as_borrowed().as_str(), "height");
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
        r#"<element name="Panel"><let name="x"><try type="Text" value="ok"><catch error="Fault" capture="bad-name" value="failed"/></try></let></element>"#,
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
    use organon_core::syntax::ElementExpression;

    let ElementExpression::Construct(value) =
        organon_core::xml::parse_expression(r#"<Text t="こんにちは {user-name} / a.b"/>"#).unwrap()
    else {
        panic!("expected construction");
    };
    assert_eq!(value.arguments["t"], "こんにちは {user-name} / a.b");
}
