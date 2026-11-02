use organon_app::{evaluation::Value, preview};
use std::collections::BTreeMap;

#[test]
fn preview_forwards_ordered_children_through_xml_elements() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/children.xml");
    let loaded = organon_cargo::load(path).unwrap();
    let mut registry = preview::registry();
    for (local, origin) in &loaded.origins {
        let symbol = organon_cargo::identity(&origin.source, &origin.name).unwrap();
        let _ = registry.alias(local, &symbol);
    }
    for label in ["a", "z"] {
        let outputs = registry
            .evaluate(
                &loaded.program,
                "Scene",
                BTreeMap::from([("label".into(), Value::new(label.to_owned()))]),
            )
            .unwrap();
        assert_eq!(
            outputs["text"].as_ref().unwrap().get::<String>().unwrap(),
            format!("[{label}b/{label}b]")
        );
    }
    let outputs = registry
        .evaluate(&loaded.program, "Empty", BTreeMap::new())
        .unwrap();
    assert_eq!(
        outputs["text"].as_ref().unwrap().get::<String>().unwrap(),
        "[/]"
    );
}
