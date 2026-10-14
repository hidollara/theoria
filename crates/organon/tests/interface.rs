use organon::{Implementation, Source, output};
use std::{cell::Cell, rc::Rc};
use theoria_reactive::Reactive;

mod contracts {
    organon::import!("tests/fixtures/native.organon.xml");
}
use contracts::{Echo, Input, Meter};

mod xml {
    organon::import!("tests/fixtures/forward.organon.xml");
}

#[derive(Default)]
struct MyMeter {
    chosen_calls: Rc<Cell<usize>>,
    inline_calls: Rc<Cell<usize>>,
    factor: f32,
}

impl Implementation<Meter> for MyMeter {
    fn build(&self, input: Input<Meter>) -> Meter {
        let choose = input.choose().clone();
        let left = input.left().clone();
        let right = input.right().clone();
        let checked = left.clone();
        let uncached = right.clone();
        let calls = self.chosen_calls.clone();
        let inline_calls = self.inline_calls.clone();
        let factor = self.factor;
        Meter {
            chosen: output(move |cx| {
                calls.set(calls.get() + 1);
                if *choose.read(cx) > 0.0 {
                    *left.read(cx)
                } else {
                    *right.read(cx)
                }
            }),
            constant: output(|_| "constant".to_owned()),
            checked: output(move |cx| {
                let value = *checked.read(cx);
                if value < 0.0 {
                    Err("negative")
                } else {
                    Ok(value)
                }
            }),
            uncached: output(move |cx| {
                inline_calls.set(inline_calls.get() + 1);
                *uncached.read(cx) * factor
            }),
        }
    }
}

struct MyEcho;
impl Implementation<Echo> for MyEcho {
    fn build(&self, input: Input<Echo>) -> Echo {
        let value = input.value().clone();
        Echo {
            value: output(move |cx| value.read(cx).into_cloned()),
        }
    }
}

#[test]
fn fixedness_uses_actual_reads_and_fixed_errors_remain_values() {
    let reactive = Reactive::new();
    let unused = reactive.create_state(9.0);
    let implementation = MyMeter {
        factor: 2.0,
        ..Default::default()
    };
    let meter = Meter::builder()
        .right(unused.value())
        .left(-3.0)
        .choose(1.0)
        .build(&reactive, &implementation)
        .unwrap();
    assert_eq!(implementation.chosen_calls.get(), 1);
    assert_eq!(meter.chosen_fixed(), Some(&-3.0));
    assert_eq!(meter.checked_fixed(), Some(&Err("negative")));
    assert_eq!(meter.constant_fixed().map(String::as_str), Some("constant"));
    assert!(meter.uncached_fixed().is_none());
    let echo = Echo::builder()
        .value(meter.checked_source())
        .build(&reactive, &MyEcho)
        .unwrap();
    assert_eq!(echo.value_fixed(), Some(&Err("negative")));
    unused.set(10.0).unwrap();
    assert_eq!(meter.chosen_fixed(), Some(&-3.0));
    assert_eq!(implementation.chosen_calls.get(), 1);
}

#[test]
fn conditional_dependencies_switch_and_native_outputs_reuse_the_initial_result() {
    let reactive = Reactive::new();
    let choose = reactive.create_state(1.0);
    let left = reactive.create_state(3.0);
    let right = reactive.create_state(9.0);
    let implementation = MyMeter::default();
    let meter = Meter::builder()
        .choose(choose.value())
        .left(left.value())
        .right(right.value())
        .build(&reactive, &implementation)
        .unwrap();
    assert!(meter.chosen_fixed().is_none());
    let value = meter.chosen();
    let read = reactive.create_computed(move |cx| *cx.read(&value));
    assert_eq!(*read.snapshot().unwrap(), 3.0);
    assert_eq!(implementation.chosen_calls.get(), 1);
    right.set(10.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 3.0);
    assert_eq!(implementation.chosen_calls.get(), 1);
    choose.set(0.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 10.0);
    assert_eq!(implementation.chosen_calls.get(), 2);
    left.set(-1.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 10.0);
    assert_eq!(implementation.chosen_calls.get(), 2);
    let checked = meter.checked();
    let checked = reactive.create_computed(move |cx| cx.read(&checked).cloned());
    assert_eq!(*checked.snapshot().unwrap(), Err("negative"));
    left.set(7.0).unwrap();
    assert_eq!(*checked.snapshot().unwrap(), Ok(7.0));
}

#[test]
fn interface_policy_preserves_inline_computation_between_elements() {
    let reactive = Reactive::new();
    let right = reactive.create_state(9.0);
    let implementation = MyMeter {
        factor: 2.0,
        ..Default::default()
    };
    let meter = Meter::builder()
        .choose(1.0)
        .left(3.0)
        .right(right.value())
        .build(&reactive, &implementation)
        .unwrap();
    assert_eq!(implementation.inline_calls.get(), 1);
    let source = meter.uncached_source();
    let twice = output(move |cx| *source.read(cx) + *source.read(cx))
        .bind(&reactive, false)
        .unwrap();
    assert_eq!(implementation.inline_calls.get(), 3);
    let native = twice.value();
    let read = reactive.create_computed(move |cx| *cx.read(&native));
    assert_eq!(*read.snapshot().unwrap(), 36.0);
    assert_eq!(implementation.inline_calls.get(), 3);
    right.set(10.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 40.0);
    assert_eq!(implementation.inline_calls.get(), 5);
}

#[test]
fn missing_inputs_fail_before_implementation_or_evaluation() {
    let reactive = Reactive::new();
    let implementation = MyMeter::default();
    assert!(matches!(
        Meter::builder().build(&reactive, &implementation),
        Err(organon::BuildError::MissingInput("choose"))
    ));
    assert_eq!(implementation.chosen_calls.get(), 0);
}

#[test]
fn sources_can_be_cloned_and_borrowed_without_cloning_their_value() {
    struct NotClone(u32);
    let source = Source::from(NotClone(4));
    let duplicate = source.clone();
    let value = output(move |cx| duplicate.read(cx).0)
        .bind(&Reactive::new(), false)
        .unwrap();
    assert_eq!(value.as_fixed(), Some(&4));
}

#[test]
fn xml_and_rust_share_initial_evaluation_fixedness_and_inline_sources() {
    let reactive = Reactive::new();
    let right = reactive.create_state(9.0);
    let implementation = MyMeter {
        factor: 2.0,
        ..Default::default()
    };
    let meter = Meter::builder()
        .choose(1.0)
        .left(3.0)
        .right(right.value())
        .build(&reactive, &implementation)
        .unwrap();
    let fixed = xml::Forward::builder()
        .value(meter.chosen_source())
        .build(&reactive)
        .unwrap();
    assert_eq!(fixed.forwarded_fixed(), Some(&3.0));
    assert_eq!(implementation.inline_calls.get(), 1);
    let dynamic = xml::Forward::builder()
        .value(meter.uncached_source())
        .build(&reactive)
        .unwrap();
    assert!(dynamic.forwarded_fixed().is_none());
    assert_eq!(implementation.inline_calls.get(), 2);
    let native = dynamic.forwarded();
    let read = reactive.create_computed(move |cx| *cx.read(&native));
    assert_eq!(*read.snapshot().unwrap(), 18.0);
    assert_eq!(implementation.inline_calls.get(), 2);
    right.set(10.0).unwrap();
    assert_eq!(implementation.inline_calls.get(), 2);
    assert_eq!(*read.snapshot().unwrap(), 20.0);
    assert_eq!(implementation.inline_calls.get(), 3);
}

#[test]
fn xml_build_reports_evaluation_access_conflicts() {
    let reactive = Reactive::new();
    reactive
        .mutate(|_| {
            assert!(matches!(
                xml::Forward::builder().value(1.0).build(&reactive),
                Err(organon::BuildError::Reactive(
                    theoria_reactive::ReactiveError::AccessConflict
                ))
            ));
        })
        .unwrap();
    assert!(xml::Forward::builder().value(1.0).build(&reactive).is_ok());
}
