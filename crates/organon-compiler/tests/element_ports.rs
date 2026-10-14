use organon_compiler::{compile_xml, generate_element_rust};
use organon_format::ElementDefinition;

const SOURCE: &str = r#"<organon version="1">
  <element name="Host">
    <input name="button" type="Button"/>
    <let name="forward" export="true"><Button tile="{button.icon}"/></let>
    <let name="nested" export="true"><get name="extent"><get name="icon"><ref name="button"/></get></get></let>
    <let name="attribute" export="true"><Tile width="{button.icon.extent}"/></let>
  </element>
  <element name="Button">
    <input name="tile" type="Tile"/>
    <let name="icon" export="true"><ref name="tile"/></let>
  </element>
  <element name="Tile">
    <input name="width" type="f32"/>
    <let name="extent" export="true"><ref name="width"/></let>
    <let name="private"><ref name="width"/></let>
  </element>
</organon>"#;

#[test]
fn forward_named_types_and_nested_selections_are_resolved_before_codegen() {
    syn::parse_file(&compile_xml(SOURCE).unwrap()).unwrap();
    for (source, expected) in [
        (
            SOURCE.replace("type=\"Button\"", "type=\"Missing\""),
            "unknown element",
        ),
        (SOURCE.replace("{button.icon}", "{button}"), "expected Tile"),
        (
            SOURCE.replace("{button.icon.extent}", "{button.icon.private}"),
            "private output",
        ),
        (
            SOURCE.replace("{button.icon.extent}", "{button.icon.missing}"),
            "private output",
        ),
        (
            SOURCE.replace("{button.icon.extent}", "{button.icon.extent.field}"),
            "get requires an element",
        ),
        (
            SOURCE.replace("<get name=\"extent\">", "<get name=\"private\">"),
            "private output",
        ),
        (
            SOURCE.replace("type=\"Tile\"", "type=\"Host\""),
            "type dependency",
        ),
        (
            SOURCE.replace("name=\"private\"", "name=\"extent_source\" export=\"true\""),
            "ambiguous",
        ),
    ] {
        let error = compile_xml(&source).unwrap_err().to_string();
        assert!(error.contains(expected), "expected {expected}: {error}");
    }
}

#[test]
fn entry_points_reject_unresolved_named_types_and_recursive_types() {
    for name in ["Missing", "Host"] {
        let xml = format!(
            r#"<element name="Host" version="1"><input name="item" type="{name}"/><let name="out" export="true"><ref name="item"/></let></element>"#
        );
        let element = ElementDefinition::from_xml(&xml).unwrap();
        assert!(generate_element_rust(&element).is_err());
        assert!(compile_xml(&xml).is_err());
    }
    let xml = r#"<organon version="1"><interface name="Bridge"><output name="out" type="Missing"/></interface></organon>"#;
    assert!(
        compile_xml(xml)
            .unwrap_err()
            .to_string()
            .contains("unknown element type")
    );
}

#[test]
fn elements_never_implicitly_convert_to_their_only_output() {
    let source = SOURCE.replace(
        "<ref name=\"tile\"/>",
        r#"<Translate x="0" y="0"><ref name="tile"/></Translate>"#,
    );
    assert!(
        compile_xml(&source)
            .unwrap_err()
            .to_string()
            .contains("Canvas")
    );
}
