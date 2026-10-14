use organon_compiler::{compile_xml, generate_element_rust};
use organon_format::ElementDefinition;

#[test]
fn ref_aliases_preserve_every_supported_input_type() {
    for kind in [
        "f32",
        "String",
        "Color",
        "ColorField",
        "Canvas",
        "Path",
        "Fonts",
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><input name="source" type="{kind}"/><let name="alias" export="true"><ref name="source"/></let></element>"#
        );
        let attribute = xml.replace(r#"><ref name="source"/></let>"#, r#" value="{source}"/>"#);
        let code = compile_xml(&xml).unwrap();
        assert!(compile_xml(&attribute).is_err());
        syn::parse_file(&code).unwrap();
    }
}

#[test]
fn refs_resolve_names_and_check_the_enclosing_type() {
    for body in [
        r#"<let name="a" export="true"><ref name="missing"/></let>"#,
        r#"<let name="a" export="true"><ref name="b"/></let><let name="b"><ref name="a"/></let>"#,
        r#"<let name="a" export="true"><Translate x="0" y="0"><ref name="path"/></Translate></let>"#,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill><ref name="canvas"/></Fill></Draw></let>"##,
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><input name="path" type="Path"/><input name="canvas" type="Canvas"/>{body}</element>"#
        );
        // The syntax is valid; name resolution or type checking rejects it.
        ElementDefinition::from_xml(&xml).unwrap();
        assert!(compile_xml(&xml).is_err(), "{xml}");
    }
}

const SOURCE: &str = r##"<element name="Example" version="1">
  <input name="width" type="f32"/>
  <let name="shape"><Polygon>
    <Point x="0" y="0"/><Point x="{width}" y="0"/><Point x="0" y="20"/>
  </Polygon></let>
  <let name="outline" export="true"><ref name="shape"/></let>
  <let name="drawing" export="true"><Draw source="#ffffff"><Fill><ref name="shape"/></Fill></Draw></let>
</element>"##;

#[test]
fn entry_points_produce_a_typed_multi_output_builder() {
    let definition = ElementDefinition::from_xml(SOURCE).unwrap();
    let generated = generate_element_rust(&definition).unwrap();
    assert_eq!(generated, compile_xml(SOURCE).unwrap());
    syn::parse_file(&generated).unwrap();
    assert!(generated.contains("pub struct r#Example"));
    assert!(generated.contains("pub fn r#outline_fixed"));
    assert!(generated.contains("pub r#outline: ::theoria_canvas::path::Path"));
}

#[test]
fn unresolved_or_wrongly_typed_references_fail_before_code_generation() {
    for bad in [
        SOURCE.replace("{width}", "{missing}"),
        SOURCE.replace("{width}", "{shape}"),
        SOURCE.replace("type=\"f32\"", "type=\"Canvas\""),
        SOURCE.replace("{width}", "NaN"),
        SOURCE.replace("<ref name=\"shape\"", "<ref name=\"width\""),
        SOURCE.replace("<ref name=\"shape\"", "<ref name=\"missing\""),
        SOURCE.replace("name=\"outline\"", "name=\"self\""),
    ] {
        assert!(compile_xml(&bad).is_err(), "{bad}");
    }
    let malformed = SOURCE.replace("<Polygon>", "<Polygon unknown=\"yes\">");
    assert!(
        compile_xml(&malformed)
            .unwrap_err()
            .to_string()
            .contains("unknown attribute")
    );
}

#[test]
fn path_only_and_rust_keyword_names_generate_valid_modules() {
    let source = r#"<element name="Example" version="1"><input name="type" type="Path"/><let name="match" export="true"><ref name="type"/></let></element>"#;
    let code = compile_xml(source).unwrap();
    syn::parse_file(&code).unwrap();
    assert!(!code.contains("Coverage"));
}

#[test]
fn lets_check_placement_dependencies_and_reference_types() {
    for body in [
        r#"<let name="a" export="true"><ref name="missing"/></let>"#,
        r#"<let name="a" export="true"><ref name="b"/></let><let name="b"><ref name="a"/></let>"#,
        r#"<let name="a" export="true"><let name="b"><ref name="a"/></let></let>"#,
        r#"<let name="a" export="true"><Translate x="0" y="0"><ref name="b"/></Translate></let><let name="b"><Rect x="0" y="0" width="1" height="1"/></let>"#,
        r##"<let name="a" export="true"><Draw source="#ffffff"><Fill><ref name="b"/></Fill></Draw></let><let name="b"><Draw source="#ffffff"><All/></Draw></let>"##,
    ] {
        let source = format!("<element name=\"Example\" version=\"1\">{body}</element>");
        assert!(compile_xml(&source).is_err(), "{source}");
    }
}

#[test]
fn internal_inline_bindings_are_expanded_without_callable_values() {
    let source = include_str!("fixtures/xml/reactive.organon.xml");
    let code = compile_xml(source).unwrap();
    syn::parse_file(&code).unwrap();
    assert!(!code.contains("source_inline_shape"));
    assert!(!code.contains("source_raw_shape"));
    assert!(code.contains("let bound_direct =")); // Public inline value needs a reading interface.
    assert!(code.contains("let binding_inline_shape =")); // Internal use is a local expression.
}

#[test]
fn public_api_rejects_colliding_generated_names() {
    for name in ["Builder", "Input", "Result", "__Output", "self", "_"] {
        assert!(
            compile_xml(&SOURCE.replace("name=\"Example\"", &format!("name=\"{name}\""))).is_err()
        );
    }
    for name in ["build", "build_in", "component", "__build", "__element"] {
        assert!(compile_xml(&SOURCE.replace("width", name)).is_err());
    }
    assert!(compile_xml(&SOURCE.replace("name=\"outline\"", "name=\"builder\"")).is_err());
    assert!(compile_xml(&SOURCE.replace("name=\"drawing\"", "name=\"outline_fixed\"")).is_err());
}
