use std::{error::Error, fs, path::PathBuf, process::Command, time::SystemTime};

/// Generate code at test runtime, then compile and run its behavioral tests.
/// The temporary consumer is a test artifact, not a workspace member.
#[test]
fn generated_builders_compile_and_run() -> Result<(), Box<dyn Error>> {
    let compiler = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repository = compiler
        .ancestors()
        .nth(2)
        .ok_or("missing repository root")?;
    let fixtures = compiler.join("tests/fixtures");
    let executable = std::env::current_exe()?;
    let target = executable
        .ancestors()
        .nth(3)
        .ok_or("test executable must be inside Cargo's target directory")?
        .join("organon-generated-tests");
    let stamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)?
        .as_nanos();
    let consumer = target.join(format!("cases/{}-{stamp}", std::process::id()));
    fs::create_dir_all(consumer.join("tests/generated"))?;

    for name in [
        "panel",
        "mark",
        "label",
        "literal",
        "interpolated_text",
        "inline_text",
        "interpolated_rect",
        "fields",
        "lets",
        "reactive",
        "short_circuit",
    ] {
        organon_compiler::compile(
            fixtures.join(format!("xml/{name}.organon.xml")),
            consumer.join(format!("tests/generated/{name}.rs")),
        )?;
    }
    for name in [
        "canvas.rs",
        "text_canvas.rs",
        "fields.rs",
        "lets.rs",
        "reactive.rs",
        "public_api.rs",
    ] {
        fs::copy(fixtures.join(name), consumer.join("tests").join(name))?;
    }

    let mut dependencies = String::new();
    for name in [
        "organon",
        "theoria-canvas",
        "theoria-text",
        "theoria-reactive",
        "theoria-component",
        "theoria-event",
    ] {
        let path = repository
            .join("crates")
            .join(name)
            .to_string_lossy()
            .replace('\\', "/");
        dependencies.push_str(&format!("{name} = {{ path = {path:?} }}\n"));
    }
    fs::write(
        consumer.join("Cargo.toml"),
        format!(
            r#"
[package]
name = "organon-generated-test-consumer"
version = "0.0.0"
edition = "2024"
publish = false

[workspace]

[dev-dependencies]
{dependencies}
font-test-data = "0.8.0"
"#
        ),
    )?;
    // Reuse the workspace's resolved dependency versions. The outer test's
    // dev-dependencies ensure these are available even on a fresh checkout.
    fs::copy(repository.join("Cargo.lock"), consumer.join("Cargo.lock"))?;

    // Use a separate target directory so nested Cargo cannot wait on the outer
    // invocation's build lock. Retain artifacts for caching and failure diagnosis.
    let output = Command::new(env!("CARGO"))
        .args(["test", "--offline", "--color", "never", "--manifest-path"])
        .arg(consumer.join("Cargo.toml"))
        .arg("--target-dir")
        .arg(target.join("build"))
        .output()?;
    assert!(
        output.status.success(),
        "generated code tests failed; artifacts: {}\n{}\n{}",
        consumer.display(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
    Ok(())
}
