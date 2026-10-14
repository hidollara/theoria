#![cfg(feature = "xml")]

use organon_core::{
    semantic::{Child, ExpressionKind},
    syntax::ChildCount,
};

const SOURCE: &str = r#"<organon version="1">
  <declare-element name="Item"/>
  <declare-element name="Group" child="Item*"/>
  <element name="Wrap" child="Item">
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
    let program = organon_core::resolver::resolve_module(&file).unwrap();
    let wrap = program.bodies.iter().find(|e| e.name == "Wrap").unwrap();
    let contract = program.contracts[&wrap.name].children.as_ref().unwrap();
    assert_eq!((contract.minimum, contract.maximum), (1, Some(1)));
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
fn child_suffixes_define_counts_for_both_definition_forms() {
    for tag in ["element", "declare-element"] {
        let body = if tag == "element" {
            r#"<let name="item"><Item/></let><provide name="item" ref="item"/>"#
        } else {
            ""
        };
        for (suffix, expected) in [
            ("", ChildCount::One),
            ("?", ChildCount::Optional),
            ("*", ChildCount::Many),
        ] {
            for count in 0..=3 {
                let items = "<Item/>".repeat(count);
                let xml = format!(
                    r#"<organon version='1'>
                  <declare-element name="Item"/>
                  <{tag} name="Target" child="Item{suffix}">{body}</{tag}>
                  <element name="Scene"><let name="target"><Target>{items}</Target></let><provide name="target" ref="target"/></element>
                </organon>"#
                );
                let module = organon_core::xml::parse(&xml).unwrap();
                let resolved = organon_core::resolver::resolve_module(&module);
                let valid = match expected {
                    ChildCount::One => count == 1,
                    ChildCount::Optional => count <= 1,
                    ChildCount::Many => true,
                };
                assert_eq!(resolved.is_ok(), valid, "{tag} {suffix:?} {count}");
                if let Ok(program) = resolved {
                    let children = program.contracts["Target"].children.as_ref().unwrap();
                    assert_eq!(children.kind.name(), "Item");
                    assert_eq!(
                        (children.minimum, children.maximum),
                        match expected {
                            ChildCount::One => (1, Some(1)),
                            ChildCount::Optional => (0, Some(1)),
                            ChildCount::Many => (0, None),
                        }
                    );
                }
            }
        }
    }
}

#[test]
fn syntax_errors_and_semantic_errors_are_separate() {
    for attributes in [
        "min-children=\"1\"",
        "max-children=\"1\"",
        "child=\"Item\" min-children=\"1\"",
        "child=\"Item\" max-children=\"1\"",
        "child=\"\"",
        "child=\"?\"",
        "child=\"*\"",
        "child=\"Item+\"",
        "child=\"Item??\"",
        "child=\"Item**\"",
        "child=\"Item?*\"",
        "child=\"Item*?\"",
        "child=\" Item\"",
        "child=\"Item ?\"",
        "child=\"[Item]\"",
    ] {
        for tag in ["declare-element", "element"] {
            let xml = format!("<organon version='1'><{tag} name=\"X\" {attributes}/></organon>");
            assert!(organon_core::xml::parse(&xml).is_err(), "{xml}");
        }
    }
    for child in ["<children type=\"Item\"/>", "<children><Item/></children>"] {
        assert!(organon_core::xml::parse(&SOURCE.replace("<children/>", child)).is_err());
    }
    for source in [
        SOURCE.replace("name=\"Wrap\" child=\"Item\"", "name=\"Wrap\""),
        SOURCE
            .replace("<Group>", "<Item>")
            .replace("</Group>", "</Item>"),
        SOURCE.replace("<Wrap><Item/></Wrap>", "<Wrap/>"),
        SOURCE.replace("<Wrap><Item/></Wrap>", "<Wrap><Group/></Wrap>"),
        SOURCE.replace(
            "<let name=\"group\">",
            "<let name=\"invalid\"><children/></let><let name=\"group\">",
        ),
    ] {
        let file = organon_core::xml::parse(&source).unwrap();
        assert!(
            organon_core::resolver::resolve_module(&file).is_err(),
            "{source}"
        );
    }
}

#[test]
fn incoming_counts_must_fit_even_when_splices_are_combined() {
    for receiving in ["", "?", "*"] {
        for incoming in ["", "?", "*"] {
            for extra in ["", "<Item/>", "<children/>"] {
                let xml = format!(
                    r#"<organon version='1'>
                  <declare-element name="Item"/>
                  <declare-element name="Target" child="Item{receiving}"/>
                  <element name="Wrap" child="Item{incoming}">
                    <let name="target"><Target><children/>{extra}</Target></let>
                    <provide name="target" ref="target"/>
                  </element>
                </organon>"#
                );
                let valid = receiving == "*"
                    || (extra.is_empty()
                        && match receiving {
                            "" => incoming.is_empty(),
                            "?" => incoming != "*",
                            _ => unreachable!(),
                        });
                let module = organon_core::xml::parse(&xml).unwrap();
                assert_eq!(
                    organon_core::resolver::resolve_module(&module).is_ok(),
                    valid,
                    "{receiving:?} <- {incoming:?} + {extra}"
                );
            }
        }
    }
}
