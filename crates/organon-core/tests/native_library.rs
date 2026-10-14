#![cfg(feature = "xml")]

use organon_core::semantic;
const LIBRARY: &str = r#"<organon version="1">
<declare-element tag="false" name="Number"/>
<interface name="Numeric"><output name="value" type="Number"/></interface>
<declare-element name="Sum" child="Numeric"><output name="value" type="Number"/></declare-element>
<declare-element name="Literal"><input name="value" type="Number"/><output name="value" type="Number"/><implements interface="Numeric"/></declare-element>
<element name="Example"><let name="total"><Sum><Literal value="2"/></Sum></let><provide name="total" ref="total"/></element>
</organon>"#;
#[test]
fn native_contracts_are_read_and_validate_children() {
    let file = organon_core::xml::parse(LIBRARY).unwrap();
    file.validate().unwrap();
    let program = organon_core::resolver::resolve_module(&file).unwrap();
    assert_eq!(
        program.contracts["Sum"].kind,
        semantic::DefinitionKind::DeclareElement { tag: true }
    );
    assert_eq!(
        program.contracts["Number"].kind,
        semantic::DefinitionKind::DeclareElement { tag: false }
    );
    assert_eq!(
        program.contracts["Numeric"].kind,
        semantic::DefinitionKind::Interface
    );
    assert_eq!(
        program.contracts["Example"].kind,
        semantic::DefinitionKind::Element
    );
    assert!(program.contracts["Sum"].constructible());
    assert!(!program.contracts["Number"].constructible());
    assert!(!program.contracts["Numeric"].constructible());
    let children = program.contracts["Sum"].children.as_ref().unwrap();
    assert_eq!(children.kind.name(), "Numeric");
    assert_eq!((children.minimum, children.maximum), (1, Some(1)));
    for source in [
        LIBRARY.replace("<Literal value=\"2\"/>", ""),
        LIBRARY.replace(
            "<Literal value=\"2\"/>",
            "<Literal value=\"1\"/><Literal value=\"2\"/><Literal value=\"3\"/>",
        ),
    ] {
        assert!(
            organon_core::resolver::resolve_module(&organon_core::xml::parse(&source).unwrap())
                .unwrap_err()
                .to_string()
                .contains("child count")
        );
    }
}
#[test]
fn declarations_reject_xml_implementations_and_attribute_interpretation_is_deferred() {
    for body in [
        r#"<impl interface="Numeric"><provide name="value" ref="value"/></impl>"#,
        r#"<provide name="value" ref="value"/>"#,
    ] {
        let source = LIBRARY.replace(r#"<implements interface="Numeric"/>"#, body);
        assert!(organon_core::xml::parse(&source).is_err());
    }
    let source = LIBRARY
        .replace(
            "<let name=\"total\">",
            "<input name=\"number\" type=\"Number\"/><let name=\"total\">",
        )
        .replace("value=\"2\"", "value=\"1{number}\"");
    assert!(
        organon_core::resolver::resolve_module(&organon_core::xml::parse(&source).unwrap()).is_ok()
    );
}

#[test]
fn element_outputs_are_explicit_and_children_require_the_contract() {
    let no_contract = LIBRARY.replace("<implements interface=\"Numeric\"/>", "");
    assert!(
        organon_core::resolver::resolve_module(&organon_core::xml::parse(&no_contract).unwrap())
            .is_err()
    );
    let selection = LIBRARY
        .replace("<let name=\"total\">", "<bind value=\"total\">")
        .replace("</Sum></let>", "</Sum></bind>");
    organon_core::resolver::resolve_module(&organon_core::xml::parse(&selection).unwrap()).unwrap();
    assert!(
        organon_core::xml::parse(
            r#"<organon version="1"><primitive name="Old" returns="Number"/></organon>"#
        )
        .is_err()
    );
}

#[test]
fn ports_declare_their_allowed_failure_types() {
    let source = r#"<organon version="1">
      <declare-element tag="false" name="Number"/>
      <declare-element tag="false" name="Reason"/>
      <declare-element name="Native">
        <input name="value" type="Number" optional="true"><error type="Reason"/></input>
        <output name="value" type="Number"><error type="Reason"/></output>
        <output name="plain" type="Number"/>
      </declare-element>
    </organon>"#;
    let program =
        organon_core::resolver::resolve_module(&organon_core::xml::parse(source).unwrap()).unwrap();
    let native = &program.contracts["Native"];
    assert_eq!(
        native.inputs[0].ty.errors,
        native.outputs[0].ty.declared().unwrap().errors
    );
    assert_eq!(
        native.outputs[0].ty.declared().unwrap().errors[0].name(),
        "Reason"
    );
    assert!(!native.outputs[1].ty.declared().unwrap().fallible());
    assert!(
        organon_core::resolver::resolve_module(
            &organon_core::xml::parse(&source.replace("type=\"Reason\"", "type=\"Absent\""))
                .unwrap()
        )
        .is_err()
    );
    assert!(
        organon_core::xml::parse(
            &source.replace("type=\"Number\"", "type=\"Number\" fallible=\"true\"")
        )
        .is_err()
    );
}

#[test]
fn bindings_and_outputs_do_not_accept_optimization_policy_attributes() {
    for source in [
        r#"<organon version="1"><element name="Example"><let name="value" inline="true"><ref name="input"/></let></element></organon>"#,
        r#"<organon version="1"><declare-element name="Example"><output name="value" type="Number" inline="false"/></declare-element></organon>"#,
    ] {
        let error = organon_core::xml::parse(source).unwrap_err();
        assert!(error.to_string().contains("inline"), "{error}");
    }
}

#[test]
fn declaration_controls_tag_construction_but_not_attribute_parsing() {
    let source = r#"<organon version="1"><declare-element name="Number" tag="false"/></organon>"#;
    let file = organon_core::xml::parse(source).unwrap();
    file.validate().unwrap();
    for attribute in ["parse=\"true\"", "text=\"literal\"", "tag=\"yes\""] {
        assert!(organon_core::xml::parse(&source.replace("tag=\"false\"", attribute)).is_err());
    }
    assert!(organon_core::xml::parse(&source.replace("declare", "type")).is_err());
    let source = source.replace("</organon>", r#"<element name="Invalid"><let name="n"><Number/></let><provide name="n" ref="n"/></element></organon>"#);
    assert!(
        organon_core::resolver::resolve_module(&organon_core::xml::parse(&source).unwrap())
            .is_err()
    );
}

#[test]
fn declaration_member_order_is_retained() {
    let file = organon_core::xml::parse(r#"<organon version="1"><declare-element name="Number" tag="false"><output name="n" type="Number"/><input name="x" type="Number"/></declare-element></organon>"#).unwrap();
    let organon_core::syntax::Item::Definition(organon_core::syntax::Definition::DeclareElement(
        value,
    )) = &file.items[0]
    else {
        panic!()
    };
    assert!(matches!(
        value.members.as_slice(),
        [
            organon_core::syntax::DeclareElementMember::Output(_),
            organon_core::syntax::DeclareElementMember::Input(_)
        ]
    ));
}

#[test]
fn nonconstructible_declaration_is_not_an_interface() {
    let file = organon_core::xml::parse(r#"<organon version="1"><declare-element name="Value" tag="false"/><element name="Invalid"><impl interface="Value"/></element></organon>"#).unwrap();
    assert!(organon_core::resolver::resolve_module(&file).is_err());
}

#[test]
fn value_conversions_are_not_xml_declarations() {
    assert!(organon_core::xml::parse(r#"<organon version="1"><declare-element name="Field" tag="false"><accept type="Color"/></declare-element></organon>"#).is_err());
}
