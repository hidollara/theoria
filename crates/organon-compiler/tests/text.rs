use organon_compiler::{compile_xml, generate_element_rust};
use organon_format::ValueExpr;
use organon_format::{CanvasExpr, ElementDefinition};

const TEXT: &str = r##"<element name="Example" version="1">
  <input name="fonts" type="Fonts"/>
  <input name="label" type="String"/>
  <input name="width" type="f32"/>
  <let name="drawing" export="true">
    <Text fonts="{fonts}" font_size="20" foreground="#aabbcc" wrap_width="{width}">
      <s t="{label}"/>
    </Text>
  </let>
</element>"##;

#[test]
fn text_declarations_generate_valid_rust_with_typed_bindings() {
    let definition = ElementDefinition::from_xml(TEXT).unwrap();
    syn::parse_file(&generate_element_rust(&definition).unwrap()).unwrap();
}

#[test]
fn literal_text_is_escaped_as_a_rust_string_literal() {
    let mut definition = ElementDefinition::from_xml(TEXT).unwrap();
    for literal in [
        "",
        "{label}",
        "$cash",
        "{{nested}} {name} $label",
        "日本語 & <tag> \"quoted\"\n\t\rnext",
        "\\\"; panic!(); //",
    ] {
        let ValueExpr::Canvas(CanvasExpr::Text {
            content,
            wrap_width,
            ..
        }) = &mut definition.definitions[0].value
        else {
            panic!()
        };
        *content = vec![organon_format::TextContent::string(
            literal.replace('{', "{{").replace('}', "}}"),
        )];
        *wrap_width = None;
        let code = generate_element_rust(&definition).unwrap();
        syn::parse_file(&code).unwrap();
        assert!(code.contains(&format!("Span::new({literal:?})")));
    }
}

#[test]
fn rejects_mistyped_text_styles_dimensions_and_inputs() {
    for source in [
        TEXT.replace("{label}", "{fonts}"),
        TEXT.replace("{label}", "{unknown}"),
        TEXT.replace("{fonts}", "{label}"),
        TEXT.replace("fonts=\"{fonts}\"", "fonts=\"system-ui\""),
        TEXT.replace("font_size=\"20\"", "font_size=\"-1\""),
        TEXT.replace("font_size=\"20\"", "font_size=\"NaN\""),
        TEXT.replace("{width}", "-1"),
        TEXT.replace("{width}", "inf"),
        TEXT.replace("{width}", "{fonts}"),
        TEXT.replace("#aabbcc", "{fonts}"),
    ] {
        assert!(compile_xml(&source).is_err(), "{source}");
    }
}

#[test]
fn generation_checks_format_validity_for_programmatically_built_text() {
    let mut definition = ElementDefinition::from_xml(TEXT).unwrap();
    for text in ["\0", "\u{1}", "\u{fffe}", "\u{ffff}"] {
        let ValueExpr::Canvas(CanvasExpr::Text { content, .. }) =
            &mut definition.definitions[0].value
        else {
            panic!()
        };
        *content = vec![organon_format::TextContent::string(text)];
        assert!(generate_element_rust(&definition).is_err());
    }
}

#[test]
fn inline_style_and_template_dependencies_are_checked_even_on_empty_ranges() {
    for body in [
        r#"<s t="{fonts}"/>"#,
        r#"<span font_size="{unknown}"><s t="x"/></span>"#,
        r#"<span fonts="{label}"/>"#,
        r#"<span font_size="-1"/>"#,
        r#"<s t="x" foreground="{fonts}"/>"#,
        r#"<span foreground="invalid"/>"#,
    ] {
        let source = TEXT.replace(r#"<s t="{label}"/>"#, body);
        assert!(compile_xml(&source).is_err(), "{body}");
    }
}
