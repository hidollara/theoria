//! Exercise real consumer packages without adding test crates to the workspace.
use organon_core::syntax::Item;
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

pub fn run(suite: &str, cases: &[(&str, &[&str])]) {
    run_checked(suite, cases, &[]);
}

pub fn run_checked(suite: &str, cases: &[(&str, &[&str])], failures: &[(&str, &str, &str, &str)]) {
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
        let mut definition = String::from("<organon version=\"1\">\n");
        for entry in *entries {
            let file =
                organon_core::xml::parse(&fs::read_to_string(root.join(entry)).unwrap()).unwrap();
            // Fixture paths are static; published names have been validated by the parser.
            definition.push_str(&format!("  <import from=\"./{entry}\"/>\n"));
            for item in file.items {
                if let Item::Export(export) = item {
                    definition.push_str(&format!("  <export name=\"{}\"", export.name));
                    if let Some(alias) = export.alias {
                        definition.push_str(&format!(" as=\"{alias}\""));
                    }
                    definition.push_str("/>\n");
                }
            }
        }
        definition.push_str("</organon>\n");
        fs::write(root.join("organon.xml"), definition).unwrap();
        // Compile-fail cases can also alter the native implementation in isolation.
        let source = "case.rs";
        fs::copy(
            package.join(format!("tests/cases/{case}.rs")),
            root.join(source),
        )
        .unwrap();
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
        for (path, from, to, diagnostic) in failures {
            let path = root.join(path);
            let original = fs::read_to_string(&path).unwrap();
            assert!(original.contains(from));
            fs::write(&path, original.replace(from, to)).unwrap();
            let output = Command::new(env!("CARGO"))
                .args([
                    "test",
                    "--no-run",
                    "--offline",
                    "--color",
                    "never",
                    "--manifest-path",
                ])
                .arg(root.join("Cargo.toml"))
                .arg("--target-dir")
                .arg(target.join("build"))
                .output()
                .unwrap();
            fs::write(&path, original).unwrap();
            let errors = String::from_utf8_lossy(&output.stderr);
            assert!(
                !output.status.success() && errors.contains(diagnostic),
                "expected {diagnostic}:\n{errors}"
            );
        }
    }
}
