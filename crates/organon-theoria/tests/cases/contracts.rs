use organon_theoria::Provide;
use theoria_reactive::Reactive;
mod generated {
    organon_theoria::library!(crate::generated);
}
use generated::*;

#[organon_theoria::implement]
impl Native {
    #[output(name = "text")]
    #[output(interface = "Caption", name = "content")]
    fn text(input: Input<'_, Self>) -> String {
        input.text().into_cloned()
    }
}
#[test]
fn contracts_preserve_fixedness_and_distinct_namespaces() {
    let reactive = Reactive::new();
    let label = Label::builder()
        .text("hello")
        .width(12.0)
        .build(&reactive)
        .unwrap();
    let text: Caption = label.provide();
    let size: Measure = label.provide();
    assert_eq!(text.content_fixed().map(String::as_str), Some("hello"));
    assert_eq!(size.content_fixed(), Some(&12.0));
    let consumer = Consumer::builder().item(text).build(&reactive).unwrap();
    assert_eq!(consumer.text_fixed().map(String::as_str), Some("hello"));
    let native = Native::builder().text("native").build(&reactive).unwrap();
    let text: Caption = native.provide();
    assert_eq!(text.content_fixed().map(String::as_str), Some("native"));
}
#[test]
fn xml_conformance_preserves_reactive_output_updates() {
    let reactive = Reactive::new();
    let text = reactive.create_state(String::from("first"));
    let scene = Scene::builder()
        .text(text.value())
        .width(1.0)
        .build(&reactive)
        .unwrap();
    assert!(scene.text_output_fixed().is_none());
    let value = scene.text_output();
    let read = reactive
        .create_computed(move |cx| value.read(&organon_theoria::ReadContext::new(cx)).clone());
    assert_eq!(&*read.snapshot().unwrap(), "first");
    text.set("second".into()).unwrap();
    assert_eq!(&*read.snapshot().unwrap(), "second");
}

#[test]
fn interface_adaptation_tracks_element_replacement() {
    let reactive = Reactive::new();
    let make = |text: &str| {
        Label::builder()
            .text(text.to_owned())
            .width(1.0)
            .build(&reactive)
            .unwrap()
    };
    let label = reactive.create_state(make("first"));
    let forwarded = Forward::builder()
        .label(label.value())
        .build(&reactive)
        .unwrap();
    let value = forwarded.text();
    let read = reactive
        .create_computed(move |cx| value.read(&organon_theoria::ReadContext::new(cx)).clone());
    assert_eq!(&*read.snapshot().unwrap(), "first");
    label.set(make("second")).unwrap();
    assert_eq!(&*read.snapshot().unwrap(), "second");
}
