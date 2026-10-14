use organon_compiler::compile_xml;

const SOURCE: &str = r#"<organon version="1">
  <interface name="Panel">
    <input name="width" type="f32"/>
    <output name="value" type="f32"/>
  </interface>
  <element name="Picture">
    <input name="canvas" type="Canvas"/>
    <let name="drawing" export="true"><ref name="canvas"/></let>
  </element>
</organon>"#;

#[test]
fn files_generate_contracts_and_element_definitions_together() {
    let code = compile_xml(SOURCE).unwrap();
    syn::parse_file(&code).unwrap();
    let file = organon_format::DefinitionFile::from_xml(SOURCE).unwrap();
    assert_eq!(code, organon_compiler::generate_file_rust(&file).unwrap());
}

#[test]
fn contracts_reject_rust_names_and_accessor_collisions() {
    for source in [
        SOURCE.replace("name=\"Panel\"", "name=\"Built\""),
        SOURCE.replace("name=\"width\"", "name=\"build\""),
        SOURCE.replace("name=\"width\"", "name=\"self\""),
        SOURCE.replace(
            "</interface>",
            "<output name=\"value_fixed\" type=\"f32\"/></interface>",
        ),
        SOURCE.replace(
            "</interface>",
            "<output name=\"value_source\" type=\"f32\"/></interface>",
        ),
    ] {
        assert!(compile_xml(&source).is_err(), "{source}");
    }
}
