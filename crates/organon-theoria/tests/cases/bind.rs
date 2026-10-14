use std::cell::Cell;
use theoria_reactive::Reactive;

mod definitions {
    organon_theoria::library!(crate::definitions);
}

thread_local! { static EVALUATIONS: Cell<usize> = const { Cell::new(0) }; }
#[organon_theoria::implement]
impl Scale {
    #[output(name = "scaled")]
    fn scaled(input: Input<'_, Self>) -> f32 {
        EVALUATIONS.set(EVALUATIONS.get() + 1);
        *input.value() * 2.0
    }
    forward!(value => original);
}

#[test]
fn selections_preserve_fixed_values_and_construct_anonymous_instances_once() {
    EVALUATIONS.set(0);
    let reactive = Reactive::new();
    let selected = definitions::Selections::builder()
        .width(4.0)
        .caption(String::from("hello"))
        .build(&reactive)
        .unwrap();
    assert_eq!(EVALUATIONS.get(), 1);
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
    EVALUATIONS.set(0);
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
            *scaled.read(&organon_theoria::ReadContext::new(cx)),
            text.read(&organon_theoria::ReadContext::new(cx)).cloned(),
            drawing.read(&organon_theoria::ReadContext::new(cx)).is_ok(),
            mask.read(&organon_theoria::ReadContext::new(cx)).is_ok(),
            path.read(&organon_theoria::ReadContext::new(cx)).is_ok(),
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
    // Scale's output is inline: consumers may evaluate it independently.
    let evaluations = EVALUATIONS.get();
    assert!(evaluations > 1);
    read.snapshot().unwrap();
    assert_eq!(EVALUATIONS.get(), evaluations);
}
