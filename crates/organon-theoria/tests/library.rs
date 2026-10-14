#[path = "support/library.rs"]
mod library;

#[test]
fn const_parser_requirements_determine_output_error_types() {
    library::run_checked(
        "theoria",
        &[("requirements", &["tests/fixtures/requirements.xml"])],
        false,
        &[
            (
                "tests/fixtures/requirements.xml",
                "n=\"10\"",
                "n=\"{risk}\"",
                "must satisfy FixedBox",
            ),
            (
                "case.rs",
                "const fn parse",
                "fn parse",
                "#[parse] requires a const fn",
            ),
            (
                "tests/fixtures/requirements.xml",
                "value=\"dependency\"",
                "value=\"unknown\"",
                "expected literal or dependency",
            ),
        ],
    );
}

#[test]
fn xml_elements_forward_children_without_rebuilding_them() {
    library::run(
        "theoria",
        &[("children", &["tests/fixtures/children.xml"])],
    );
}

#[test]
fn decimal_literals_and_references_are_checked_at_compile_time() {
    library::run_checked(
        "theoria",
        &[("const_number", &["tests/fixtures/const_number.xml"])],
        &[
            (
                "tests/fixtures/const_number.xml",
                "1.00000005960464477539062500001",
                "1e2",
                "expected a signed decimal number",
            ),
            (
                "tests/fixtures/const_number.xml",
                "1.00000005960464477539062500001",
                "NaN",
                "expected a signed decimal number",
            ),
            (
                "tests/fixtures/const_number.xml",
                "1.00000005960464477539062500001",
                "340282356779733661637539395458142568448",
                "outside the finite f32 range",
            ),
            (
                "tests/fixtures/const_number.xml",
                "{later}",
                "{missing}",
                "unknown attribute reference",
            ),
            (
                "tests/fixtures/const_number.xml",
                "<ref name=\"input\"/>",
                "<Literal/>",
                "reference does not satisfy",
            ),
        ],
    );
}

#[test]
fn string_templates_are_parsed_and_checked_at_compile_time() {
    library::run_checked(
        "theoria",
        &[("const_template", &["tests/fixtures/const_template.xml"])],
        &[
            (
                "tests/fixtures/const_template.xml",
                "{count}",
                "{missing}",
                "unknown attribute reference",
            ),
            (
                "tests/fixtures/const_template.xml",
                "{count}",
                "{count",
                "invalid attribute reference name",
            ),
            (
                "tests/fixtures/const_template.xml",
                "type=\"number\"",
                "type=\"Color\"",
                "reference does not satisfy",
            ),
            (
                "tests/fixtures/const_template.xml",
                "text=\"\"",
                "text=\"}\"",
                "unescaped closing brace",
            ),
        ],
    );
}

#[test]
fn attribute_preparation_requires_const_parsers_and_preserves_runtime_construction() {
    library::run_checked(
        "theoria",
        &[("parsers", &["tests/fixtures/parsers.xml"])],
        &[
            (
                "case.rs",
                "type Value = Number;",
                "type Value = Number; type Value = Number;",
                "only one type Value is allowed",
            ),
            (
                "tests/fixtures/parsers.xml",
                "digits=\"123456789\"",
                "digits=\"12x\"",
                "expected a digit",
            ),
            (
                "tests/fixtures/parsers.xml",
                "base=\"seven\"",
                "base=\"invalid\"",
                "expected seven",
            ),
            (
                "tests/fixtures/parsers.xml",
                "base=\"{later}\"",
                "base=\"{missing}\"",
                "unknown attribute reference",
            ),
            (
                "tests/fixtures/parsers.xml",
                "base=\"{later}\"",
                "base=\"{result}\"",
                "reference does not satisfy",
            ),
        ],
    );
}

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
    );
}
