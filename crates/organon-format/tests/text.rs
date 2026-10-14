use organon_format::{CanvasExpr, ElementDefinition, TextContent, ValueExpr};

fn xml(body: &str) -> String {
    format!(
        r##"<element name="Example" version="1">
  <input name="fonts" type="Fonts"/>
  <input name="label" type="String"/>
  <input name="size" type="f32"/>
  <let name="drawing" export="true">
    <Text fonts="{{fonts}}" font_size="20" foreground="#ffffff">
      {body}
    </Text>
  </let>
</element>"##
    )
}

#[test]
fn inline_structure_and_templates_survive_canonical_saving() {
    let definition = ElementDefinition::from_xml(&xml(r#"
      <s t="  A&#9;&quot;&amp;&lt;&#10;&#13;&#x202E;{label}&#x202C;  "/>
      <span font_size="{size}">
        <s t="{{literal}}" font_size="12"/>
        <br/>
        <span><s t="B"/></span>
      </span>
      <br/>
    "#))
    .unwrap();
    assert_eq!(
        definition.definitions[0].references().unwrap(),
        ["fonts", "label", "size"]
    );
    let ValueExpr::Canvas(CanvasExpr::Text { content, .. }) = &definition.definitions[0].value
    else {
        panic!()
    };
    assert_eq!(content.len(), 3);
    assert_eq!(
        content[0],
        TextContent::string("  A\t\"&<\n\r\u{202e}{label}\u{202c}  ")
    );
    let saved = definition.to_xml().unwrap();
    assert!(saved.contains("&#9;") && saved.contains("&#10;") && saved.contains("&#13;"));
    assert_eq!(ElementDefinition::from_xml(&saved).unwrap(), definition);
}

#[test]
fn only_s_accepts_templates_and_inline_leaves_cannot_have_children() {
    for body in [
        "literal text",
        "<s>literal text</s>",
        "<s/>",
        "<s text=\"x\"/>",
        "<s t=\"x\"><br/></s>",
        "<span t=\"x\"/>",
        "<span>literal text</span>",
        "<br t=\"x\"/>",
        "<br><s t=\"x\"/></br>",
        "<t value=\"x\"/>",
        "<run t=\"x\"/>",
        "<Line/>",
        "<s xmlns=\"urn:other\" t=\"x\"/>",
        "<span font_size=\"{broken\"/>",
        "<s t=\"x\" fonts=\"system-ui\"/>",
        "<s t=\"&#0;\"/>",
    ] {
        assert!(ElementDefinition::from_xml(&xml(body)).is_err(), "{body}");
    }
    assert!(ElementDefinition::from_xml(&xml("<s t=\"\"/><span/>")).is_ok());
    assert!(
        ElementDefinition::from_xml(&xml("<s t=\"x\"/>").replace("<Text ", "<Text content=\"x\" "))
            .is_err()
    );
}

#[test]
fn literal_attribute_whitespace_follows_xml_not_custom_dedent_rules() {
    let definition = ElementDefinition::from_xml(&xml("<s t=\"A\n\tB&#10;&#9;C\"/>")).unwrap();
    let ValueExpr::Canvas(CanvasExpr::Text { content, .. }) = &definition.definitions[0].value
    else {
        panic!()
    };
    assert_eq!(content, &vec![TextContent::string("A  B\n\tC")]);
}

#[test]
fn text_nesting_obeys_the_expression_depth_limit() {
    for depth in [127, 128] {
        let source = xml(&format!(
            "{}<s t=\"x\"/>{}",
            "<span>".repeat(depth),
            "</span>".repeat(depth)
        ));
        let parsed = ElementDefinition::from_xml(&source);
        assert_eq!(parsed.is_ok(), depth == 127);
        if let Ok(value) = parsed {
            assert!(value.to_xml().is_ok());
        }
    }
}
