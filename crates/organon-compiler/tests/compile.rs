use organon_compiler::{compile_xml, generate_element_rust};
use organon_format::ValueExpr;
use organon_format::{CanvasExpr, CoverageExpr, ElementDefinition, Input, InputType, PathExpr};

const SOURCE: &str = r##"<element name="Example" version="1">
  <input name="width" type="f32"/>
  <input name="fill" type="Color"/>
  <input name="label" type="Canvas"/>
  <let name="drawing" export="true">
    <SourceOver space="LinearSrgb">
      <Draw source="{fill}"><Fill><Rect x="0" y="0" width="{width}" height="40"/></Fill></Draw>
      <Translate x="12" y="8"><ref name="label"/></Translate>
    </SourceOver>
  </let>
</element>"##;

#[test]
fn xml_and_format_entry_points_generate_the_same_rust() {
    let definition = ElementDefinition::from_xml(SOURCE).unwrap();
    let code = generate_element_rust(&definition).unwrap();
    assert_eq!(compile_xml(SOURCE).unwrap(), code);
    syn::parse_file(&code).unwrap();
}

#[test]
fn color_codes_preserve_authored_bytes_and_alpha() {
    for (text, expected) in [
        ("#1234aB", [0x12, 0x34, 0xab, 255]),
        ("#1234ab56", [0x12, 0x34, 0xab, 0x56]),
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><let name="drawing" export="true"><Draw source="{text}"><Fill><Rect x="0" y="0" width="1" height="2"/></Fill></Draw></let></element>"#
        );
        let definition = ElementDefinition::from_xml(&xml).unwrap();
        let [r, g, b, a] = expected;
        let code = generate_element_rust(&definition).unwrap();
        assert!(code.contains(&format!("[{r}_u8, {g}, {b}]")));
        assert!(code.contains(&format!("from({a}_u8)")));
    }
}

#[test]
fn rejects_invalid_values_and_input_bindings() {
    for source in [
        SOURCE.replace("height=\"40\"", "height=\"width / 2\""),
        SOURCE.replace("height=\"40\"", "height=\"NaN\""),
        SOURCE.replace("height=\"40\"", "height=\"inf\""),
        SOURCE.replace("height=\"40\"", "height=\"-1\""),
        SOURCE.replace("{width}", "{missing}"),
        SOURCE.replace("{width}", "$width"),
        SOURCE.replace("{width}", "{missing}0"),
        SOURCE.replace("{width}", "{fill}0"),
        SOURCE.replace("{width}", "{fill}"),
        SOURCE.replace("<ref name=\"label\"", "<ref name=\"fill\""),
        SOURCE.replace("{fill}", "#nothex"),
        SOURCE.replace("{fill}", "#あいうえ"),
        SOURCE.replace("LinearSrgb", "unknown-space"),
    ] {
        compile_xml(&source).unwrap_err();
    }
}

#[test]
fn rejects_invalid_and_duplicate_input_names_but_supports_rust_keywords() {
    for name in [
        "",
        "_",
        "self",
        "Self",
        "super",
        "crate",
        "x;panic!()",
        "a-b",
    ] {
        assert!(compile_xml(&SOURCE.replace("width", name)).is_err());
    }
    let duplicate = SOURCE.replace("<input name=\"fill\"", "<input name=\"width\"");
    assert!(compile_xml(&duplicate).is_err());
    let keyword = SOURCE
        .replace("name=\"width\"", "name=\"type\"")
        .replace("{width}", "{type}");
    let code = compile_xml(&keyword).unwrap();
    syn::parse_file(&code).unwrap();
    assert!(code.contains("inputs.r#type"));
}

#[test]
fn validates_attribute_meaning_before_generating_code() {
    let mut definition = ElementDefinition::from_xml(SOURCE).unwrap();
    definition.inputs.push(Input {
        name: "bad-name".into(),
        kind: InputType::Scalar,
    });
    assert!(generate_element_rust(&definition).is_err());
    let mut definition = ElementDefinition::from_xml(SOURCE).unwrap();
    definition.definitions[0].value = ValueExpr::Canvas(CanvasExpr::Draw {
        source: "#00000000".into(),
        coverage: CoverageExpr::Fill(PathExpr::Rect {
            x: f32::MAX.to_string(),
            y: "0".into(),
            width: f32::MAX.to_string(),
            height: "1".into(),
        }),
    });
    assert!(generate_element_rust(&definition).is_err());
}

#[test]
fn propagates_format_errors_through_the_compiler_entry_point() {
    let xml = SOURCE.replace("height=\"40\"", "heigth=\"40\"");
    let expected = ElementDefinition::from_xml(&xml).unwrap_err().to_string();
    assert_eq!(compile_xml(&xml).unwrap_err().to_string(), expected);
}

#[test]
fn fields_are_native_resources_and_cannot_be_interpolated_as_text() {
    let source = include_str!("fixtures/xml/fields.organon.xml");
    syn::parse_file(&compile_xml(source).unwrap()).unwrap();
    for invalid in [
        source.replace("source=\"{brush}\"", "source=\"prefix {brush}\""),
        source.replace("source=\"{brush}\"", "source=\"{host_path}\""),
        source.replace("source=\"{brush}\"", "source=\"{unknown}\""),
        source.replace("<ref name=\"outline\"", "<ref name=\"brush\""),
        source.replace("width=\"{width}\"", "width=\"{brush}\""),
    ] {
        assert!(compile_xml(&invalid).is_err(), "{invalid}");
    }
}
