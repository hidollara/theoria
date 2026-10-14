#![cfg(feature = "xml")]

use organon_core::semantic::{self, Child, ExpressionKind};

const SOURCE: &str = r#"<organon version="1">
  <declare-element name="Item"/>
  <declare-element name="Group" child="Item"/>
  <element name="Wrap" child="Item" min-children="1" max-children="2">
    <let name="group">
      <Group>
        <Item/>
        <children/>
        <Item/>
        <children/>
      </Group>
    </let>
    <provide name="group" ref="group"/>
  </element>
  <element name="Scene">
    <let name="wrapped"><Wrap><Item/></Wrap></let>
    <provide name="wrapped" ref="wrapped"/>
  </element>
</organon>"#;

#[test]
fn contracts_are_read_and_splices_keep_their_position() {
    let file = organon_core::xml::parse(SOURCE).unwrap();
    file.validate().unwrap();
    let program = semantic::resolve(&file).unwrap();
    let wrap = program.bodies.iter().find(|e| e.name == "Wrap").unwrap();
    let contract = program.contracts[&wrap.name].children.as_ref().unwrap();
    assert_eq!((contract.minimum, contract.maximum), (1, Some(2)));
    let ExpressionKind::Construct { children, .. } = &wrap.bindings[0].value.kind else {
        panic!()
    };
    assert!(matches!(
        children.as_slice(),
        [
            Child::Value(_),
            Child::Incoming { .. },
            Child::Value(_),
            Child::Incoming { .. }
        ]
    ));
}

#[test]
fn syntax_errors_and_semantic_errors_are_separate() {
    for attributes in [
        "min-children=\"1\"",
        "max-children=\"1\"",
        "child=\"Item\" min-children=\"2\" max-children=\"1\"",
        "child=\"Item\" min-children=\"-1\"",
    ] {
        for tag in ["declare", "element"] {
            let xml = format!("<organon version=\"1\"><{tag} name=\"X\" {attributes}/></organon>");
            assert!(organon_core::xml::parse(&xml).is_err(), "{xml}");
        }
    }
    for child in ["<children type=\"Item\"/>", "<children><Item/></children>"] {
        assert!(organon_core::xml::parse(&SOURCE.replace("<children/>", child)).is_err());
    }
    assert!(
        organon_core::xml::parse(
            r#"<organon version="1"><declare-element name="X"><children type="Item"/></declare-element></organon>"#
        )
        .is_err()
    );
    for source in [
        SOURCE.replace("child=\"Item\" min-children=\"1\" max-children=\"2\"", ""),
        SOURCE
            .replace("<Group>", "<Item>")
            .replace("</Group>", "</Item>"),
        SOURCE.replace("<Wrap><Item/></Wrap>", "<Wrap/>"),
        SOURCE.replace("<Wrap><Item/></Wrap>", "<Wrap><Group/></Wrap>"),
        SOURCE.replace(
            "<Group>\n        <Item/>",
            "<Group>\n        <ref name=\"missing\"/>",
        ),
        SOURCE.replace(
            "<let name=\"group\">",
            "<let name=\"invalid\"><children/></let><let name=\"group\">",
        ),
    ] {
        let file = organon_core::xml::parse(&source).unwrap();
        assert!(semantic::resolve(&file).is_err(), "{source}");
    }
}

#[test]
fn incoming_count_range_must_fit_the_receiving_contract() {
    // Two ordinary children plus two incoming sequences: 4..=6.
    for (limits, valid) in [
        ("min-children=\"4\" max-children=\"6\"", true),
        ("min-children=\"5\"", false),
        ("max-children=\"5\"", false),
    ] {
        let source = SOURCE.replace(
            "name=\"Group\" child=\"Item\"",
            &format!("name=\"Group\" child=\"Item\" {limits}"),
        );
        let file = organon_core::xml::parse(&source).unwrap();
        assert_eq!(semantic::resolve(&file).is_ok(), valid);
    }
    let source = SOURCE
        .replace(" min-children=\"1\" max-children=\"2\"", "")
        .replace("<Wrap><Item/></Wrap>", "<Wrap/>");
    let program = semantic::resolve(&organon_core::xml::parse(&source).unwrap()).unwrap();
    let contract = program.contracts["Wrap"].children.as_ref().unwrap();
    assert_eq!((contract.minimum, contract.maximum), (0, None));
}
