//! Exercise real consumer packages without adding test crates to the workspace.
use organon_core::format::{DefinitionFile, FileDeclaration, Import, ImportSource};
use std::{fs, path::Path, process::Command, time::SystemTime};

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).unwrap();
        }
    }
}

pub fn run(suite: &str, cases: &[(&str, &[&str])], app: bool) {
    let package = Path::new(env!("CARGO_MANIFEST_DIR"));
    let repository = package.ancestors().nth(2).unwrap();
    let executable = std::env::current_exe().unwrap();
    let target = executable
        .ancestors()
        .nth(3)
        .unwrap()
        .join("organon-consumer-tests");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for (case, entries) in cases {
        let root = target.join(format!(
            "cases/{suite}-{case}-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        copy_tree(
            &package.join("tests/fixtures"),
            &root.join("tests/fixtures"),
        );
        copy_tree(
            &repository.join("docs/organon-design-samples"),
            &root.join("samples"),
        );
        let mut definition = DefinitionFile {
            declarations: vec![],
        };
        for entry in *entries {
            let file =
                DefinitionFile::from_xml(&fs::read_to_string(root.join(entry)).unwrap()).unwrap();
            definition
                .declarations
                .push(FileDeclaration::Import(Import {
                    from: ImportSource::Relative(format!("./{entry}")),
                }));
            definition.declarations.extend(
                file.declarations
                    .into_iter()
                    .filter(|d| matches!(d, FileDeclaration::Export(_))),
            );
        }
        fs::write(root.join("organon.xml"), definition.to_xml().unwrap()).unwrap();
        let source = package
            .join(format!("tests/cases/{case}.rs"))
            .to_string_lossy()
            .replace('\\', "/");
        let mut manifest = format!(
            "[package]\nname = \"{suite}-{case}\"\nversion = \"0.0.0\"\nedition = \"2024\"\nautotests = false\n[workspace]\n[[test]]\nname = \"{case}\"\npath = {source:?}\n[dependencies]\n"
        );
        for name in [
            "organon-theoria",
            "organon-core",
            "organon-cargo",
            "theoria-canvas",
            "theoria-text",
            "theoria-reactive",
            "theoria-component",
        ] {
            let path = repository
                .join("crates")
                .join(name)
                .to_string_lossy()
                .replace('\\', "/");
            manifest.push_str(&format!("{name} = {{ path = {path:?} }}\n"));
        }
        if app {
            let path = package.to_string_lossy().replace('\\', "/");
            manifest.push_str(&format!(
                "organon-app = {{ path = {path:?} }}\nfont-test-data = \"0.8.0\"\n"
            ));
        }
        fs::write(root.join("Cargo.toml"), manifest).unwrap();
        fs::copy(repository.join("Cargo.lock"), root.join("Cargo.lock")).unwrap();
        let output = Command::new(env!("CARGO"))
            .args(["test", "--offline", "--color", "never", "--manifest-path"])
            .arg(root.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(target.join("build"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{suite}/{case}:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!(
            "{suite}/{case}: {}",
            String::from_utf8_lossy(&output.stdout)
        );
    }
}
