use organon_compiler::{compile_xml, generate_file_with_implementations};
use organon_format::{DefinitionFile, ElementDefinition};
use std::collections::BTreeMap;

const SOURCE: &str = r#"<organon version="1">
  <element name="Parent">
    <input name="width" type="f32"/>
    <let name="out" export="true"><get name="path"><ref name="child"/></get></let>
    <let name="child"><Child width="{width}"/></let>
  </element>
  <element name="Child">
    <input name="width" type="f32"/>
    <let name="private"><ref name="width"/></let>
    <let name="path" export="true"><Rect x="0" y="0" width="{width}" height="10"/></let>
  </element>
</organon>"#;

#[test]
fn resolves_forward_type_and_output_references_without_default_outputs() {
    let file = DefinitionFile::from_xml(SOURCE).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    syn::parse_file(&compile_xml(SOURCE).unwrap()).unwrap();
    for (bad, message) in [
        (
            SOURCE.replace("<get name=\"path\">", "<get name=\"private\">"),
            "private output",
        ),
        (
            SOURCE.replace("<get name=\"path\">", "<get name=\"missing\">"),
            "private output",
        ),
        (
            SOURCE.replace("<Child width=\"{width}\"/>", "<Child/>"),
            "missing input",
        ),
        (
            SOURCE.replace("<Child width=\"{width}\"/>", "<Child wrong=\"1\"/>"),
            "unknown input",
        ),
        (
            SOURCE.replace("<Child width=", "<Missing width="),
            "unknown element",
        ),
        (
            SOURCE.replace("<Child width=\"{width}\"/>", "<Parent width=\"{width}\"/>"),
            "cyclic element construction",
        ),
    ] {
        let error = compile_xml(&bad).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn validates_instance_argument_types_and_interface_implementation_bindings() {
    let source = r#"<organon version="1">
      <interface name="Native"><input name="value" type="Path"/><output name="value" type="Path"/></interface>
      <element name="Host"><input name="path" type="Path"/>
        <let name="native"><Native value="{path}"/></let>
        <let name="out" export="true"><get name="value"><ref name="native"/></get></let>
      </element></organon>"#;
    assert!(
        compile_xml(source)
            .unwrap_err()
            .to_string()
            .contains("implementation binding")
    );
    let implementations = BTreeMap::from([("Native".to_owned(), "crate::Make".to_owned())]);
    let file = DefinitionFile::from_xml(source).unwrap();
    syn::parse_file(&generate_file_with_implementations(&file, &implementations).unwrap()).unwrap();
    let bad =
        DefinitionFile::from_xml(&source.replace("value=\"{path}\"", "value=\"123\"")).unwrap();
    assert!(generate_file_with_implementations(&bad, &implementations).is_err());
    let bad = DefinitionFile::from_xml(&source.replace(
        "<input name=\"path\" type=\"Path\"/>",
        "<let name=\"path\"><Rect x=\"0\" y=\"0\" width=\"1\" height=\"1\"/></let>",
    ))
    .unwrap();
    assert!(
        generate_file_with_implementations(&bad, &implementations)
            .unwrap_err()
            .to_string()
            .contains("explicit error handling")
    );
}

#[test]
fn instance_storage_round_trips_without_requiring_its_dependency_files() {
    let source = r#"<element name="Parent" version="1">
      <let name="child"><Child text="literal {{not.a.reference}}"/></let>
      <let name="out" export="true"><get name="drawing"><ref name="child"/></get></let>
    </element>"#;
    let file = ElementDefinition::from_xml(source).unwrap();
    assert_eq!(
        ElementDefinition::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    assert!(
        ElementDefinition::from_xml(
            &source.replace("<let name=\"child\"", "<let name=\"child\" export=\"true\"")
        )
        .is_ok()
    );
    assert!(
        ElementDefinition::from_xml(
            &source.replace("<let name=\"child\"", "<let name=\"child\" inline=\"true\"")
        )
        .is_ok()
    );
}
