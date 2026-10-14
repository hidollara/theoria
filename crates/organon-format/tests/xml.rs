use organon_format::ValueExpr;
use organon_format::{
    CanvasExpr, CoverageExpr, ElementDefinition, Input, InputType, PathExpr, TextStyle,
};

#[test]
fn all_supported_drawing_shapes_round_trip() {
    for source in [
        include_str!("fixtures/drawing.organon.xml"),
        r##"<element name="Example" version="1"><input name="fonts" type="Fonts"/>
  <let name="drawing" export="true">
          <Text fonts="{fonts}" font_size="20" foreground="#33669980">
            <s t="A &amp; &lt;A&gt; &quot;A&quot;&#10;A"/>
          </Text>
  </let>
</element>"##,
    ] {
        let data = ElementDefinition::from_xml(source).unwrap();
        let saved = data.to_xml().unwrap();
        assert_eq!(ElementDefinition::from_xml(&saved).unwrap(), data);
        assert_eq!(data.to_xml().unwrap(), saved);
    }
}

#[test]
fn drawing_coverage_and_path_positions_are_distinct() {
    for body in [
        r##"<Draw source="#abcdef"><All/></Draw>"##,
        r##"<Draw source="#abcdef"><Fill><ref name="outline"/></Fill></Draw>"##,
        r##"<Draw source="#abcdef"><Fill><Polygon><Point x="0" y="0"/><Point x="2" y="0"/><Point x="0" y="2"/></Polygon></Fill></Draw>"##,
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><let name="drawing" export="true">{body}</let></element>"#
        );
        let data = ElementDefinition::from_xml(&xml).unwrap();
        assert_eq!(
            ElementDefinition::from_xml(&data.to_xml().unwrap()).unwrap(),
            data
        );
    }
    for body in [
        r##"<Fill path="{outline}" color="#ffffff"/>"##,
        r##"<Rect x="0" y="0" width="1" height="1" color="#ffffff"/>"##,
        r##"<Draw source="#ffffff"><Rect x="0" y="0" width="1" height="1"/></Draw>"##,
        r##"<Draw source="#ffffff"><Fill path="{outline}"><Polygon/></Fill></Draw>"##,
        r##"<Draw source="#ffffff"><Fill><Polygon><Point x="0" y="0"/></Polygon></Fill></Draw>"##,
        r##"<Draw source="#ffffff"><All/><All/></Draw>"##,
        r##"<Draw source="#ffffff"><All unexpected="1"/></Draw>"##,
        r##"<Draw source="#ffffff"><Fill path="{outline}" color="#ffffff"/></Draw>"##,
        r#"<Composite space="LinearSrgb"><ref name="a"/><ref name="b"/></Composite>"#,
        r#"<Composite operation="{mode}" space="LinearSrgb"><ref name="a"/><ref name="b"/></Composite>"#,
        r#"<SourceOver space="linear-srgb"><ref name="a"/><ref name="b"/></SourceOver>"#,
    ] {
        let xml = format!(
            r#"<element name="Example" version="1"><let name="drawing" export="true">{body}</let></element>"#
        );
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{xml}");
    }
}

#[test]
fn attributes_remain_authored_text_not_evaluated_values() {
    let data = ElementDefinition::from_xml(
        r##"<element name="Example" version="1">
      <input name="width" type="f32"/>
  <let name="drawing" export="true">
        <Draw source="#aAbBcC"><Fill><Rect x="001.00" y="-0" width="{width}" height="040.00"/></Fill></Draw>

  </let>
</element>"##,
    )
    .unwrap();
    let ValueExpr::Canvas(CanvasExpr::Draw {
        source: color,
        coverage:
            CoverageExpr::Fill(PathExpr::Rect {
                x,
                y,
                width,
                height,
            }),
    }) = &data.definitions[0].value
    else {
        panic!()
    };
    assert_eq!(
        (
            x.as_str(),
            y.as_str(),
            width.as_str(),
            height.as_str(),
            color.as_str()
        ),
        ("001.00", "-0", "{width}", "040.00", "#aAbBcC")
    );
    assert_eq!(
        ElementDefinition::from_xml(&data.to_xml().unwrap()).unwrap(),
        data
    );
}

#[test]
fn text_attribute_escapes_xml_and_preserves_whitespace() {
    let value = "\"<&\n\r\t{name} $$".to_owned();
    let data = ElementDefinition {
        name: "Example".into(),
        inputs: vec![Input {
            name: "fonts".into(),
            kind: InputType::Fonts,
        }],
        definitions: vec![drawing(CanvasExpr::Text {
            content: vec![organon_format::TextContent::string(value)],
            wrap_width: None,
            style: TextStyle {
                fonts: "{fonts}".into(),
                font_size: "20".into(),
                foreground: "#ffffff".into(),
            },
        })],
    };
    assert_eq!(
        ElementDefinition::from_xml(&data.to_xml().unwrap()).unwrap(),
        data
    );
}

#[test]
fn malformed_structure_and_unserializable_data_are_rejected() {
    for source in [
        "<element name=\"Example\" version=\"2\"><let name=\"drawing\" export=\"true\"><ref name=\"x\"/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><ref typo=\"x\"/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><unknown/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><Translate x=\"0\" y=\"0\"/></let></element>",
        "<element name=\"Example\" version=\"1\" xmlns=\"urn:other\"><let name=\"drawing\" export=\"true\"><ref name=\"x\"/></let></element>",
        "<element name=\"Example\" version=\"1\"/>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><ref/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><ref name=\"x\"/><ref name=\"y\"/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><ref name=\"x\"/>unexpected text</let></element>",
        "<element name=\"Example\" version=\"1\"><input name=\"x\" type=\"unknown\"/><let name=\"drawing\" export=\"true\"><ref name=\"x\"/></let></element>",
        "<element name=\"Example\" version=\"1\"><let name=\"drawing\" export=\"true\"><Text content=\"x\"/></let></element>",
    ] {
        assert!(ElementDefinition::from_xml(source).is_err(), "{source}");
    }
    for invalid in ["\0", "\u{1}", "\u{fffe}"] {
        let data = ElementDefinition {
            name: "Example".into(),
            inputs: vec![],
            definitions: vec![drawing(CanvasExpr::Reference(invalid.into()))],
        };
        assert!(data.to_xml().is_err());
    }
    let mut node = CanvasExpr::Reference("{x}".into());
    for _ in 0..130 {
        node = CanvasExpr::Translate {
            x: "0".into(),
            y: "0".into(),
            content: Box::new(node),
        };
    }
    assert!(
        ElementDefinition {
            name: "Example".into(),
            inputs: vec![],
            definitions: vec![drawing(node)]
        }
        .to_xml()
        .is_err()
    );
}

#[test]
fn text_requires_style_attributes_and_rejects_style_children() {
    let valid = r##"<element name="Example" version="1"><input name="fonts" type="Fonts"/>
  <let name="drawing" export="true">
        <Text fonts="{fonts}" font_size="20" foreground="#ffffff" wrap_width="100">
          <s t="A"/>
        </Text>
  </let>
</element>"##;
    let parsed = ElementDefinition::from_xml(valid).unwrap();
    let saved = parsed.to_xml().unwrap();
    assert!(saved.contains("<Text fonts="));
    assert!(!saved.contains("<TextStyle"));
    assert_eq!(ElementDefinition::from_xml(&saved).unwrap(), parsed);
    for xml in [
        valid.replace("font_size", "fontsize"),
        valid.replace("wrap_width", "width"),
        valid.replace("font_size=\"20\"", ""),
        valid.replace("foreground=\"#ffffff\"", ""),
        valid.replace("t=\"A\"", ""),
        valid.replace("fonts=\"{fonts}\"", ""),
        valid.replace(
            "</Text>",
            "<TextStyle fonts=\"{fonts}\" font_size=\"20\" foreground=\"#ffffff\"/></Text>",
        ),
    ] {
        assert!(ElementDefinition::from_xml(&xml).is_err(), "{xml}");
    }
}

#[test]
fn parser_and_serializer_enforce_the_same_nesting_limit() {
    for depth in [128, 129] {
        let xml = format!(
            "<element name=\"Example\" version=\"1\"><input name=\"c\" type=\"Canvas\"/><let name=\"drawing\" export=\"true\">{}<ref name=\"c\"/>{}</let></element>",
            "<Translate x=\"0\" y=\"0\">".repeat(depth),
            "</Translate>".repeat(depth)
        );
        if depth == 128 {
            let data = ElementDefinition::from_xml(&xml).unwrap();
            assert_eq!(
                ElementDefinition::from_xml(&data.to_xml().unwrap()).unwrap(),
                data
            );
        } else {
            assert!(
                ElementDefinition::from_xml(&xml)
                    .unwrap_err()
                    .to_string()
                    .contains("nesting")
            );
        }
    }
}

fn drawing(value: CanvasExpr) -> organon_format::Let {
    organon_format::Let {
        name: "drawing".into(),
        export: true,
        inline: false,
        value: ValueExpr::Canvas(value),
    }
}
