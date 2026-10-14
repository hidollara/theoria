use organon::{Implementation, output};
use std::cell::Cell;
use theoria_reactive::Reactive;

mod definitions {
    organon::import!("tests/fixtures/get.xml", implementations { Scale = crate::ScaleImpl });
}

thread_local! { static BUILDS: Cell<usize> = const { Cell::new(0) }; }
struct ScaleImpl;
impl Implementation<definitions::Scale> for ScaleImpl {
    fn build(&self, input: definitions::Input<definitions::Scale>) -> definitions::Scale {
        BUILDS.set(BUILDS.get() + 1);
        let scaled = input.value().clone();
        let original = scaled.clone();
        definitions::Scale {
            scaled: output(move |cx| *scaled.read(cx) * 2.0),
            original: output(move |cx| *original.read(cx)),
        }
    }
}

#[test]
fn selections_preserve_fixed_values_and_construct_anonymous_instances_once() {
    BUILDS.set(0);
    let reactive = Reactive::new();
    let selected = definitions::Selections::builder()
        .width(4.0)
        .caption(String::from("hello"))
        .build(&reactive)
        .unwrap();
    assert_eq!(BUILDS.get(), 1);
    assert_eq!(selected.scaled_fixed(), Some(&8.0));
    assert_eq!(selected.extent_fixed(), Some(&4.0));
    assert_eq!(selected.constant_fixed(), Some(&3.0));
    assert_eq!(selected.text_fixed().map(String::as_str), Some("hello"));
    assert!(selected.path_fixed().unwrap().is_ok());
    assert!(selected.mask_fixed().unwrap().is_ok());
    assert!(selected.drawing_fixed().unwrap().is_ok());
}

#[test]
fn selected_native_and_xml_outputs_update_and_recover_without_reconstruction() {
    BUILDS.set(0);
    let reactive = Reactive::new();
    let width = reactive.create_state(4.0);
    let caption = reactive.create_state(String::from("hello"));
    let selected = definitions::Selections::builder()
        .width(width.value())
        .caption(caption.value())
        .build(&reactive)
        .unwrap();
    assert!(selected.scaled_fixed().is_none());
    assert!(selected.text_fixed().is_none());
    assert!(selected.path_fixed().is_none());
    assert_eq!(selected.constant_fixed(), Some(&3.0));
    let scaled = selected.scaled();
    let text = selected.text();
    let drawing = selected.drawing();
    let mask = selected.mask();
    let path = selected.path();
    let read = reactive.create_computed(move |cx| {
        (
            *cx.read(&scaled),
            cx.read(&text).cloned(),
            cx.read(&drawing).is_ok(),
            cx.read(&mask).is_ok(),
            cx.read(&path).is_ok(),
        )
    });
    assert_eq!(
        *read.snapshot().unwrap(),
        (8.0, "hello".into(), true, true, true)
    );
    width.set(-1.0).unwrap();
    assert_eq!(
        *read.snapshot().unwrap(),
        (-2.0, "hello".into(), false, false, false)
    );
    caption.set(String::from("updated")).unwrap();
    width.set(6.0).unwrap();
    assert_eq!(
        *read.snapshot().unwrap(),
        (12.0, "updated".into(), true, true, true)
    );
    assert_eq!(BUILDS.get(), 1);
}
