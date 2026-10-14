use organon_compiler::compile_xml;

const SOURCE: &str = r##"<organon version="1">
  <element name="Host">
    <input name="width" type="f32"/>
    <let name="drawing" export="true"><Translate x="0" y="0">
      <get name="drawing"><Panel width="{width}"/></get>
    </Translate></let>
    <let name="path" export="true"><get name="path"><ref name="panel"/></get></let>
    <let name="mask" export="true"><Draw source="#ffffff"><Fill>
      <get name="path"><Panel width="{width}"/></get>
    </Fill></Draw></let>
    <let name="panel"><Panel width="{width}"/></let>
  </element>
  <element name="Panel">
    <input name="width" type="f32"/>
    <let name="private"><ref name="width"/></let>
    <let name="path" export="true"><Rect x="0" y="0" width="{width}" height="10"/></let>
    <let name="drawing" export="true"><Draw source="#ffffff"><Fill><ref name="path"/></Fill></Draw></let>
  </element>
</organon>"##;

#[test]
fn resolves_anonymous_and_named_selection_in_value_canvas_and_path_positions() {
    syn::parse_file(&compile_xml(SOURCE).unwrap()).unwrap();
    // Authored names cannot collide with lifted anonymous instances.
    let collision = SOURCE.replace("name=\"panel\"", "name=\"__organon_instance_5\"");
    syn::parse_file(&compile_xml(&collision).unwrap()).unwrap();
}

#[test]
fn rejects_wrong_output_types_private_outputs_and_invalid_targets() {
    for (source, message) in [
        (
            SOURCE.replace("<get name=\"drawing\">", "<get name=\"path\">"),
            "Canvas",
        ),
        (
            SOURCE.replace("<get name=\"path\"><Panel", "<get name=\"drawing\"><Panel"),
            "Path",
        ),
        (
            SOURCE.replace("<get name=\"path\">", "<get name=\"private\">"),
            "private output",
        ),
        (
            SOURCE.replace("<get name=\"path\">", "<get name=\"missing\">"),
            "private output",
        ),
        (
            SOURCE.replace("<ref name=\"panel\"/>", "<ref name=\"width\"/>"),
            "get requires an element",
        ),
        (
            SOURCE.replace(
                "<get name=\"drawing\"><Panel width=\"{width}\"/></get>",
                "<get name=\"drawing\"><Rect x=\"0\" y=\"0\" width=\"1\" height=\"1\"/></get>",
            ),
            "get requires an element",
        ),
        (
            SOURCE.replace("<Panel width=\"{width}\"/>", "<Host width=\"{width}\"/>"),
            "cyclic element construction",
        ),
        (
            SOURCE.replace("<Panel width=\"{width}\"/>", "<Missing/>"),
            "unknown element",
        ),
        (
            SOURCE.replace("<Panel width=\"{width}\"/>", "<Panel/>"),
            "missing input",
        ),
    ] {
        let error = compile_xml(&source).unwrap_err().to_string();
        assert!(error.contains(message), "expected {message}: {error}");
    }
}

#[test]
fn single_definition_cannot_hide_recursive_construction_in_get() {
    let source = r#"<element name="Loop" version="1">
      <let name="out" export="true"><get name="out"><Loop/></get></let>
    </element>"#;
    assert!(
        compile_xml(source)
            .unwrap_err()
            .to_string()
            .contains("cyclic element construction")
    );
}
