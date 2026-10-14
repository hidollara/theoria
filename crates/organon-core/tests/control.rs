#![cfg(feature = "xml")]
use organon_core::{
    semantic::{self, Argument, ExpressionKind},
    xml,
};

fn definition(body: &str) -> String {
    format!(
        r#"<organon version="1">
      <declare-element name="Data"/>
      <interface name="View"/>
      <declare-element name="Widget"><implements interface="View"/></declare-element>
      <interface name="Problem"/>
      <declare-element name="Fault"><implements interface="Problem"/></declare-element>
      <declare-predicate name="Ready"><input name="item" type="Data"/></declare-predicate>
      <predicate name="Visible"><input name="a" type="Data"/>
        <not><Ready item="{{a}}"/></not>
      </predicate>
      <element name="Example"><input name="outer" type="Data"/>
        <let name="value">{body}</let>
        <provide name="output" ref="value"/>
      </element>
    </organon>"#
    )
}

#[test]
fn xml_predicates_validate_input_error_contracts() {
    let source = definition(r#"<Data/>"#).replace(
        r#"<input name="a" type="Data"/>"#,
        r#"<input name="a" type="Data"><error type="Fault"/></input>"#,
    );
    let check = |s: &str| semantic::resolve(&xml::parse(s).unwrap());
    let program = check(&source).unwrap();
    let predicate = &program.predicates["Visible"];
    assert_eq!(predicate.inputs[0].ty.errors[0].spelling(), "Fault");
    assert!(matches!(
        predicate.body,
        semantic::PredicateBody::Defined { .. }
    ));
    let requirements = semantic::effects::graph(&program);
    assert!(
        requirements
            .attributes
            .iter()
            .any(|a| a.text == "{a}" && a.bindings.contains_key("a"))
    );
    for errors in [
        r#"<error type="Missing"/>"#,
        r#"<error type="Fault"/><error type="Fault"/>"#,
    ] {
        assert!(check(&source.replace(r#"<error type="Fault"/>"#, errors)).is_err());
    }
}

#[test]
fn element_typed_predicate_arguments_leave_unaccepted_reasons_with_the_caller() {
    let doc = r#"<organon version="1">
      <declare-element name="Data"/>
      <declare-element name="Temporary"/>
      <declare-element name="Permanent"/>
      <element name="Item">
        <input name="data" type="Data"/>
        <provide name="data" ref="data"/>
      </element>
      <predicate name="Accept">
        <input name="item" type="Item"><error type="Temporary"/></input>
        <all/>
      </predicate>
      <element name="Checked">
        <input name="source" type="Item"><error type="Temporary"/><error type="Permanent"/></input>
        <let name="answer"><select type="Data"><case result="ok"><Accept item="source"/></case></select></let>
        <provide name="answer" ref="answer"/>
      </element>
    </organon>"#;
    let program = semantic::resolve(&xml::parse(doc).unwrap()).unwrap();
    let errors = known_errors(&program, "Checked", "answer");
    assert_eq!(
        errors.iter().map(|e| e.spelling()).collect::<Vec<_>>(),
        ["Permanent"]
    );
}

#[test]
fn exits_keep_the_parser_selected_by_the_success_or_reason_type() {
    let doc = definition(
        r#"<select type="View">
      <case result="{item}">
        <all><Visible a="{outer}"/><not><any/></not></all>
        <let name="item"><Widget/></let>
      </case>
      <case throw="Fault" result="invalid"/>
    </select>"#,
    );
    let program = semantic::resolve(&xml::parse(&doc).unwrap()).unwrap();
    let output = &program.contracts["Example"].outputs[0].ty;
    assert_eq!(output.kind().spelling(), "View");
    assert_eq!(
        known_errors(&program, "Example", "output")[0].spelling(),
        "Fault"
    );
    let ExpressionKind::Select(cases) = &program.bodies[0].bindings[0].value.kind else {
        panic!()
    };
    for (case, (parser, raw)) in cases.iter().zip([("View", "{item}"), ("Fault", "invalid")]) {
        let Argument::Attribute { expected, text, .. } = &case.body.result else {
            panic!()
        };
        assert_eq!((expected.kind.spelling(), text.as_str()), (parser, raw));
    }
}

#[test]
fn scopes_allow_forward_declarations_and_reject_shadowing_and_cross_branch_reads() {
    let body = r#"<try type="Boxed" result="work">
      <let name="work"><ref name="later"/></let>
      <let name="later"><Boxed/></let>
      <catch result="recovery"><let name="recovery"><Boxed/></let></catch>
    </try>"#;
    let doc = definition(body).replace(
        "<element name=\"Example\">",
        r#"<element name="Boxed">
      <let name="data"><Data/></let><provide name="value" ref="data"/>
    </element><element name="Example">"#,
    );
    assert!(semantic::resolve(&xml::parse(&doc).unwrap()).is_ok());
    for bad in [
        doc.replace("result=\"recovery\"", "result=\"work\""),
        doc.replace("name=\"recovery\"", "name=\"outer\""),
        doc.replace(
            "<Boxed/></let></catch>",
            "<ref name=\"later\"/></let></catch>",
        ),
    ] {
        assert!(
            semantic::resolve(&xml::parse(&bad).unwrap()).is_err(),
            "{bad}"
        );
    }
}

#[test]
fn catch_subtracts_only_unconditionally_handled_reasons() {
    let doc = definition(
        r#"<try type="Boxed" result="choice">
      <let name="choice"><select type="Boxed"><case throw="Fault" result="broken"/></select></let>
      <catch type="Problem" name="why" result="recovery">
        <let name="recovery"><Boxed/></let>
      </catch>
    </try>"#,
    )
    .replace(
        "<element name=\"Example\">",
        r#"<element name="Boxed">
      <let name="data"><Data/></let><provide name="value" ref="data"/>
    </element><element name="Example">"#,
    );
    let errors = |s: &str| {
        known_errors(
            &semantic::resolve(&xml::parse(s).unwrap()).unwrap(),
            "Example",
            "output",
        )
    };
    assert!(errors(&doc).is_empty());
    let guarded = doc.replace(
        "<let name=\"recovery\">",
        "<Ready item=\"{outer}\"/><let name=\"recovery\">",
    );
    assert_eq!(errors(&guarded)[0].spelling(), "Fault");
    let rethrow = doc.replace("result=\"recovery\"", "throw=\"Data\" result=\"reason\"");
    assert_eq!(errors(&rethrow)[0].spelling(), "Data");
    for assertion in ["<unreachable/>", r#"<unreachable type="Problem"/>"#] {
        let before = rethrow.replace("<catch ", &format!("{assertion}<catch "));
        assert!(errors(&before).is_empty());
        let after = rethrow.replace("</catch>", &format!("</catch>{assertion}"));
        assert_eq!(errors(&after)[0].spelling(), "Data");
    }
}

#[test]
fn catch_preserves_native_parser_requirements_without_guessing_reference_syntax() {
    let doc = definition(
        r#"<try type="Data" result="{outer}">
      <catch type="Fault" result="offline"/>
    </try>"#,
    )
    .replace(
        r#"<input name="outer" type="Data"/>"#,
        r#"<input name="outer" type="Data"><error type="Fault"/></input>"#,
    );
    for fallback in ["offline", "{{outer}}", "again: {outer}"] {
        let program =
            semantic::resolve(&xml::parse(&doc.replace("offline", fallback)).unwrap()).unwrap();
        let effects = semantic::effects::graph(&program);
        let requirement = effects
            .attributes
            .iter()
            .find(|a| a.text == fallback)
            .unwrap();
        assert_eq!(requirement.kind.spelling(), "Data");
        assert!(requirement.bindings.contains_key("outer"));
        // Braces and literal-looking strings are equally opaque to core.
        assert!(matches!(
            program.contracts["Example"].outputs[0].ty,
            semantic::OutputType::Inferred(_)
        ));
    }
}

#[test]
fn rejects_misplaced_predicates_and_ambiguous_branches() {
    for body in [
        r#"<select type="Data"><case result="a"><Ready/><Ready/></case></select>"#,
        r#"<select type="Data"><case result="a"><not/></case></select>"#,
        r#"<select type="Data"><case result="a"><not><Ready/><Ready/></not></case></select>"#,
        r#"<try type="Data" result="a"><catch name="e" result="b"/></try>"#,
        r#"<try type="Data" result="a"><Ready/></try>"#,
        r#"<try type="Data" result="a"><unreachable result="a"/></try>"#,
        r#"<try type="Data" result="a"><unreachable><Ready/></unreachable></try>"#,
        r#"<select type="Data"><unreachable/></select>"#,
        r#"<select type="Data"><let name="x"><Data/></let></select>"#,
    ] {
        assert!(xml::parse(&definition(body)).is_err(), "{body}");
    }
    for body in [
        r#"<select type="Data"><case result="a"><Data/></case></select>"#,
        r#"<select type="Data"><case result="a"><Ready/></case></select>"#,
        r#"<select type="Data"><case throw="Undeclared" result="a"/></select>"#,
        r#"<try type="Data" result="a"><unreachable type="Undeclared"/></try>"#,
        r#"<Visible a="{outer}"/>"#,
    ] {
        assert!(
            semantic::resolve(&xml::parse(&definition(body)).unwrap()).is_err(),
            "{body}"
        );
    }
}

// Before native parsers run, this is a lower bound, not an output contract.
fn known_errors(
    program: &semantic::Program,
    element: &str,
    output: &str,
) -> Vec<organon_core::types::TypeName> {
    let graph = semantic::effects::graph(program);
    let selected = graph
        .attributes
        .iter()
        .map(|a| vec![false; a.bindings.len()])
        .collect::<Vec<_>>();
    let sets = graph.evaluate(&selected);
    let id = graph.outputs[&(element.into(), output.into())];
    graph
        .reasons
        .iter()
        .zip(&sets[id])
        .filter(|(_, present)| **present)
        .map(|(r, _)| r.clone())
        .collect()
}
