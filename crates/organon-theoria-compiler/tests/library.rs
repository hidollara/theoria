use std::{fs, path::PathBuf, time::SystemTime};

fn fixture(source: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/organon-compiler-tests")
        .join(format!("{}-{stamp}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("Cargo.toml"), "[package]\nname='test-library'\nversion='0.0.0'\nedition='2024'\n[workspace]\n[lib]\npath='lib.rs'\n").unwrap();
    fs::write(root.join("lib.rs"), "").unwrap();
    let path = root.join("organon.xml");
    fs::write(&path, source).unwrap();
    path
}

#[test]
fn declarations_without_primitive_names_use_the_same_compilation_path() {
    let path = fixture(
        r#"<organon version="1">
  <declare name="Number" tag="false"/>
  <element name="Echo">
    <input name="value" type="Number"/>
    <let name="copy"><ref name="value"/></let>
    <provide name="copy" ref="copy"/>
  </element>
  <export name="Echo"/>
</organon>"#,
    );
    let code = organon_theoria_compiler::library::compile(&path, "crate::ui").unwrap();
    syn::parse_file(&code).unwrap();
    assert!(code.contains("Echo"));
    assert!(!code.contains("theoria_canvas"));
    assert!(!code.contains("cfg (test)"));
}

#[test]
fn implicit_graphics_and_primitive_types_are_not_accepted() {
    let path = fixture(
        r#"<organon version="1">
  <element name="View">
    <input name="width" type="f32"/>
    <let name="shape"><Rect width="{width}" height="20"/></let>
    <provide name="shape" ref="shape"/>
  </element>
  <export name="View"/>
</organon>"#,
    );
    assert!(organon_theoria_compiler::library::compile(&path, "crate").is_err());
}

#[test]
fn libraries_without_value_declarations_use_the_same_generation_path() {
    let path = fixture(
        r#"<organon version="1">
  <declare name="Native"/>
  <element name="Echo">
    <input name="value" type="Native"/>
    <provide name="value" ref="value"/>
  </element>
  <export name="Native"/>
  <export name="Echo"/>
</organon>"#,
    );
    let code = organon_theoria_compiler::library::compile(&path, "crate").unwrap();
    syn::parse_file(&code).unwrap();
    assert!(code.contains("organon.xml"));
}
