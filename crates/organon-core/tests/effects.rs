#![cfg(feature = "xml")]
use organon_core::{semantic, xml};

#[test]
fn interface_failures_do_not_match_concrete_or_unrelated_handlers() {
    let document = r#"<organon version="1">
      <interface name="Problem"/>
      <interface name="Unrelated"/>
      <declare-element name="Fault" tag="false">
        <implements interface="Problem"/>
      </declare-element>
      <declare-element name="Other" tag="false"/>
      <declare-element name="Data" tag="false"/>
      <element name="Probe">
        <input name="source" type="Data"><error type="Problem"/></input>
        <let name="result">
          <try type="Data" result="primary">
            <catch type="Fault" throw="Other" result="reason"/>
            <catch type="Unrelated" throw="Other" result="reason"/>
            <catch type="Problem" result="recovered"/>
          </try>
        </let>
        <provide name="value" ref="result"/>
      </element>
    </organon>"#;
    let program = semantic::resolve(&xml::parse(document).unwrap()).unwrap();
    let graph = semantic::effects::graph(&program);
    let selected = graph
        .attributes
        .iter()
        .map(|a| {
            a.bindings
                .keys()
                .map(|name| a.text == "primary" && name == "source")
                .collect()
        })
        .collect::<Vec<Vec<bool>>>();
    let sets = graph.evaluate(&selected);
    let output = graph.outputs[&("Probe".into(), "value".into())];
    assert!(sets[output].iter().all(|present| !present));
}

#[test]
fn parsed_requirements_control_caught_and_handler_failures() {
    let document = r#"<organon version="1">
      <declare-element name="Data" tag="false"/>
      <declare-element name="A" tag="false"/>
      <declare-element name="B" tag="false"/>
      <declare-element name="C" tag="false"/>
      <element name="Probe">
        <input name="source" type="Data"><error type="A"/><error type="B"/></input>
        <let name="result">
          <try type="Data" result="primary">
            <catch type="A" throw="C" result="reason"/>
          </try>
        </let>
        <provide name="value" ref="result"/>
      </element>
    </organon>"#;
    for (guard, expected) in [(false, vec!["B", "C"]), (true, vec!["A", "B", "C"])] {
        let document = if guard {
            document.replace("result=\"reason\"/>", "result=\"reason\"><all/></catch>")
        } else {
            document.to_owned()
        };
        let program = semantic::resolve(&xml::parse(&document).unwrap()).unwrap();
        // Native syntax is opaque until the parser supplies its dependencies.
        assert!(matches!(
            program.contracts["Probe"].outputs[0].ty,
            semantic::OutputType::Inferred(_)
        ));
        let graph = semantic::effects::graph(&program);
        let output = graph.outputs[&("Probe".into(), "value".into())];
        let requirements = |uses_source| {
            graph
                .attributes
                .iter()
                .map(|a| {
                    a.bindings
                        .keys()
                        .map(|name| uses_source && a.text == "primary" && name == "source")
                        .collect()
                })
                .collect::<Vec<Vec<bool>>>()
        };
        assert!(
            graph.evaluate(&requirements(false))[output]
                .iter()
                .all(|present| !present)
        );
        let sets = graph.evaluate(&requirements(true));
        let actual: Vec<_> = graph
            .reasons
            .iter()
            .zip(&sets[output])
            .filter(|(_, present)| **present)
            .map(|(reason, _)| reason.spelling())
            .collect();
        assert_eq!(actual, expected);
    }
}
