use organon_core::format::DefinitionFile;
use organon_core::format::{Instance, ValueExpr};

#[test]
fn publication_and_element_outputs_are_distinct() {
    let source = r#"<organon version="1">
  <export name="Example" as="PublicExample"/>
  <element name="Example">
    <input name="value" type="Number"/>
    <provide name="result" ref="value"/>
  </element>
  <declare name="Number" tag="false">
  </declare>
</organon>
"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    assert_eq!(file.to_xml().unwrap(), source);
    assert!(matches!(
        &file.declarations[0],
        organon_core::format::FileDeclaration::Export(p) if p.public_name() == "PublicExample"
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
        assert!(DefinitionFile::from_xml(&invalid).is_err(), "{invalid}");
    }
    assert!(ValueExpr::from_xml("<export name=\"Example\"/>").is_err());
}

#[test]
fn documents_have_one_organon_root_and_elements_have_no_version() {
    assert!(DefinitionFile::from_xml(r#"<element name="Old" version="1"/>"#).is_err());
    assert!(
        DefinitionFile::from_xml(
            r#"<organon version="1"><element name="Old" version="1"/></organon>"#
        )
        .is_err()
    );
    let source = r#"<organon version="1">
  <interface name="Contract">
    <output name="value" type="Number"/>
  </interface>
  <declare name="Number" tag="false">
  </declare>
  <declare name="Native">
    <output name="value" type="Number"/>
    <impl interface="Contract">
      <provide name="value" ref="value"/>
    </impl>
  </declare>
  <element name="Example">
    <input name="value" type="Number"/>
    <impl interface="Contract">
      <provide name="value" ref="value"/>
    </impl>
    <provide name="value" ref="value"/>
  </element>
</organon>
"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    let saved = file.to_xml().unwrap();
    assert_eq!(saved, source);
    assert_eq!(DefinitionFile::from_xml(&saved).unwrap(), file);
}

#[test]
fn expression_fragments_obey_the_same_format_rules_as_documents() {
    let source = "<Foreign text=\"&amp;&lt;&quot;&#10;&#13;&#9;{unclosed\"/>";
    let expression = ValueExpr::from_xml(source).unwrap();
    let saved = expression.to_xml().unwrap();
    assert_eq!(ValueExpr::from_xml(&saved).unwrap(), expression);
    for source in [
        "<Foreign bad-name=\"x\"/>",
        "<Foreign>text</Foreign>",
        "<ref name=\"a.b\"/>",
    ] {
        assert!(ValueExpr::from_xml(source).is_err(), "{source}");
    }
    let expression = ValueExpr::Instance(Instance {
        element: "Foreign".into(),
        inputs: [("xmlns".into(), "namespace".into())].into(),
        children: vec![],
    });
    assert!(expression.to_xml().is_err());
}

#[test]
fn format_keeps_semantic_errors_for_resolution() {
    let source = r#"<organon version="1">
      <element name="Example">
        <let name="a"><ref name="missing"/></let>
        <let name="a"><ref name="missing"/></let>
        <provide name="a" ref="a"/>
      </element>
    </organon>"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    assert!(organon_core::semantic::resolve(&file).is_err());
}
