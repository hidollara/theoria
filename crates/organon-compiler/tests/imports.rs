use organon_compiler::{compile, compile_xml, generate_loaded_file, load_definitions};
use std::{error::Error, fs, path::PathBuf, time::SystemTime};

fn directory() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("organon-imports-{}-{stamp}", std::process::id()))
}
fn contract(name: &str) -> String {
    format!(
        r#"<organon version="1"><interface name="{name}"><output name="value" type="f32"/></interface></organon>"#
    )
}

#[test]
fn resolves_diamonds_relative_to_each_file_and_reloads_edits() -> Result<(), Box<dyn Error>> {
    let root = directory();
    fs::create_dir_all(root.join("nested"))?;
    fs::write(root.join("base.xml"), contract("Base"))?;
    fs::write(
        root.join("nested/left.xml"),
        r#"<organon version="1"><import path="../base.xml"/></organon>"#,
    )?;
    fs::write(
        root.join("right.xml"),
        r#"<organon version="1"><import path="./base.xml"/></organon>"#,
    )?;
    let source = r#"<organon version="1"><import path="nested/left.xml"/><import path="right.xml"/><import path="base.xml"/></organon>"#;
    fs::write(root.join("entry.xml"), source)?;
    let loaded = load_definitions(root.join("entry.xml"))?;
    assert_eq!(loaded.paths.len(), 4);
    assert_eq!(loaded.file.definitions.len(), 1);
    assert_eq!(loaded.file.definitions[0].name(), "Base");
    assert!(loaded.paths.iter().all(|p| p.is_absolute()));
    syn::parse_file(&generate_loaded_file(&loaded.file)?)?;
    assert!(
        compile_xml(source)
            .unwrap_err()
            .to_string()
            .contains("unresolved imports")
    );
    compile(root.join("entry.xml"), root.join("generated.rs"))?;
    assert!(fs::read_to_string(root.join("generated.rs"))?.contains("r#Base"));
    fs::write(root.join("base.xml"), contract("Changed"))?;
    assert_eq!(
        load_definitions(root.join("entry.xml"))?.file.definitions[0].name(),
        "Changed"
    );
    Ok(())
}

#[test]
fn errors_identify_missing_files_collisions_and_cycles() -> Result<(), Box<dyn Error>> {
    let root = directory();
    fs::create_dir_all(&root)?;
    let entry = root.join("entry.xml");
    fs::write(
        &entry,
        r#"<organon version="1"><import path="other.xml"/></organon>"#,
    )?;
    let error = load_definitions(&entry).unwrap_err().to_string();
    assert!(error.contains("entry.xml") && error.contains("other.xml"));
    fs::write(
        root.join("other.xml"),
        r#"<organon version="1"><import path="entry.xml"/></organon>"#,
    )?;
    let error = load_definitions(&entry).unwrap_err().to_string();
    assert!(
        error.contains("cyclic imports")
            && error.contains("entry.xml")
            && error.contains("other.xml")
    );
    fs::write(root.join("other.xml"), contract("Same"))?;
    fs::write(
        &entry,
        contract("Same").replace("<interface", "<import path=\"other.xml\"/><interface"),
    )?;
    let error = load_definitions(&entry).unwrap_err().to_string();
    assert!(
        error.contains("duplicate definition")
            && error.contains("Same")
            && error.contains("other.xml")
    );
    fs::write(root.join("other.xml"), "not XML")?;
    assert!(
        load_definitions(&entry)
            .unwrap_err()
            .to_string()
            .contains("other.xml")
    );
    Ok(())
}
