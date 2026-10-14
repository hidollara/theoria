#[path = "support/library.rs"]
mod library;

#[test]
fn generated_libraries_preserve_construction_and_reactive_contracts() {
    library::run(
        "theoria",
        &[
            ("bind", &["tests/fixtures/bind.xml"]),
            ("contracts", &["tests/fixtures/contracts.xml"]),
            ("declarations", &["samples/bind.xml"]),
            ("elements", &["tests/fixtures/elements.xml"]),
            ("imports", &["tests/fixtures/scene.xml"]),
            (
                "interface",
                &[
                    "tests/fixtures/native.organon.xml",
                    "tests/fixtures/forward.organon.xml",
                ],
            ),
            ("typed_attributes", &["tests/fixtures/typed_attributes.xml"]),
            ("unified", &["tests/fixtures/unified.xml"]),
        ],
        false,
    );
}
