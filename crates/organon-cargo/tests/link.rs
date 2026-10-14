use organon_cargo::{connect_library_paths, export_connector};
use std::{fs, path::PathBuf, process::Command};

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/organon-cargo-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
        fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    fn compile(&self, name: &str, source: &str, dependency: Option<&str>) -> std::process::Output {
        let input = self.0.join(format!("{name}.rs"));
        fs::write(&input, source).unwrap();
        let mut command = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()));
        command.args(["--edition=2024", "--crate-type=rlib", "--crate-name", name]);
        command
            .arg(&input)
            .arg("-o")
            .arg(self.0.join(format!("lib{name}.rlib")));
        if let Some(dependency) = dependency {
            command.arg("--extern").arg(format!(
                "renamed={}",
                self.0.join(format!("lib{dependency}.rlib")).display()
            ));
        }
        command.output().unwrap()
    }
}

#[test]
fn library_connections_work_without_an_execution_backend() {
    let fixture = Fixture::new();
    let connector = export_connector(&syn::parse_quote!(crate::graphics::widgets)).unwrap();
    let provider = format!(
        "pub mod graphics {{pub mod widgets {{
            {connector}
            pub mod __organon {{pub const VALUE: u32 = 42;}}
        }}}}"
    );
    let result = fixture.compile("provider", &provider, None);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    // Both ordinary paths and paths inside macro arguments connect through the
    // provider's own module, even when Cargo exposes it under a dependency alias.
    let consumer = connect_library_paths(
        "pub const VALUE: u32 = ::renamed::__organon::VALUE;
         mod nested {
             macro_rules! read { ($value:expr) => { $value }; }
             const _: () = assert!(read!(::renamed::__organon::VALUE) == 42);
         }
         const _: () = assert!(VALUE == 42);",
        None,
    )
    .unwrap();
    let result = fixture.compile("consumer", &consumer, Some("provider"));
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );

    let misplaced = format!("pub mod graphics {{ {connector} pub mod __organon {{}} }}");
    let result = fixture.compile("misplaced", &misplaced, None);
    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr)
            .contains("library! path must match its containing module")
    );
}
