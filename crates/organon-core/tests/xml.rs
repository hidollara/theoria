#![cfg(feature = "xml")]

use organon_core::syntax::ElementExpression;

#[test]
fn definition_content_does_not_encode_xml_presentation() {
    let compact = r#"<organon version="1"><element name="View"><let name="x"><Item a="&amp;" b="2"/></let></element></organon>"#;
    let formatted = "<organon version='1'>\r\n  <!-- comment -->\r\n  <element name='View'>\r\n    <let name='x'><Item b='2' a='&#38;'></Item></let>\r\n  </element>\r\n</organon>";
    assert_eq!(
        organon_core::xml::parse(compact).unwrap(),
        organon_core::xml::parse(formatted).unwrap()
    );
    assert!(organon_core::xml::parse(&compact.replace("b=\"2\"", "a=\"2\"")).is_err());
}

#[test]
fn publication_and_element_outputs_are_distinct() {
    let source = r#"<organon version="1">
  <export name="Example" as="PublicExample"/>
  <element name="Example">
    <input name="value" type="Number"/>
    <provide name="result" ref="value"/>
  </element>
  <declare-element name="Number" tag="false">
  </declare-element>
</organon>
"#;
    let file = organon_core::xml::parse(source).unwrap();
    file.validate().unwrap();
    assert!(matches!(
        &file.items[0],
        organon_core::syntax::Item::Export(p) if p.public_name() == "PublicExample"
    ));
    for invalid in [
        source.replace("<export name=\"Example\"", "<pub name=\"Example\""),
        source.replace(
            "<provide name=\"result\" ref=\"value\"/>",
            "<export name=\"value\"/>",
        ),
        source.replace(
            "<export name=\"Example\" as=\"PublicExample\"/>",
            "<provide name=\"Example\" ref=\"Example\"/>",
        ),
        source.replace(
            "<provide name=\"result\" ref=\"value\"/>",
            "<provide name=\"value\"/>",
        ),
        source.replace(
            "<provide name=\"result\" ref=\"value\"/>",
            "<provide name=\"value\" as=\"result\"/>",
        ),
        source.replace("<export name=\"Example\"", "<export name=\"a.b\""),
        source.replace(
            "<export name=\"Example\" as=\"PublicExample\"/>",
            "<export/>",
        ),
    ] {
        assert!(organon_core::xml::parse(&invalid).is_err(), "{invalid}");
    }
    assert!(organon_core::xml::parse_expression("<export name=\"Example\"/>").is_err());
}

#[test]
fn documents_have_one_organon_root_and_elements_have_no_version() {
    assert!(organon_core::xml::parse(r#"<element name="Old" version="1"/>"#).is_err());
    assert!(
        organon_core::xml::parse(
            r#"<organon version="1"><element name="Old" version="1"/></organon>"#
        )
        .is_err()
    );
    let source = r#"<organon version="1">
  <interface name="Contract">
    <output name="value" type="Number"/>
  </interface>
  <declare-element name="Number" tag="false">
  </declare-element>
  <declare-element name="Native">
    <output name="value" type="Number"/>
    <implements interface="Contract"/>
  </declare-element>
  <element name="Example">
    <input name="value" type="Number"/>
    <impl interface="Contract">
      <provide name="value" ref="value"/>
    </impl>
    <provide name="value" ref="value"/>
  </element>
</organon>
"#;
    let file = organon_core::xml::parse(source).unwrap();
    file.validate().unwrap();
}

#[test]
fn expression_fragments_obey_the_same_syntax_rules_as_documents() {
    let source = "<Foreign text=\"&amp;&lt;&quot;&#10;&#13;&#9;{unclosed\"/>";
    let expression = organon_core::xml::parse_expression(source).unwrap();
    let ElementExpression::Construct(construction) = expression else {
        panic!()
    };
    assert_eq!(construction.arguments["text"], "&<\"\n\r\t{unclosed");
    for source in [
        "<Foreign bad-name=\"x\"/>",
        "<Foreign>text</Foreign>",
        "<ref name=\"a.b\"/>",
    ] {
        assert!(
            organon_core::xml::parse_expression(source).is_err(),
            "{source}"
        );
    }
    assert!(organon_core::xml::parse_expression("<Foreign text=\"\0\"/>").is_err());
}

#[test]
fn syntax_keeps_semantic_errors_for_resolution() {
    let source = r#"<organon version="1">
      <element name="Example">
        <let name="a"><ref name="missing"/></let>
        <let name="a"><ref name="missing"/></let>
        <provide name="a" ref="a"/>
      </element>
    </organon>"#;
    let file = organon_core::xml::parse(source).unwrap();
    file.validate().unwrap();
    assert!(organon_core::resolver::resolve_module(&file).is_err());
}
