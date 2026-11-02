use organon_app::Document;
use organon_core::syntax::{Definition, ElementMember, Item, ValueExpression};

#[test]
fn attribute_edits_preserve_surrounding_source() {
    let original = "<organon version='1'>\r\n  <!-- 編集 -->\r\n  <element name='View'><let name='value'><ThirdParty z='3' text='old' a='1'/></let></element>\r\n</organon>";
    let mut document = Document::new(original);
    document
        .replace_attribute(&[0, 0, 0], "text", "日本語 '\"<&\n\r\t")
        .unwrap();
    assert_eq!(
        document.source(),
        original.replace(
            "text='old'",
            "text='日本語 &apos;&quot;&lt;&amp;&#10;&#13;&#9;'"
        )
    );
    let tree = document.xml().unwrap();
    let node = tree
        .descendants()
        .find(|n| n.has_tag_name("ThirdParty"))
        .unwrap();
    assert_eq!(
        node.attributes().map(|a| a.name()).collect::<Vec<_>>(),
        ["z", "text", "a"]
    );
    assert_eq!(node.attribute("text"), Some("日本語 '\"<&\n\r\t"));
    let before = document.source().to_owned();
    assert!(
        document
            .replace_attribute(&[0, 0, 0], "text", "\0")
            .is_err()
    );
    assert_eq!(document.source(), before);
}

#[test]
fn editor_retains_incomplete_or_unknown_input() {
    let mut document = Document::new("<organon><unfinished/></organon>");
    assert!(document.xml().is_ok());
    assert!(document.definitions().is_err());
    document.replace_source("<organon");
    assert!(document.xml().is_err());
    assert_eq!(document.source(), "<organon");
    document.replace_source("<organon version=\"1\"/>");
    assert!(document.definitions().is_ok());
}

#[test]
fn generation_is_an_app_policy_that_preserves_definition_content() {
    let definitions =
        organon_core::xml::parse(include_str!("../../../crates/organon-theoria/organon.xml"))
            .unwrap();
    let saved = organon_app::xml::write(&definitions).unwrap();
    assert_eq!(organon_core::xml::parse(&saved).unwrap(), definitions);
    let mut definitions = organon_core::xml::parse(r#"<organon version="1">
      <element name="View"><let name="value"><ThirdParty number="024.00"><s t="こんにちは {name}&#10;&lt;&amp;&quot;"/></ThirdParty></let></element>
    </organon>"#).unwrap();
    let Item::Definition(Definition::Element(element)) = &mut definitions.items[0] else {
        panic!()
    };
    let ElementMember::Let(binding) = &mut element.members[0] else {
        panic!()
    };
    let ValueExpression::Construct(construction) = &mut binding.value else {
        panic!()
    };
    construction
        .arguments
        .insert("number".parse().unwrap(), "048.00".into());
    let saved = organon_app::xml::write(&definitions).unwrap();
    assert!(saved.contains("048.00"));
    assert_eq!(organon_core::xml::parse(&saved).unwrap(), definitions);
    let typed =
        organon_core::xml::parse(include_str!("fixtures/interface_attributes.xml")).unwrap();
    assert_eq!(
        organon_core::xml::parse(&organon_app::xml::write(&typed).unwrap()).unwrap(),
        typed
    );
}

#[test]
fn branches_round_trip_with_ordered_members_and_escaped_results() {
    let definitions = organon_core::xml::parse(
        r#"<organon version="1">
      <declare-predicate name="Valid">
        <error type="Fault"/>
        <input name="text" type="String"/>
      </declare-predicate>
      <predicate name="Tolerant">
        <input name="label" type="String">
          <error type="Fault"/>
        </input>
        <Valid text="{label}"/>
      </predicate>
      <element name="View">
        <let name="value">
          <try type="String" result="{selected}">
            <let name="selected">
              <select type="String">
                <case result="&lt;&amp;"><not><Valid text="a"/></not></case>
                <case throw="Fault" result="bad"/>
              </select>
            </let>
            <catch type="Fault" name="problem" result="fallback"/>
            <unreachable type="OtherFault"/>
            <unreachable/>
          </try>
        </let>
      </element>
    </organon>"#,
    )
    .unwrap();
    let saved = organon_app::xml::write(&definitions).unwrap();
    assert_eq!(organon_core::xml::parse(&saved).unwrap(), definitions);
}

#[test]
fn edited_definitions_reach_preview_without_xml_serialization() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(format!("src/preview/unsaved-{}.xml", std::process::id()));
    assert!(!path.exists());
    let mut definitions = organon_core::xml::parse(
        r#"<organon version="1">
      <import from="organon-theoria"/>
      <element name="Edited">
        <bind text="text"><s t="unsaved"/></bind>
        <provide name="text" ref="text"/>
      </element>
      <export name="Edited"/>
    </organon>"#,
    )
    .unwrap();
    let Item::Definition(Definition::Element(element)) = &mut definitions.items[1] else {
        panic!()
    };
    let ElementMember::Bind(binding) = &mut element.members[0] else {
        panic!()
    };
    let ValueExpression::Construct(construction) = &mut binding.value else {
        panic!()
    };
    // Valid attribute data for this provider, but not representable in XML.
    construction
        .arguments
        .insert("t".parse().unwrap(), "unsaved\0value".into());
    assert!(organon_app::xml::write(&definitions).is_err());
    let preview = organon_app::preview::LibraryPreview::from_definitions(
        &path,
        definitions,
        organon_app::preview::registry(),
    )
    .unwrap();
    let mut outputs = preview
        .registry
        .evaluate(
            &preview.definition.program,
            &preview.element,
            Default::default(),
        )
        .unwrap();
    let text = outputs
        .remove("text")
        .unwrap()
        .unwrap()
        .get::<String>()
        .unwrap();
    assert_eq!(text, "unsaved\0value");
    assert!(!path.exists());
}
