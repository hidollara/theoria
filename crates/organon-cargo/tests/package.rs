use organon_cargo::{self as package, CargoSources};
use organon_core::loader::Sources;
use std::{
    fs,
    path::{Path, PathBuf},
};

struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "organon-cargo-package-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir_all(root.join("app/src")).unwrap();
        fs::create_dir_all(root.join("library/src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            "[workspace]\nmembers = [\"app\", \"library\"]\nresolver = \"3\"\n",
        )
        .unwrap();
        fs::write(
            root.join("app/Cargo.toml"),
            r#"
[package]
name = "consumer"
version = "0.1.0"
edition = "2024"
[dependencies]
ui = { package = "widgets", path = "../library" }
"#,
        )
        .unwrap();
        fs::write(
            root.join("library/Cargo.toml"),
            r#"
[package]
name = "widgets"
version = "0.1.0"
edition = "2024"
[lib]
name = "widget_types"
"#,
        )
        .unwrap();
        for directory in ["app", "library"] {
            fs::write(root.join(directory).join("src/lib.rs"), "").unwrap();
        }
        fs::write(
            root.join("library/organon.xml"),
            r#"<organon version="1"><declare name="Data" tag="false"/><export name="Data"/></organon>"#,
        )
        .unwrap();
        fs::write(
            root.join("app/organon.xml"),
            r#"<organon version="1"><import from="ui"/><export name="Data"/></organon>"#,
        )
        .unwrap();
        Self(root)
    }
    fn path(&self, path: &str) -> PathBuf {
        self.0.join(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        if canonical(&self.0).starts_with(canonical(std::env::temp_dir())) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
}

fn canonical(path: impl AsRef<Path>) -> PathBuf {
    fs::canonicalize(path).unwrap()
}

#[test]
fn cargo_lookup_uses_dependency_aliases_and_normalized_manifest_paths() {
    let fixture = Fixture::new();
    let mut sources = CargoSources::default();
    let manifest = fixture.path("library/Cargo.toml");
    assert_eq!(
        sources.library_name(&manifest).unwrap().as_deref(),
        Some("widget_types")
    );
    assert_eq!(
        sources
            .library_name(&canonical(&manifest))
            .unwrap()
            .as_deref(),
        Some("widget_types")
    );
    let dependencies = sources
        .local_dependencies(&fixture.path("app/Cargo.toml"))
        .unwrap();
    assert_eq!(dependencies.len(), 1);
    assert_eq!(dependencies[0].0, "ui");
    assert_eq!(
        canonical(&dependencies[0].1),
        canonical(fixture.path("library"))
    );
    let entry = fixture.path("app/organon.xml");
    assert_eq!(
        sources
            .dependency_rust_path(&fixture.path("app/Cargo.toml"), "widgets")
            .unwrap(),
        "::ui"
    );
    assert_eq!(
        sources
            .dependency_rust_path(&fixture.path("app/Cargo.toml"), "consumer")
            .unwrap(),
        "crate"
    );
    assert_eq!(
        canonical(sources.dependency(&entry, "ui").unwrap()),
        canonical(fixture.path("library/organon.xml"))
    );
    assert_eq!(
        canonical(sources.dependency(&entry, "consumer").unwrap()),
        canonical(&entry)
    );
    assert!(sources.dependency(&entry, "widgets").is_err());
    assert!(
        sources
            .tracked_paths()
            .any(|p| canonical(p) == canonical(&manifest))
    );
    let loaded = package::load(&entry).unwrap();
    assert!(loaded.program.signatures.contains_key("Data"));
    let providers = sources
        .provider_paths(&fixture.path("app/Cargo.toml"), &loaded.loaded)
        .unwrap();
    assert_eq!(
        providers[&canonical(fixture.path("library/organon.xml"))],
        "::ui"
    );
    let declaration = fixture.path("library/organon.xml");
    assert_eq!(
        package::manifest(&declaration).unwrap(),
        canonical(manifest)
    );
    assert_eq!(
        package::identity(&declaration, "Data").unwrap(),
        package::identity(&canonical(declaration), "Data").unwrap()
    );
}
