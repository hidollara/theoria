mod generated {
    include!("generated/reactive.rs");
}

mod short_circuit {
    include!("generated/short_circuit.rs");
}

#[test]
fn fixed_errors_skip_unread_reactive_dependencies() {
    let reactive = Reactive::new();
    let reads = Rc::new(Cell::new(0));
    let calls = reads.clone();
    let paint = reactive.create_computed(move |_| {
        calls.set(calls.get() + 1);
        color()
    });
    let output = short_circuit::ShortCircuit::builder()
        .width(-1.0).z_color(paint.value()).build(&reactive).unwrap();
    assert!(output.drawing_fixed().unwrap().is_err());
    assert_eq!(reads.get(), 0);
    let drawing = output.drawing();
    let read = reactive.create_computed(move |cx| cx.read(&drawing).cloned());
    assert!(read.snapshot().unwrap().is_err());
    assert_eq!(reads.get(), 0);
}

use std::{cell::Cell, rc::Rc};
use theoria_canvas::{Canvas, CanvasNode, Color, CoverageNode, Opacity, color::Srgb};
use theoria_reactive::Reactive;

fn color() -> Color {
    Color::new(Srgb, [20_u8, 30, 40], Opacity::<u8>::ONE)
}
fn width(canvas: &Canvas) -> f32 {
    let CanvasNode::Draw(draw) = canvas.node() else {
        panic!()
    };
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    path.subpaths()[0].segments()[0].end().x
}

#[test]
fn fixed_sources_match_snapshot_evaluation_including_inline_exports() {
    let reactive = Reactive::new();
    let snapshot = generated::build(&generated::Inputs {
        width: 30.0,
        color: color(),
    })
    .unwrap();
    let output = generated::Reactive::builder()
        .width(30.0)
        .color(color())
        .build(&reactive)
        .unwrap();
    assert!(output.shape_fixed().is_some());
    assert!(output.direct_fixed().is_some());
    assert!(output.consumer_fixed().is_some());
    assert!(output.fixed_fixed().is_some());
    assert_eq!(output.extent_fixed(), Some(&30.0));
    assert_eq!(
        width(output.cached_fixed().unwrap().as_ref().unwrap()),
        width(&snapshot.cached)
    );
}

#[test]
fn construction_evaluates_once_and_updates_remain_lazy() {
    let reactive = Reactive::new();
    let state = reactive.create_state(30.0);
    let paint = reactive.create_state(color());
    let reads = Rc::new(Cell::new(0));
    let count = reads.clone();
    let source = state.value();
    let input = reactive.create_computed(move |cx| {
        count.set(count.get() + 1);
        *cx.read(&source)
    });
    let output = generated::Reactive::builder()
        .width(input.value())
        .color(paint.value())
        .build(&reactive)
        .unwrap();
    assert_eq!(reads.get(), 1);
    let cached = output.cached();
    let reader = reactive.create_computed(move |cx| {
        let a = cx.read(&cached).cloned().unwrap();
        let b = cx.read(&cached).cloned().unwrap();
        assert!(std::ptr::eq(a.node(), b.node()));
        width(&a)
    });
    assert_eq!(*reader.snapshot().unwrap(), 30.0);
    assert_eq!(reads.get(), 1);
    paint
        .set(Color::new(Srgb, [80_u8, 20, 10], Opacity::<u8>::ONE))
        .unwrap();
    assert_eq!(*reader.snapshot().unwrap(), 30.0);
    assert_eq!(reads.get(), 1);
    state.set(50.0).unwrap();
    assert_eq!(reads.get(), 1);
    assert_eq!(*reader.snapshot().unwrap(), 50.0);
    assert_eq!(reads.get(), 2);
}

#[test]
fn every_fixed_and_reactive_input_combination_tracks_per_output_dependencies() {
    for fixed_width in [false, true] {
        for fixed_color in [false, true] {
            let reactive = Reactive::new();
            let width_state = reactive.create_state(30.0);
            let color_state = reactive.create_state(color());
            let width_input: generated::Input<f32> = if fixed_width {
                30.0.into()
            } else {
                width_state.value().into()
            };
            let color_input: generated::Input<Color> = if fixed_color {
                color().into()
            } else {
                color_state.value().into()
            };
            let output = generated::Reactive::builder()
                .width(width_input)
                .color(color_input)
                .build(&reactive)
                .unwrap();
            assert_eq!(output.shape_fixed().is_some(), fixed_width);
            assert_eq!(output.cached_fixed().is_some(), fixed_width && fixed_color);
            assert_eq!(output.direct_fixed().is_some(), fixed_width && fixed_color);
            assert_eq!(
                output.consumer_fixed().is_some(),
                fixed_width && fixed_color
            );
            for canvas in [output.cached(), output.direct(), output.consumer()] {
                let reader = reactive.create_computed(move |cx| cx.read(&canvas).cloned());
                assert_eq!(width(reader.snapshot().unwrap().as_ref().unwrap()), 30.0);
                width_state.set(-1.0).unwrap();
                assert_eq!(reader.snapshot().unwrap().is_err(), !fixed_width);
                width_state.set(60.0).unwrap();
                assert_eq!(
                    width(reader.snapshot().unwrap().as_ref().unwrap()),
                    if fixed_width { 30.0 } else { 60.0 }
                );
                width_state.set(30.0).unwrap();
            }
        }
    }
}
