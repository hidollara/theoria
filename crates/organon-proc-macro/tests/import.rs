use std::{error::Error, fs, path::Path, process::Command, time::SystemTime};

const XML: &str = r#"<element name="Shape" version="1">
  <let name="outline" export="true">
    <Rect x="0" y="0" width="12" height="24"/>
  </let>
</element>"#;

// The XML lives outside src, with spaces and Unicode in its path. Imports in
// separate Rust modules must resolve from the consumer's manifest, not this crate
// or the Rust source file, and must not share generated helper names.
const RUST: &str = r#"
mod first { organon::import!("definitions/図形 sample.xml"); }
mod second { organon::import!(r"definitions/図形 sample.xml"); }
fn main() {
    let _ = first::Shape::builder();
    let _ = second::Shape::builder();
}
"#;

#[test]
fn imports_track_xml_changes_and_report_errors() -> Result<(), Box<dyn Error>> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .ok_or("missing repository root")?;
    let executable = std::env::current_exe()?;
    let target = executable
        .ancestors()
        .nth(3)
        .ok_or("missing target directory")?
        .join("organon-macro-tests");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let consumer = target.join(format!("cases/{}-{stamp}", std::process::id()));
    fs::create_dir_all(consumer.join("src"))?;
    fs::create_dir_all(consumer.join("definitions"))?;
    let mut manifest = String::from(
        "[package]\nname = \"organon-macro-consumer\"\nversion = \"0.0.0\"\nedition = \"2024\"\n[workspace]\n[dependencies]\n",
    );
    for name in [
        "organon",
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
    fs::write(consumer.join("Cargo.toml"), manifest)?;
    fs::copy(repository.join("Cargo.lock"), consumer.join("Cargo.lock"))?;
    let rust = consumer.join("src/main.rs");
    let xml = consumer.join("definitions/図形 sample.xml");
    fs::write(&rust, RUST)?;
    fs::write(&xml, XML)?;

    let check = |expected_error: Option<&str>| -> Result<(), Box<dyn Error>> {
        let output = Command::new(env!("CARGO"))
            .args(["check", "--offline", "--color", "never", "--manifest-path"])
            .arg(consumer.join("Cargo.toml"))
            .arg("--target-dir")
            .arg(target.join("build"))
            .output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        match expected_error {
            Some(message) => {
                assert!(
                    !output.status.success(),
                    "expected {message}; {}",
                    consumer.display()
                );
                assert!(stderr.contains(message), "missing {message}; {stderr}");
            }
            None => assert!(output.status.success(), "{}: {stderr}", consumer.display()),
        }
        Ok(())
    };
    check(None)?;

    // Track a transitive source, deduplicated even through multiple import paths.
    fs::create_dir_all(consumer.join("definitions/parts"))?;
    let dependency = consumer.join("definitions/parts/shared.xml");
    fs::write(&dependency, XML)?;
    fs::write(
        consumer.join("definitions/parts/entry.xml"),
        r#"<organon version="1"><import path="shared.xml"/></organon>"#,
    )?;
    fs::write(
        &xml,
        r#"<organon version="1"><import path="parts/entry.xml"/><import path="parts/./shared.xml"/></organon>"#,
    )?;
    check(None)?;
    fs::write(&dependency, XML.replace("Shape", "DependencyChanged"))?;
    check(Some("could not find `Shape`"))?;
    fs::write(&dependency, XML)?;
    check(None)?;
    fs::write(&xml, XML)?;

    // Only the XML changes: rustc must re-expand the macro and detect that the
    // previously imported type no longer exists. No clean or Rust edit is used.
    fs::write(&xml, XML.replace("Shape", "ChangedShape"))?;
    check(Some("could not find `Shape`"))?;
    fs::write(&rust, RUST.replace("::Shape", "::ChangedShape"))?;
    check(None)?;

    let invalid = XML.replace("width=\"12\"", "unknown=\"12\"");
    let expected = organon_compiler::compile_xml(&invalid)
        .unwrap_err()
        .to_string();
    fs::write(&xml, invalid)?;
    check(Some(&expected))?;

    fs::write(&rust, "organon::import!(\"missing.xml\");\nfn main() {}")?;
    check(Some("missing.xml"))?;
    fs::write(&rust, "organon::import!(42);\nfn main() {}")?;
    check(Some("expected string literal"))?;

    // Public interface use crosses the proc-macro/facade boundary. The Rust
    // compiler checks the returned named fields and their computation types.
    fs::write(
        &xml,
        r#"<organon version="1"><interface name="Panel">
        <input name="width" type="f32"/><output name="value" type="f32"/>
        </interface>
        <element name="Shape"><let name="outline" export="true">
        <Rect x="0" y="0" width="12" height="24"/>
        </let></element></organon>"#,
    )?;
    let implementation = r#"
mod contracts { organon::import!("definitions/図形 sample.xml"); }
use contracts::{Input, Panel};
struct Make;
impl organon::Implementation<Panel> for Make {
    fn build(&self, input: Input<Panel>) -> Panel {
        let width = input.width().clone();
        Panel { value: organon::output(move |cx| *width.read(cx)) }
    }
}
fn main() {
    let reactive = theoria_reactive::Reactive::new();
    let value = Panel::builder().width(3.0).build(&reactive, &Make).unwrap();
    assert_eq!(value.value_fixed(), Some(&3.0));
    let _shape = contracts::Shape::builder().build(&reactive).unwrap();
}
"#;
    fs::write(&rust, implementation)?;
    check(None)?;
    fs::write(
        &rust,
        implementation.replace("*width.read(cx)", "\"wrong output\""),
    )?;
    check(Some("mismatched types"))?;
    fs::write(
        &rust,
        implementation.replace(
            "Panel { value: organon::output(move |cx| *width.read(cx)) }",
            "Panel {}",
        ),
    )?;
    check(Some("missing field `value`"))?;
    fs::write(
        &rust,
        implementation.replace("input.width()", "input.unknown()"),
    )?;
    check(Some("no method named `unknown`"))?;

    let with_call = fs::read_to_string(&xml)?.replace(
        "</organon>",
        r#"
        <element name="Host"><let name="panel"><Panel width="5"/></let>
        <let name="value" export="true"><get name="value"><ref name="panel"/></get></let></element></organon>"#,
    );
    fs::write(&xml, with_call)?;
    fs::write(&rust, implementation)?;
    check(Some("needs an implementation binding"))?;
    let bound = implementation
        .replace(
            "sample.xml\");",
            "sample.xml\", implementations { Panel = crate::Make });",
        )
        .replace(
            "let reactive =",
            "let _ = contracts::Host::builder(); let reactive =",
        );
    fs::write(&rust, &bound)?;
    check(None)?;
    fs::write(&rust, bound.replace("Panel = crate::Make", "Panel = 42"))?;
    check(Some("Implementation<Panel>"))?;
    fs::write(
        &rust,
        bound.replace("Panel = crate::Make", "Unknown = crate::Make"),
    )?;
    check(Some("does not name an interface"))?;
    fs::write(
        &rust,
        bound.replace(
            "Panel = crate::Make",
            "Panel = crate::Make, Panel = crate::Make",
        ),
    )?;
    check(Some("duplicate implementation binding"))?;
    Ok(())
}
