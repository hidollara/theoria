use std::{error::Error, fs, path::Path, process::Command, time::SystemTime};

const XML: &str = r#"<organon version="1">
  <import from="organon-theoria"/>
  <element name="ExampleShape">
    <let name="outline">
      <Rect x="0" y="0" width="12" height="24"/>
    </let>
    <provide name="outline" ref="outline"/>
    <provide name="outline_source" ref="outline"/>
  </element>
  <export name="ExampleShape"/>
</organon>"#;

// The root is resolved from the consumer manifest. Ordinary Rust use shares
// the library types; transitive XML imports below exercise Unicode paths.
const RUST: &str = r#"
pub mod first { organon_theoria::library!(crate::first); }
mod second { pub use crate::first::*; }
fn main() {
    let _ = first::ExampleShape::builder();
    let _ = second::ExampleShape::builder();
}
"#;

#[test]
fn libraries_track_xml_changes_and_report_errors() -> Result<(), Box<dyn Error>> {
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
        "organon-theoria",
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
    let xml = consumer.join("organon.xml");
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

    // Empty and interface-only libraries do not require native construction storage.
    fs::write(
        &rust,
        "pub mod ui { organon_theoria::library!(crate::ui); } fn main() {}",
    )?;
    fs::write(&xml, r#"<organon version="1"/>"#)?;
    check(None)?;
    fs::write(
        &xml,
        r#"<organon version="1">
      <interface name="Empty"/>
      <element name="Identity">
        <input name="value" type="Empty"/>
        <provide name="value" ref="value"/>
      </element>
      <export name="Empty"/>
      <export name="Identity"/>
    </organon>"#,
    )?;
    check(None)?;
    fs::write(&rust, RUST)?;
    fs::write(&xml, XML)?;

    // File publication controls Rust names too: aliases replace the exposed name,
    // and local definitions are not implicitly public when no export is present.
    fs::write(
        &xml,
        XML.replace(
            r#"<export name="ExampleShape"/>"#,
            r#"<export name="ExampleShape" as="Card"/>"#,
        ),
    )?;
    check(Some("could not find `ExampleShape`"))?;
    fs::write(&rust, RUST.replace("::ExampleShape", "::Card"))?;
    check(None)?;
    fs::write(&xml, XML.replace(r#"<export name="ExampleShape"/>"#, ""))?;
    fs::write(&rust, RUST)?;
    check(Some("could not find `ExampleShape`"))?;
    fs::write(&xml, XML)?;

    // Track a transitive source, deduplicated even through multiple import paths.
    fs::create_dir_all(consumer.join("definitions/parts"))?;
    let dependency = consumer.join("definitions/parts/図形 sample.xml");
    fs::write(&dependency, XML)?;
    fs::write(
        consumer.join("definitions/parts/entry.xml"),
        r#"<organon version="1"><import from="./図形 sample.xml"/><export name="ExampleShape"/></organon>"#,
    )?;
    fs::write(
        &xml,
        r#"<organon version="1"><import from="./definitions/parts/entry.xml"/><import from="./definitions/parts/./図形 sample.xml"/><export name="ExampleShape"/></organon>"#,
    )?;
    check(None)?;
    fs::write(
        &dependency,
        XML.replace("ExampleShape", "DependencyChanged"),
    )?;
    check(Some("unknown public definition"))?;
    fs::write(&dependency, XML)?;
    check(None)?;
    fs::write(&xml, XML)?;

    // Only the XML changes: rustc must re-expand the macro and detect that the
    // previously imported type no longer exists. No clean or Rust edit is used.
    fs::write(&xml, XML.replace("ExampleShape", "ChangedExampleShape"))?;
    check(Some("could not find `ExampleShape`"))?;
    fs::write(
        &rust,
        RUST.replace("::ExampleShape", "::ChangedExampleShape"),
    )?;
    check(None)?;

    let invalid = XML.replace("width=\"12\"", "unknown=\"12\"");
    fs::write(&xml, invalid)?;
    check(Some("unknown input"))?;

    fs::write(&rust, RUST)?;
    fs::write(
        &xml,
        r#"<organon version="1"><import from="./missing.xml"/></organon>"#,
    )?;
    check(Some("missing.xml"))?;
    fs::write(&rust, "organon_theoria::library!(42); fn main() {}")?;
    check(Some("expected identifier"))?;

    // Cargo can rename the runtime facade as well as XML's dependency source.
    let manifest_path = consumer.join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path)?;
    fs::write(
        &manifest_path,
        manifest.replace(
            "organon-theoria = { path",
            "ui = { package = \"organon-theoria\", path",
        ),
    )?;
    fs::write(&xml, XML.replace("from=\"organon-theoria\"", "from=\"ui\""))?;
    fs::write(&rust, RUST.replace("organon_theoria::", "ui::"))?;
    check(None)?;
    fs::write(&manifest_path, manifest)?;

    // Public interface use crosses the proc-macro/facade boundary. The Rust
    // compiler checks the returned named fields and their computation types.
    fs::write(
        &xml,
        r#"<organon version="1">
  <import from="organon-theoria"/>
  <declare name="Panel">
    <input name="width" type="f32"/>
    <output name="value" type="f32"/>
  </declare>
  <element name="ExampleShape">
    <let name="outline">
      <Rect x="0" y="0" width="12" height="24"/>
    </let>
    <provide name="outline" ref="outline"/>
  </element>
  <export name="Panel"/>
  <export name="ExampleShape"/>
</organon>"#,
    )?;
    let implementation = r#"
pub mod contracts { organon_theoria::library!(crate::contracts); }
use contracts::Panel;
#[organon_theoria::implement]
impl Panel {
    #[output(name = "value")]
    fn value(input: Input<'_, Self>) -> f32 { *input.width() }
}
fn main() {
    let reactive = theoria_reactive::Reactive::new();
    let value = Panel::builder().width(3.0).build(&reactive).unwrap();
    assert_eq!(value.value_fixed(), Some(&3.0));
    let _shape = contracts::ExampleShape::builder().build(&reactive).unwrap();
}
"#;
    fs::write(&rust, implementation)?;
    check(None)?;
    let declared = fs::read_to_string(&xml)?;
    fs::write(
        &xml,
        declared.replace(
            r#"<export name="Panel"/>"#,
            r#"<export name="Panel" as="PublicPanel"/>"#,
        ),
    )?;
    fs::write(
        &rust,
        implementation
            .replace("contracts::Panel", "contracts::PublicPanel as Panel")
            .replace("impl Panel", "impl PublicPanel"),
    )?;
    check(None)?;
    fs::write(&xml, declared)?;
    fs::write(
        &rust,
        implementation.replace("*input.width()", "\"wrong output\""),
    )?;
    check(Some("mismatched types"))?;
    fs::write(
        &rust,
        implementation.replace("#[output(name = \"value\")]", ""),
    )?;
    check(Some("Evaluate"))?;
    fs::write(
        &rust,
        implementation.replace("input.width()", "input.unknown()"),
    )?;
    check(Some("no method named `unknown`"))?;

    let with_call = fs::read_to_string(&xml)?.replace(
        "</organon>",
        r#"
  <element name="Host">
    <let name="panel">
      <Panel width="5"/>
    </let>
    <bind value="value">
      <ref name="panel"/>
    </bind>
    <provide name="value" ref="value"/>
  </element>
  <export name="Host"/>
</organon>"#,
    );
    fs::write(&xml, with_call)?;
    fs::write(&rust, implementation)?;
    check(None)?;
    fs::write(&rust, implementation.replace("impl Panel", "impl Unknown"))?;
    check(Some("unknown declaration"))?;
    fs::write(
        &rust,
        implementation.replace(
            "#[organon_theoria::implement]",
            "#[organon_theoria::implement(name = \"Panel\")]",
        ),
    )?;
    check(Some("implement takes no arguments"))?;
    Ok(())
}
