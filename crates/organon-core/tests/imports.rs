use organon_core::{
    format::DefinitionFile,
    loader::{self, FilesOnly},
};
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
    format!(r#"<declare name="{name}"><output name="value" type="{name}"/></declare>"#)
}
fn write(root: &std::path::Path, name: &str, content: &str) {
    fs::write(
        root.join(name),
        format!(r#"<organon version="1">{content}</organon>"#),
    )
    .unwrap();
}

#[test]
fn import_syntax_is_unambiguous_and_roundtrips() {
    for from in ["organon-theoria", "ui", "./parts.xml", "../parts.xml"] {
        let xml = format!(
            r#"<organon version="1"><import from="{from}"/><use from="{from}" name="Panel" as="Card"/></organon>"#
        );
        let file = DefinitionFile::from_xml(&xml).unwrap();
        assert_eq!(
            DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
            file
        );
    }
    for from in [
        "/parts.xml",
        "C:/parts.xml",
        "parts.xml",
        "./bad\\parts.xml",
    ] {
        let xml = format!(r#"<organon version="1"><use from="{from}" name="Panel"/></organon>"#);
        assert!(DefinitionFile::from_xml(&xml).is_err(), "{from}");
    }
    assert!(
        DefinitionFile::from_xml(r#"<organon version="1"><import from="ui" as="UI"/></organon>"#)
            .is_err()
    );
    assert!(
        DefinitionFile::from_xml(r#"<organon version="1"><import path="./parts.xml"/></organon>"#)
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
        assert!(DefinitionFile::from_xml(&xml).is_err(), "{import}");
    }
    let file =
        DefinitionFile::from_xml(r#"<organon version="1"><use from="ui" name="Panel"/></organon>"#)
            .unwrap();
    assert_eq!(file.uses().next().unwrap().name, "Panel");
    assert_eq!(
        DefinitionFile::from_xml(&file.to_xml().unwrap()).unwrap(),
        file
    );
    assert!(organon_core::semantic::resolve(&file).is_err());
    assert!(organon_core::format::ValueExpr::from_xml(r#"<use from="ui" name="Panel"/>"#).is_err());
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
    let loaded = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(loaded.paths.len(), 3);
    assert_eq!(loaded.public.len(), 2);
    assert_eq!(loaded.public["A"], loaded.public["AliasA"]);
    assert_eq!(loaded.origins[&loaded.public["A"]].name, "A");
    assert_eq!(loaded.file.definitions().count(), 3);
    assert!(!loaded.public.contains_key("Other"));
    assert!(!loaded.public.contains_key("Ain"));
    write(
        &root,
        "entry.xml",
        r#"<import from="./a.xml"/><export name="Ain"/>"#,
    );
    assert!(
        loader::load(root.join("entry.xml"), &mut FilesOnly)
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
        loader::load(root.join("entry.xml"), &mut FilesOnly)
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
    let loaded = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
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
        loader::load(root.join("entry.xml"), &mut FilesOnly)
            .unwrap_err()
            .to_string()
            .contains("does not publish")
    );
}

#[test]
fn publication_is_explicit_and_independent_of_declaration_order() {
    let root = directory();
    write(&root, "base.xml", &declaration("Panel"));
    let loaded = loader::load(root.join("base.xml"), &mut FilesOnly).unwrap();
    assert!(loaded.public.is_empty());
    assert_eq!(loaded.file.definitions().count(), 1);
    write(
        &root,
        "entry.xml",
        r#"<use from="./base.xml" name="Panel"/>"#,
    );
    assert!(
        loader::load(root.join("entry.xml"), &mut FilesOnly)
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
    let loaded = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
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
            loader::load(root.join("entry.xml"), &mut FilesOnly)
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
    let loaded = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    assert_eq!(loaded.paths.len(), 3);
    let names: Vec<_> = loaded
        .file
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
    let changed = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
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
      <declare name="Hidden" tag="false"/>
      <declare name="Public"><output name="value" type="Hidden"/></declare>
      <export name="Public"/>
    "#,
    );
    for declaration in [
        r#"<declare name="Consumer"><input name="value" type="Hidden"/></declare>"#,
        r#"<declare name="Consumer"><children type="Hidden"/></declare>"#,
        r#"<interface name="Consumer"><output name="value" type="Hidden"/></interface>"#,
        r#"<element name="Consumer"><input name="value" type="Hidden"/><provide name="value" ref="value"/></element>"#,
    ] {
        write(
            &root,
            "entry.xml",
            &format!(r#"<use from="./library.xml" name="Public"/>{declaration}"#),
        );
        let error = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap_err();
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
      <declare name="Consumer"><input name="value" type="Visible"/></declare>
    "#,
    );
    let loaded = loader::load(root.join("entry.xml"), &mut FilesOnly).unwrap();
    organon_core::semantic::resolve(&loaded.file).unwrap();
}
