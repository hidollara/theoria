use std::{cell::Cell, rc::Rc};
use theoria_reactive::Reactive;

mod definitions {
    organon_theoria::library!(crate::definitions);
}

#[organon_theoria::implement]
impl Bridge {
    #[output(name = "item")]
    fn item(input: Input<'_, Self>) -> Result<definitions::Button, &'static str> {
        if *input.ready() > 0.0 {
            Ok(input.item().into_cloned())
        } else {
            Err("not ready")
        }
    }
}

fn button(
    reactive: &Reactive,
    width: impl Into<organon_theoria::Source<f32>>,
) -> definitions::Button {
    let tile = definitions::Tile::builder()
        .width(width)
        .build(reactive)
        .unwrap();
    definitions::Button::builder()
        .tile(tile)
        .build(reactive)
        .unwrap()
}

#[test]
fn fixed_elements_can_be_provided_aliased_and_passed_to_other_elements() {
    let reactive = Reactive::new();
    let consumer = definitions::Consumer::builder()
        .button(button(&reactive, 4.0))
        .build(&reactive)
        .unwrap();
    assert_eq!(consumer.extent_fixed(), Some(&4.0));
    assert_eq!(consumer.attribute_extent_fixed(), Some(&4.0));
    assert_eq!(consumer.forward_extent_fixed(), Some(&4.0));
    assert_eq!(consumer.icon_fixed().unwrap().extent_fixed(), Some(&4.0));
    assert_eq!(consumer.copy_fixed().unwrap().extent_fixed(), Some(&4.0));
    assert!(consumer.button_copy_fixed().is_some());
    assert!(consumer.path_fixed().unwrap().is_ok());
    // An Element-valued output is directly usable as another generated input.
    let from_output = definitions::Consumer::builder()
        .button(consumer.button_copy())
        .build(&reactive)
        .unwrap();
    let extent = from_output.extent();
    assert_eq!(
        *reactive
            .create_computed(move |cx| *extent.read(&organon_theoria::ReadContext::new(cx)))
            .snapshot()
            .unwrap(),
        4.0
    );
}

#[test]
fn a_fixed_element_does_not_freeze_its_reactive_outputs() {
    let reactive = Reactive::new();
    let width = reactive.create_state(4.0);
    let consumer = definitions::Consumer::builder()
        .button(button(&reactive, width.value()))
        .build(&reactive)
        .unwrap();
    assert!(consumer.button_copy_fixed().is_some());
    assert!(consumer.icon_fixed().is_some());
    assert!(consumer.extent_fixed().is_none());
    assert!(consumer.attribute_extent_fixed().is_none());
    let extent = consumer.extent();
    let attribute = consumer.attribute_extent();
    let forwarded = consumer.forward_extent();
    let path = consumer.path();
    let read = reactive.create_computed(move |cx| {
        (
            *extent.read(&organon_theoria::ReadContext::new(cx)),
            *attribute.read(&organon_theoria::ReadContext::new(cx)),
            *forwarded.read(&organon_theoria::ReadContext::new(cx)),
            path.read(&organon_theoria::ReadContext::new(cx)).is_ok(),
        )
    });
    assert_eq!(*read.snapshot().unwrap(), (4.0, 4.0, 4.0, true));
    width.set(-1.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (-1.0, -1.0, -1.0, false));
    width.set(7.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (7.0, 7.0, 7.0, true));
}

#[test]
fn replacing_an_element_switches_dependencies_to_the_new_child() {
    let reactive = Reactive::new();
    let old_width = reactive.create_state(4.0);
    let new_width = reactive.create_state(8.0);
    let selected = reactive.create_state(button(&reactive, old_width.value()));
    let consumer = definitions::BoundConsumer::builder()
        .button(selected.value())
        .build(&reactive)
        .unwrap();
    assert!(consumer.button_copy_fixed().is_none());
    assert!(consumer.icon_fixed().is_none());
    let extent = consumer.extent();
    let attribute = consumer.attribute_extent();
    let evaluations = Rc::new(Cell::new(0));
    let count = evaluations.clone();
    let read = reactive.create_computed(move |cx| {
        count.set(count.get() + 1);
        (
            *extent.read(&organon_theoria::ReadContext::new(cx)),
            *attribute.read(&organon_theoria::ReadContext::new(cx)),
        )
    });
    assert_eq!(*read.snapshot().unwrap(), (4.0, 4.0));
    selected.set(button(&reactive, new_width.value())).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (8.0, 8.0));
    let before = evaluations.get();
    old_width.set(100.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (8.0, 8.0));
    assert_eq!(evaluations.get(), before);
    new_width.set(9.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (9.0, 9.0));
}

#[test]
fn native_element_ports_preserve_result_errors_and_recover_without_rebuilding() {
    let reactive = Reactive::new();
    let ready = reactive.create_state(0.0);
    let width = reactive.create_state(4.0);
    let consumer = definitions::NativeConsumer::builder()
        .button(button(&reactive, width.value()))
        .ready(ready.value())
        .build(&reactive)
        .unwrap();
    let bridge = consumer.bridge_fixed().unwrap().clone();
    let forward = definitions::NativeForward::builder()
        .bridge(bridge)
        .build(&reactive)
        .unwrap();
    assert!(forward.same_fixed().is_some());
    assert!(consumer.extent_fixed().is_none());
    let extent = consumer.extent();
    let forwarded = forward.extent();
    let path = consumer.path();
    let read = reactive.create_computed(move |cx| {
        (
            extent.read(&organon_theoria::ReadContext::new(cx)).cloned(),
            forwarded
                .read(&organon_theoria::ReadContext::new(cx))
                .cloned(),
            path.read(&organon_theoria::ReadContext::new(cx)).is_ok(),
        )
    });
    assert_eq!(
        *read.snapshot().unwrap(),
        (Err("not ready"), Err("not ready"), false)
    );
    ready.set(1.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (Ok(4.0), Ok(4.0), true));
    width.set(-1.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), (Ok(-1.0), Ok(-1.0), false));
    ready.set(0.0).unwrap();
    assert_eq!(
        *read.snapshot().unwrap(),
        (Err("not ready"), Err("not ready"), false)
    );
}
