mod drawing {
    include!("generated/reactive.rs");
}
mod label {
    include!("generated/label.rs");
}
mod panel {
    include!("generated/panel.rs");
}

use theoria_canvas::{Canvas, Color, Opacity, color::Srgb};
use theoria_reactive::Reactive;

fn color() -> Color {
    Color::new(Srgb, [20_u8, 30, 40], Opacity::<u8>::ONE)
}

#[test]
fn partial_configuration_order_and_per_output_fixedness() {
    let reactive = Reactive::new();
    let paint = reactive.create_state(color());
    let base: drawing::Builder<drawing::Reactive> =
        drawing::Reactive::builder().color(paint.value());
    assert!(base.clone().build(&reactive).unwrap_err().to_string().contains("width"));
    let element = base.clone().width(30.0).build(&reactive).unwrap();
    let other = base.width(60.0).build(&reactive).unwrap();
    assert_eq!(element.extent_fixed(), Some(&30.0));
    assert_eq!(other.extent_fixed(), Some(&60.0));
    assert!(element.shape_fixed().unwrap().is_ok());
    assert!(element.cached_fixed().is_none());
    assert!(element.direct_fixed().is_none());
    assert!(element.fixed_fixed().is_some());
    // Both fixed and dynamic outputs expose ordinary native handles. Fallibility
    // is a property of the expression, independent of whether it is fixed.
    let _: theoria_reactive::Value<f32> = element.extent();
    let _: theoria_reactive::Value<Canvas> = element.fixed();
    let _: theoria_reactive::Value<Result<Canvas, &'static str>> = element.cached();

    let shape = element.shape();
    let shape_reader = reactive.create_computed(move |cx| cx.read(&shape).cloned());
    let before = shape_reader.snapshot().unwrap().as_ref().unwrap().clone();
    let a = element.direct();
    let b = element.direct();
    let drawing_reader = reactive
        .create_computed(move |cx| (cx.read(&a).cloned().unwrap(), cx.read(&b).cloned().unwrap()));
    let first = drawing_reader.snapshot().unwrap();
    assert!(std::ptr::eq(first.0.node(), first.1.node()));
    paint
        .set(Color::new(Srgb, [80_u8, 90, 10], Opacity::<u8>::ONE))
        .unwrap();
    let second = drawing_reader.snapshot().unwrap();
    assert!(!std::ptr::eq(first.0.node(), second.0.node()));
    assert_eq!(&before, shape_reader.snapshot().unwrap().as_ref().unwrap());
}

fn label_builder() -> label::Builder<label::Label> {
    label::Label::builder()
        .fonts(vec![
            theoria_text::font::Font::from_bytes(font_test_data::AHEM).unwrap(),
        ])
        .label("ABC")
        .width(200.0)
        .ink(color())
}

#[test]
fn text_evaluation_errors_are_values_including_fixed_errors() {
    let reactive = Reactive::new();
    let fixed = label_builder().font_size(-1.0).build(&reactive).unwrap();
    assert!(fixed.drawing_fixed().unwrap().is_err());
    let value = fixed.drawing();
    let read = reactive.create_computed(move |cx| cx.read(&value).cloned());
    assert!(read.snapshot().unwrap().is_err());

    let size = reactive.create_state(-1.0);
    let dynamic = label_builder()
        .font_size(size.value())
        .build(&reactive)
        .unwrap();
    assert!(dynamic.drawing_fixed().is_none());
    let value = dynamic.drawing();
    let read = reactive.create_computed(move |cx| cx.read(&value).cloned());
    assert!(read.snapshot().unwrap().is_err());
    size.set(20.0).unwrap();
    assert!(read.snapshot().unwrap().is_ok());
    size.set(f32::NAN).unwrap();
    assert!(read.snapshot().unwrap().is_err());
    size.set(16.0).unwrap();
    assert!(read.snapshot().unwrap().is_ok());
}

#[test]
fn failed_output_does_not_prevent_unrelated_fixed_outputs() {
    let reactive = Reactive::new();
    let element = drawing::Reactive::builder()
        .width(-1.0)
        .color(color())
        .build(&reactive)
        .unwrap();
    assert_eq!(element.extent_fixed(), Some(&-1.0));
    assert!(element.shape_fixed().unwrap().is_err());
    assert!(element.cached_fixed().unwrap().is_err());
    assert!(element.direct_fixed().unwrap().is_err());
    assert!(element.fixed_fixed().is_some());
}

#[test]
fn separately_generated_elements_connect_through_native_values() {
    let reactive = Reactive::new();
    let width = reactive.create_state(30.0);
    let source = drawing::Reactive::builder()
        .width(width.value())
        .color(color())
        .build(&reactive)
        .unwrap();
    let target = panel::Panel::builder()
        .width(source.extent())
        .fill(color())
        .mark(source.fixed())
        .build(&reactive)
        .unwrap();
    let canvas = target.drawing();
    let read = reactive.create_computed(move |cx| cx.read(&canvas).cloned());
    let before = read.snapshot().unwrap().as_ref().unwrap().clone();
    width.set(60.0).unwrap();
    let after = read.snapshot().unwrap().as_ref().unwrap().clone();
    assert!(!std::ptr::eq(before.node(), after.node()));
    width.set(-1.0).unwrap();
    assert!(read.snapshot().unwrap().is_err());
}

#[test]
fn component_mount_builds_in_the_existing_instance() {
    use theoria_component::ComponentInstance;
    let queue = theoria_event::DispatchQueue::new();
    let mut exported = None;
    let mut width = None;
    let _instance = ComponentInstance::new(queue.dispatcher(), |cx| {
        let state = cx.create_state(30.0);
        let component = drawing::Reactive::builder()
            .width(state.value())
            .color(color())
            .component(|result| exported = Some(result.unwrap()));
        cx.mount(component);
        let element = exported.as_ref().unwrap();
        let value = element.extent();
        width = Some((state, cx.create_computed(move |cx| *cx.read(&value))));
        assert!(element.extent_fixed().is_none());
    });
    let (state, read) = width.unwrap();
    assert_eq!(*read.snapshot().unwrap(), 30.0);
    state.set(42.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 42.0);
}
