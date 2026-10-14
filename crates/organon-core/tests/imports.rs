#![cfg(feature = "xml")]

use organon_core::loader;
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn directory() -> PathBuf {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!(
        "organon-core-imports-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
fn declaration(name: &str) -> String {
    format!(
        r#"<declare-element name="{name}"><output name="value" type="{name}"/></declare-element>"#
    )
}

#[test]
fn child_contracts_follow_definition_identity_across_files() {
    let root = directory();
    write(
        &root,
        "parts.xml",
        r#"
      <declare-element name="Item"/>
      <declare-element name="Group" child="Item"/>
      <element name="Wrapper" child="Item">
        <let name="group"><Group><children/></Group></let>
        <provide name="group" ref="group"/>
      </element>
      <export name="Wrapper"/>
    "#,
    );
    write(
        &root,
        "entry.xml",
        r#"
      <declare-element name="Item"/>
      <use from="./parts.xml" name="Wrapper" as="Frame"/>
      <export name="Frame"/>
      <export name="Item"/>
    "#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    let program = organon_core::semantic::resolve(&loaded.definitions).unwrap();
    let name = &loaded.public["Frame"];
    let child = program.contracts[name].children.as_ref().unwrap();
    let origin = &loaded.origins[child.kind.spelling()];
    assert_eq!(origin.name, "Item");
    assert_eq!(
        origin.source,
        fs::canonicalize(root.join("parts.xml")).unwrap()
    );
    assert_ne!(child.kind.spelling(), loaded.public["Item"]);
}
fn write(root: &std::path::Path, name: &str, content: &str) {
    fs::write(
        root.join(name),
        format!(r#"<organon version="1">{content}</organon>"#),
    )
    .unwrap();
}

#[test]
fn import_sources_and_selections_are_unambiguous() {
    for from in ["organon-theoria", "ui", "./parts.xml", "../parts.xml"] {
        let xml = format!(
            r#"<organon version="1"><import from="{from}"/><use from="{from}" name="Panel" as="Card"/></organon>"#
        );
        let file = organon_core::xml::parse(&xml).unwrap();
        file.validate().unwrap();
    }
    for from in [
        "/parts.xml",
        "C:/parts.xml",
        "parts.xml",
        "./bad\\parts.xml",
    ] {
        let xml = format!(r#"<organon version="1"><use from="{from}" name="Panel"/></organon>"#);
        assert!(organon_core::xml::parse(&xml).is_err(), "{from}");
    }
    assert!(
        organon_core::xml::parse(r#"<organon version="1"><import from="ui" as="UI"/></organon>"#)
            .is_err()
    );
    assert!(
        organon_core::xml::parse(r#"<organon version="1"><import path="./parts.xml"/></organon>"#)
            .is_err()
    );
    for import in [
        r#"<use from="ui"/>"#,
        r#"<use from="ui" as="Card"/>"#,
        r#"<import from="ui" name="Panel"/>"#,
        r#"<import from="ui"><use name="Panel"/></import>"#,
        r#"<use from="ui" name=""/>"#,
        r#"<use from="ui" name="*"/>"#,
        r#"<use name="Panel"/>"#,
        r#"<use from="ui" name="Panel"><use name="Other"/></use>"#,
    ] {
        let xml = format!(r#"<organon version="1">{import}</organon>"#);
        assert!(organon_core::xml::parse(&xml).is_err(), "{import}");
    }
    let file =
        organon_core::xml::parse(r#"<organon version="1"><use from="ui" name="Panel"/></organon>"#)
            .unwrap();
    assert_eq!(file.uses().next().unwrap().name, "Panel");
    file.validate().unwrap();
    assert!(organon_core::semantic::resolve(&file).is_err());
    assert!(organon_core::xml::parse_expression(r#"<use from="ui" name="Panel"/>"#).is_err());
}

#[test]
fn sources_supply_definitions_once_without_an_xml_roundtrip() {
    use organon_core::syntax::{Definition, ElementMember, Item, Module, ValueExpression};
    use std::collections::BTreeMap;
    struct InMemory(BTreeMap<String, Module>);
    impl loader::Sources for InMemory {
        type Id = String;
        fn resolve(
            &mut self,
            _: &String,
            import: &organon_core::syntax::ImportSource,
        ) -> Result<String, organon_core::Error> {
            Ok(import.as_str().trim_start_matches("./").into())
        }
        fn read(&mut self, id: &String) -> Result<Module, organon_core::Error> {
            self.0
                .remove(id)
                .ok_or_else(|| organon_core::Error("source read twice".into()))
        }
    }
    let entry = String::from("entry");
    let parts = String::from("parts.xml");
    let mut entry_content = organon_core::xml::parse(r#"<organon version="1">
      <use from="./parts.xml" name="Native" as="First"/>
      <use from="./parts.xml" name="Native" as="Second"/>
      <element name="View"><let name="value"><First text="old"/></let><provide name="value" ref="value"/></element>
      <export name="First"/><export name="Second"/><export name="View"/>
    </organon>"#).unwrap();
    let Item::Definition(Definition::Element(element)) = &mut entry_content.items[2] else {
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
        .insert("text".parse().unwrap(), "not XML\0".into());
    let parts_content = organon_core::xml::parse(
        r#"<organon version="1">
      <declare-element name="String" tag="false"/>
      <declare-element name="Native"><input name="text" type="String"/></declare-element>
      <export name="Native"/>
    </organon>"#,
    )
    .unwrap();
    let mut sources = InMemory(BTreeMap::from([
        (entry.clone(), entry_content.clone()),
        (parts.clone(), parts_content.clone()),
    ]));
    let loaded = loader::load(entry.clone(), &mut sources).unwrap();
    assert!(sources.0.is_empty());
    assert_eq!(loaded.source_order.len(), 2);
    assert_eq!(loaded.public["First"], loaded.public["Second"]);
    assert_eq!(loaded.sources[&entry], entry_content);
    assert_eq!(loaded.sources[&parts], parts_content);
    organon_core::semantic::resolve(&loaded.definitions).unwrap();
}

#[test]
fn import_and_use_share_definition_identity_without_implicit_publication() {
    let root = directory();
    write(
        &root,
        "a.xml",
        &(declaration("A")
            + &declaration("Ain")
            + &declaration("Other")
            + r#"<export name="A"/><export name="Other"/>"#),
    );
    write(
        &root,
        "b.xml",
        r#"<use from="./a.xml" name="A"/><export name="A"/>"#,
    );
    write(
        &root,
        "entry.xml",
        r#"
      <import from="./a.xml"/>
      <use from="./b.xml" name="A"/>
      <use from="./b.xml" name="A" as="AliasA"/>
      <export name="A"/>
      <export name="AliasA"/>
    "#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(loaded.source_order.len(), 3);
    assert_eq!(loaded.public.len(), 2);
    assert_eq!(loaded.public["A"], loaded.public["AliasA"]);
    assert_eq!(loaded.origins[&loaded.public["A"]].name, "A");
    assert_eq!(loaded.definitions.definitions().count(), 3);
    assert!(!loaded.public.contains_key("Other"));
    assert!(!loaded.public.contains_key("Ain"));
    write(
        &root,
        "entry.xml",
        r#"<import from="./a.xml"/><export name="Ain"/>"#,
    );
    assert!(
        load(root.join("entry.xml"), &mut FilesOnly)
            .unwrap_err()
            .to_string()
            .contains("unknown public definition")
    );
    write(
        &root,
        "entry.xml",
        r#"<import from="./a.xml"/><use from="./a.xml" name="Other" as="A"/>"#,
    );
    assert!(
        load(root.join("entry.xml"), &mut FilesOnly)
            .unwrap_err()
            .to_string()
            .contains("duplicate definition")
    );
}

#[test]
fn publication_and_aliases_are_file_scoped() {
    let root = directory();
    write(
        &root,
        "base.xml",
        &(declaration("Private") + &declaration("Panel") + r#"<export name="Panel"/>"#),
    );
    write(
        &root,
        "facade.xml",
        r#"<use from="./base.xml" name="Panel" as="Card"/><export name="Card" as="PublicCard"/>"#,
    );
    write(
        &root,
        "entry.xml",
        r#"<use from="./facade.xml" name="PublicCard" as="Widget"/><export name="Widget"/>"#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(
        loaded.public.keys().collect::<Vec<_>>(),
        [&"Widget".to_owned()]
    );
    assert!(loaded.origins.values().any(|id| id.name == "Private"));
    assert_eq!(loaded.origins["Widget"].name, "Panel");
    // An import-only facade does not accidentally publish its dependencies.
    write(
        &root,
        "facade.xml",
        r#"<use from="./base.xml" name="Panel"/>"#,
    );
    assert!(
        load(root.join("entry.xml"), &mut FilesOnly)
            .unwrap_err()
            .to_string()
            .contains("does not publish")
    );
}

#[test]
fn publication_is_explicit_and_independent_of_declaration_order() {
    let root = directory();
    write(&root, "base.xml", &declaration("Panel"));
    let loaded = load(root.join("base.xml"), &mut FilesOnly).unwrap();
    assert!(loaded.public.is_empty());
    assert_eq!(loaded.definitions.definitions().count(), 1);
    write(
        &root,
        "entry.xml",
        r#"<use from="./base.xml" name="Panel"/>"#,
    );
    assert!(
        load(root.join("entry.xml"), &mut FilesOnly)
            .unwrap_err()
            .to_string()
            .contains("does not publish")
    );
    write(
        &root,
        "base.xml",
        &(String::from(r#"<export name="Panel"/>"#) + &declaration("Panel")),
    );
    write(
        &root,
        "entry.xml",
        r#"<export name="Panel" as="Card"/><use from="./base.xml" name="Panel"/>"#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(loaded.public.len(), 1);
    assert_eq!(loaded.origins[&loaded.public["Card"]].name, "Panel");
    for (publication, message) in [
        (r#"<export name="Missing"/>"#, "unknown public definition"),
        (
            r#"<export name="Panel"/><export name="Panel"/>"#,
            "duplicate public name",
        ),
        (
            r#"<export name="Panel" as="Card"/><export name="Other" as="Card"/>"#,
            "duplicate public name",
        ),
    ] {
        write(
            &root,
            "entry.xml",
            &(declaration("Panel") + &declaration("Other") + publication),
        );
        assert!(
            load(root.join("entry.xml"), &mut FilesOnly)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
    }
}

#[test]
fn private_names_can_repeat_and_authored_order_is_retained() {
    let root = directory();
    for (file, name) in [("left.xml", "Left"), ("right.xml", "Right")] {
        write(
            &root,
            file,
            &(declaration("ZHelper")
                + &format!(
                    r#"<element name="{name}"><let name="helper"><ZHelper/></let><provide name="helper" ref="helper"/></element><export name="{name}"/>"#
                )),
        );
    }
    write(
        &root,
        "entry.xml",
        r#"<use from="./left.xml" name="Left"/><use from="./right.xml" name="Right"/><use from="./left.xml" name="Left"/>"#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(loaded.source_order.len(), 3);
    let names: Vec<_> = loaded
        .definitions
        .definitions()
        .map(|d| loaded.origins[d.name()].name.as_str())
        .collect();
    assert_eq!(names, ["ZHelper", "Left", "ZHelper", "Right"]);
    let helpers: Vec<_> = loaded
        .origins
        .iter()
        .filter(|(_, id)| id.name == "ZHelper")
        .map(|(name, _)| name)
        .collect();
    assert_eq!(helpers.len(), 2);
    assert_ne!(helpers[0], helpers[1]);
    // Loading again takes a fresh snapshot, instead of retaining process-global state.
    write(
        &root,
        "right.xml",
        &(declaration("Changed") + r#"<export name="Changed" as="Right"/>"#),
    );
    let changed = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert!(changed.origins.values().any(|id| id.name == "Changed"));
    assert!(!changed.origins.values().any(|id| id.name == "Right"));
}

#[test]
fn private_type_names_cannot_leak_from_another_file() {
    let root = directory();
    write(
        &root,
        "library.xml",
        r#"
      <declare-element name="Hidden" tag="false"/>
      <declare-element name="Public"><output name="value" type="Hidden"/></declare-element>
      <export name="Public"/>
    "#,
    );
    for declaration in [
        r#"<declare-element name="Consumer"><input name="value" type="Hidden"/></declare-element>"#,
        r#"<declare-element name="Consumer" child="Hidden"></declare-element>"#,
        r#"<interface name="Consumer"><output name="value" type="Hidden"/></interface>"#,
        r#"<element name="Consumer"><input name="value" type="Hidden"/><provide name="value" ref="value"/></element>"#,
    ] {
        write(
            &root,
            "entry.xml",
            &format!(r#"<use from="./library.xml" name="Public"/>{declaration}"#),
        );
        let error = load(root.join("entry.xml"), &mut FilesOnly).unwrap_err();
        assert!(
            error.to_string().contains("unknown definition \"Hidden\""),
            "{error}"
        );
    }
    write(
        &root,
        "entry.xml",
        r#"
      <use from="./library.xml" name="Public" as="Visible"/>
      <declare-element name="Consumer"><input name="value" type="Visible"/></declare-element>
    "#,
    );
    let loaded = load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    organon_core::semantic::resolve(&loaded.definitions).unwrap();
}

struct FilesOnly;
impl loader::Sources for FilesOnly {
    type Id = PathBuf;
    fn resolve(
        &mut self,
        from: &PathBuf,
        import: &organon_core::syntax::ImportSource,
    ) -> Result<PathBuf, organon_core::Error> {
        match import {
            organon_core::syntax::ImportSource::Relative(p) => {
                fs::canonicalize(from.parent().unwrap().join(p))
                    .map_err(|e| organon_core::Error(e.to_string()))
            }
            _ => Err(organon_core::Error("no dependency resolver".into())),
        }
    }
    fn read(
        &mut self,
        path: &PathBuf,
    ) -> Result<organon_core::syntax::Module, organon_core::Error> {
        organon_core::xml::parse(
            &fs::read_to_string(path).map_err(|e| organon_core::Error(e.to_string()))?,
        )
    }
}
fn load(
    path: PathBuf,
    sources: &mut FilesOnly,
) -> Result<loader::Loaded<PathBuf>, organon_core::Error> {
    loader::load(fs::canonicalize(path).unwrap(), sources)
}
