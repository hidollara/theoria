#[path = "../../../crates/organon-theoria/tests/support/library.rs"]
mod library;

#[test]
fn compiled_and_previewed_libraries_share_attribute_behavior() {
    library::run(
        "app",
        &[
            ("attributes", &["tests/fixtures/attributes.xml"]),
            ("conversions", &["tests/fixtures/conversions.xml"]),
            (
                "interface_attributes",
                &["tests/fixtures/interface_attributes.xml"],
            ),
        ],
        true,
    );
}
