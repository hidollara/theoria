use organon_format::{Definition, DefinitionFile};

const SOURCE: &str = r#"<organon version="1">
  <interface name="Panel">
    <input name="width" type="f32"/>
    <output name="path" type="Result&lt;Path&gt;" inline="true"/>
  </interface>
  <element name="Drawing">
    <input name="canvas" type="Canvas"/>
    <let name="drawing" export="true"><ref name="canvas"/></let>
  </element>
</organon>"#;

#[test]
fn contracts_and_elements_share_a_versioned_file_and_round_trip() {
    let file = DefinitionFile::from_xml(SOURCE).unwrap();
    assert_eq!(file.definitions.len(), 2);
    let Definition::Interface(contract) = &file.definitions[0] else {
        panic!()
    };
    assert!(contract.outputs[0].inline);
    assert!(contract.outputs[0].port.ty.fallible);
    let serialized = file.to_xml().unwrap();
    assert_eq!(serialized.matches("version=").count(), 1);
    assert_eq!(DefinitionFile::from_xml(&serialized).unwrap(), file);
}

#[test]
fn malformed_contracts_are_rejected_at_the_format_boundary() {
    for source in [
        SOURCE.replace("version=\"1\"", "version=\"2\""),
        SOURCE.replace("name=\"Drawing\"", "name=\"Panel\""),
        SOURCE.replace("type=\"f32\"", "type=\"Imaginary!\""),
        SOURCE.replace("inline=\"true\"", "inline=\"{dynamic}\""),
        SOURCE.replace(
            "<interface name=\"Panel\">",
            "<interface name=\"Panel\" version=\"1\">",
        ),
        SOURCE.replace(
            "<element name=\"Drawing\">",
            "<element name=\"Drawing\" version=\"1\">",
        ),
        SOURCE.replace(
            "</interface>",
            "<input name=\"width\" type=\"f32\"/></interface>",
        ),
        SOURCE.replace(
            "</interface>",
            "<output name=\"path\" type=\"Path\"/></interface>",
        ),
        SOURCE.replace("</interface>", "<unexpected/></interface>"),
        "<organon version=\"1\"><interface name=\"Empty\"/></organon>".into(),
    ] {
        assert!(DefinitionFile::from_xml(&source).is_err(), "{source}");
    }
}

#[test]
fn imports_round_trip_without_resolving_files() {
    let source = r#"<organon version="1"><import path="../parts/a &amp; b.xml"/></organon>"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    assert_eq!(file.imports, ["../parts/a & b.xml"]);
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    for body in [
        r#"<import path=""/>"#,
        r#"<import path="x" extra="1"/>"#,
        r#"<import path="x"><element/></import>"#,
    ] {
        assert!(
            DefinitionFile::from_xml(&format!("<organon version=\"1\">{body}</organon>")).is_err()
        );
    }
}
