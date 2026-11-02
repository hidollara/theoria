#[path = "../../../crates/organon-theoria/tests/support/library.rs"]
mod library;

#[test]
fn failure_contracts_preserve_declared_interface_precedence() {
    library::run(
        "app",
        &[(
            "failure_contracts",
            &["tests/fixtures/failure_contracts.xml"],
        )],
        true,
    );
}

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

#[test]
fn native_outputs_and_predicates_can_only_raise_declared_errors() {
    library::run_checked(
        "app",
        &[("control", &["tests/fixtures/control.xml"])],
        true,
        &[
            (
                "case.rs",
                "::<types::number, _>(n)",
                "::<ui::types::ThirdNumber, _>(n)",
                "Contains",
            ),
            (
                "case.rs",
                "::<types::number, _>(1.0)",
                "::<ui::types::ThirdNumber, _>(1.0)",
                "Contains",
            ),
        ],
    );
}
