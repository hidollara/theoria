use organon_core::{
    format::DefinitionFile,
    semantic::{self, Argument, ExpressionKind},
    types::Type,
};

// XML-defined Elements have reference-only attributes. Native declarations defer
// attribute interpretation to their implementation; tests use the same distinction.
fn check(body: &str) -> Result<semantic::Program, organon_core::Error> {
    let source = format!(
        r#"<organon version="1">
  <declare name="Raw" tag="false"/>
  <element name="Data">
    <input name="raw" type="Raw"/>
    <provide name="raw" ref="raw"/>
  </element>
  {body}
</organon>"#
    );
    semantic::resolve(&DefinitionFile::from_xml(&source)?)
}

#[test]
fn unknown_vocabulary_roundtrips_but_requires_a_contract() {
    let source = r#"<organon version="1">
  <element name="Example">
    <let name="v">
      <Foreign a="{not interpreted}">
        <child/>
      </Foreign>
    </let>
    <provide name="v" ref="v"/>
  </element>
</organon>"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    assert!(
        semantic::resolve(&file)
            .unwrap_err()
            .to_string()
            .contains("unknown element")
    );
}

#[test]
fn dependencies_are_ordered_and_anonymous_bind_is_shared() {
    let p = check(
        r#"
  <declare name="Pair">
    <input name="data" type="Data"/>
    <output name="a" type="Data"/>
    <output name="b" type="Data"/>
  </declare>
  <element name="Example">
    <bind a="first" b="second">
      <Pair data="later"/>
    </bind>
    <let name="later">
      <ref name="source"/>
    </let>
    <input name="source" type="Data"/>
    <provide name="first" ref="first"/>
    <provide name="second" ref="second"/>
  </element>"#,
    )
    .unwrap();
    let e = p.elements.iter().find(|e| e.name == "Example").unwrap();
    assert_eq!(
        e.bindings
            .iter()
            .filter(|b| matches!(b.value.kind, ExpressionKind::Construct { .. }))
            .count(),
        1
    );
    assert_eq!(e.bindings[0].name, "later");
    assert_eq!(e.outputs[0].ty.kind, Type::parse("Data").unwrap());
}

#[test]
fn unknown_references_cycles_and_type_mismatches_are_errors() {
    for (body, expected) in [
        (
            r#"<let name="a"><ref name="missing"/></let>"#,
            "unknown binding",
        ),
        (
            r#"<let name="a"><ref name="b"/></let><let name="b"><ref name="a"/></let>"#,
            "cyclic binding",
        ),
        (
            r#"<let name="a"><Consumer data="self_value"/></let><let name="self_value"><Consumer data="source"/></let>"#,
            "expected Data, got Consumer",
        ),
    ] {
        let source = format!(
            r#"
  <declare name="Consumer">
    <input name="data" type="Data"/>
    <output name="value" type="Data"/>
  </declare>
  <element name="Example">
    <input name="source" type="Data"/>
    {body}
    <provide name="a" ref="a"/>
  </element>"#
        );
        let error = check(&source).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn interfaces_require_explicit_complete_implementations() {
    let source = r#"
  <interface name="Contract">
    <output name="value" type="Data"/>
  </interface>
  <element name="Item">
    <input name="value" type="Data"/>
    <impl interface="Contract">
      <provide name="value" ref="value"/>
    </impl>
  </element>
  <declare name="Consumer">
    <input name="item" type="Contract"/>
    <output name="value" type="Data"/>
  </declare>
  <element name="Root">
    <input name="data" type="Data"/>
    <let name="item"><Item value="data"/></let>
    <let name="result"><Consumer item="item"/></let>
    <provide name="result" ref="result"/>
  </element>"#;
    assert!(check(source).is_ok());
    assert!(check(&source.replace("<provide name=\"value\" ref=\"value\"/>", "")).is_err());
}

#[test]
fn element_and_interface_outputs_map_bindings_in_separate_namespaces() {
    let source = r#"
  <interface name="Contract">
    <output name="result" type="Data"/>
  </interface>
  <element name="Item">
    <provide name="result" ref="local"/>
    <provide name="alias" ref="local"/>
    <impl interface="Contract">
      <provide name="result" ref="local"/>
    </impl>
    <input name="local" type="Data"/>
  </element>"#;
    let program = check(source).unwrap();
    let item = program.elements.iter().find(|e| e.name == "Item").unwrap();
    assert_eq!(item.outputs.len(), 2);
    assert_eq!(item.outputs[0].name, "result");
    assert_eq!(item.outputs[1].name, "alias");
    for output in item.outputs.iter().chain(&item.implementations[0].outputs) {
        assert_eq!(output.binding, "local");
        assert_eq!(output.ty.kind, Type::parse("Data").unwrap());
    }
    assert!(
        check(&source.replace("name=\"alias\"", "name=\"result\""))
            .unwrap_err()
            .to_string()
            .contains("duplicate output")
    );
    assert!(
        check(&source.replace("ref=\"local\"", "ref=\"missing\""))
            .unwrap_err()
            .to_string()
            .contains("unknown binding")
    );
}

#[test]
fn native_attribute_text_remains_uninterpreted_with_its_expected_type() {
    // Even malformed template-looking text belongs to the native parser, not core.
    for text in ["42", "unknown", "{unclosed", "prefix {other}"] {
        let source = format!(
            r#"<organon version="1">
  <declare name="Number" tag="false"/>
  <declare name="Item">
    <input name="value" type="Number"/>
    <output name="value" type="Number"/>
  </declare>
  <element name="Example">
    <let name="x"><Item value="{text}"/></let>
    <provide name="x" ref="x"/>
  </element>
</organon>"#
        );
        let program = semantic::resolve(&DefinitionFile::from_xml(&source).unwrap()).unwrap();
        let ExpressionKind::Construct { arguments, .. } =
            &program.elements[0].bindings[0].value.kind
        else {
            panic!("expected construction")
        };
        let Argument::Attribute {
            text: actual,
            expected,
        } = &arguments["value"]
        else {
            panic!("expected deferred attribute")
        };
        assert_eq!(actual, text);
        assert_eq!(expected.kind, Type::parse("Number").unwrap());
    }
}

#[test]
fn types_must_be_declared() {
    let file = DefinitionFile::from_xml(
        r#"<organon version="1">
  <declare name="Item"><input name="value" type="Undeclared"/></declare>
</organon>"#,
    )
    .unwrap();
    assert!(
        semantic::resolve(&file)
            .unwrap_err()
            .to_string()
            .contains("unknown type")
    );
}
