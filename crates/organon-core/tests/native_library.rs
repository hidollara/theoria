use organon_core::{format::DefinitionFile, semantic};
const LIBRARY: &str = r#"<organon version="1">
<declare tag="false" name="Number"/>
<interface name="Numeric"><output name="value" type="Number"/></interface>
<declare name="Sum"><children type="Numeric" min="1" max="2"/><output name="value" type="Number"/></declare>
<declare name="Literal"><input name="value" type="Number"/><output name="value" type="Number"/><impl interface="Numeric"><provide name="value" ref="value"/></impl></declare>
<element name="Example"><let name="total"><Sum><Literal value="2"/></Sum></let><provide name="total" ref="total"/></element>
</organon>"#;
#[test]
fn native_contracts_roundtrip_and_validate_children() {
    let file = DefinitionFile::from_xml(LIBRARY).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    let program = semantic::resolve(&file).unwrap();
    assert_eq!(
        program.signatures["Sum"].kind,
        semantic::DefinitionKind::External { tag: true }
    );
    assert_eq!(
        program.signatures["Number"].kind,
        semantic::DefinitionKind::External { tag: false }
    );
    assert_eq!(
        program.signatures["Numeric"].kind,
        semantic::DefinitionKind::Interface
    );
    assert_eq!(
        program.signatures["Example"].kind,
        semantic::DefinitionKind::Element
    );
    assert!(program.signatures["Sum"].constructible());
    assert!(!program.signatures["Number"].constructible());
    assert!(!program.signatures["Numeric"].constructible());
    let children = program.signatures["Sum"].children.as_ref().unwrap();
    assert_eq!(children.kind.spelling(), "Numeric");
    assert_eq!((children.minimum, children.maximum), (1, Some(2)));
    for source in [
        LIBRARY.replace("<Literal value=\"2\"/>", ""),
        LIBRARY.replace(
            "<Literal value=\"2\"/>",
            "<Literal value=\"1\"/><Literal value=\"2\"/><Literal value=\"3\"/>",
        ),
    ] {
        assert!(
            semantic::resolve(&DefinitionFile::from_xml(&source).unwrap())
                .unwrap_err()
                .to_string()
                .contains("child count")
        );
    }
}
#[test]
fn output_mappings_are_checked_and_attribute_interpretation_is_deferred() {
    let source = LIBRARY.replace("ref=\"value\"", "ref=\"missing\"");
    assert!(semantic::resolve(&DefinitionFile::from_xml(&source).unwrap()).is_err());
    let source = LIBRARY
        .replace(
            "<let name=\"total\">",
            "<input name=\"number\" type=\"Number\"/><let name=\"total\">",
        )
        .replace("value=\"2\"", "value=\"1{number}\"");
    assert!(semantic::resolve(&DefinitionFile::from_xml(&source).unwrap()).is_ok());
}

#[test]
fn element_outputs_are_explicit_and_children_require_the_contract() {
    let no_contract = LIBRARY.replace(
        "<impl interface=\"Numeric\"><provide name=\"value\" ref=\"value\"/></impl>",
        "",
    );
    assert!(semantic::resolve(&DefinitionFile::from_xml(&no_contract).unwrap()).is_err());
    let selection = LIBRARY
        .replace("<let name=\"total\">", "<bind value=\"total\">")
        .replace("</Sum></let>", "</Sum></bind>");
    semantic::resolve(&DefinitionFile::from_xml(&selection).unwrap()).unwrap();
    assert!(
        DefinitionFile::from_xml(
            r#"<organon version="1"><primitive name="Old" returns="Number"/></organon>"#
        )
        .is_err()
    );
}

#[test]
fn fallibility_is_a_separate_port_attribute() {
    let source = r#"<organon version="1">
      <declare tag="false" name="Number"/>
      <interface name="Numeric"><output name="value" type="Number" fallible="true"/></interface>
      <declare name="Native">
        <input name="value" type="Number" fallible="true" optional="true"/>
        <output name="value" type="Number" fallible="true" inline="true"/>
        <output name="plain" type="Number" fallible="false"/>
      </declare>
    </organon>"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    let saved = file.to_xml().unwrap();
    assert_eq!(DefinitionFile::from_xml(&saved).unwrap(), file);
    assert!(!saved.contains("Result"));
    assert!(!saved.contains("fallible=\"false\""));
    let program = semantic::resolve(&file).unwrap();
    assert!(
        program.signatures["Native"]
            .optional_inputs
            .contains("value")
    );
    assert!(program.signatures["Native"].outputs[0].ty.fallible);
    assert!(!program.signatures["Native"].outputs[1].ty.fallible);
    for invalid in ["yes", "1", "{flag}"] {
        assert!(
            DefinitionFile::from_xml(
                &source.replace("fallible=\"true\"", &format!("fallible=\"{invalid}\""))
            )
            .is_err()
        );
    }
    assert!(
        DefinitionFile::from_xml(&source.replace(
            "type=\"Number\" fallible=\"true\"",
            "type=\"Result&lt;Number&gt;\""
        ))
        .is_err()
    );
}

#[test]
fn declaration_controls_tag_construction_but_not_attribute_parsing() {
    let source = r#"<organon version="1"><declare name="Number" tag="false"/></organon>"#;
    let file = DefinitionFile::from_xml(source).unwrap();
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    for attribute in ["parse=\"true\"", "text=\"literal\"", "tag=\"yes\""] {
        assert!(DefinitionFile::from_xml(&source.replace("tag=\"false\"", attribute)).is_err());
    }
    assert!(DefinitionFile::from_xml(&source.replace("declare", "type")).is_err());
    let source = source.replace("</organon>", r#"<element name="Invalid"><let name="n"><Number/></let><provide name="n" ref="n"/></element></organon>"#);
    assert!(semantic::resolve(&DefinitionFile::from_xml(&source).unwrap()).is_err());
}

#[test]
fn declaration_member_order_survives_roundtrip() {
    let file = DefinitionFile::from_xml(r#"<organon version="1"><declare name="Number" tag="false"><output name="n" type="Number"/><input name="x" type="Number"/></declare></organon>"#).unwrap();
    let saved = file.to_xml().unwrap();
    assert!(saved.find("<output").unwrap() < saved.find("<input").unwrap());
    assert_eq!(DefinitionFile::from_xml(&saved).unwrap(), file);
}

#[test]
fn nonconstructible_declaration_is_not_an_interface() {
    let file = DefinitionFile::from_xml(r#"<organon version="1"><declare name="Value" tag="false"/><element name="Invalid"><impl interface="Value"/></element></organon>"#).unwrap();
    assert!(semantic::resolve(&file).is_err());
}

#[test]
fn value_conversions_are_not_xml_declarations() {
    assert!(DefinitionFile::from_xml(r#"<organon version="1"><declare name="Field" tag="false"><accept type="Color"/></declare></organon>"#).is_err());
}
