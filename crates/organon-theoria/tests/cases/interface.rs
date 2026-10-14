use organon_theoria::{Calculation, Source};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
use theoria_reactive::Reactive;

mod contracts {
    organon_theoria::library!(crate::contracts);
}
use contracts::{Echo, Meter};

#[test]
fn bind_and_provide_preserve_inline_reads_in_each_consumer() {
    let reactive = Reactive::new();
    let right = reactive.create_state(9.0);
    let implementation = reset_meter();
    let meter = Meter::builder()
        .choose(1.0)
        .left(3.0)
        .right(right.value())
        .build(&reactive)
        .unwrap();
    let view = contracts::MeterView::builder()
        .meter(meter)
        .build(&reactive)
        .unwrap();
    assert_eq!(implementation.inline_calls.get(), 1);
    assert_eq!(implementation.chosen_calls.get(), 1);
    assert_eq!(view.chosen_fixed(), Some(&3.0));
    assert_eq!(view.checked_fixed(), Some(&Ok(3.0)));
    assert!(view.meter_fixed().is_some());
    let native = view.uncached();
    let alias = view.alias();
    let read = reactive.create_computed(move |cx| {
        *native.read(&organon_theoria::ReadContext::new(cx))
            + *alias.read(&organon_theoria::ReadContext::new(cx))
    });
    assert_eq!(*read.snapshot().unwrap(), 36.0);
    assert_eq!(implementation.inline_calls.get(), 3);
    right.set(10.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 40.0);
    assert_eq!(implementation.inline_calls.get(), 5);
    let source = view.uncached();
    let twice = Calculation::new(move |cx| *source.read(cx) + *source.read(cx))
        .bind(&reactive, false)
        .unwrap();
    assert!(twice.as_fixed().is_none());
    assert_eq!(implementation.inline_calls.get(), 7);
}

mod xml {
    pub use crate::contracts::Forward;
}

#[derive(Clone, Default)]
struct MeterStats {
    chosen_calls: Rc<Cell<usize>>,
    inline_calls: Rc<Cell<usize>>,
}
thread_local! { static STATS: RefCell<MeterStats> = RefCell::new(MeterStats::default()); }
fn reset_meter() -> MeterStats {
    STATS.with(|s| {
        *s.borrow_mut() = MeterStats::default();
        s.borrow().clone()
    })
}
#[organon_theoria::implement]
impl Meter {
    #[output(name = "chosen")]
    fn chosen(input: Input<'_, Self>) -> f32 {
        STATS.with(|s| {
            let s = s.borrow();
            s.chosen_calls.set(s.chosen_calls.get() + 1);
        });
        if *input.choose() > 0.0 {
            *input.left()
        } else {
            *input.right()
        }
    }
    #[output(name = "constant")]
    fn constant() -> String {
        "constant".into()
    }
    #[output(name = "checked")]
    fn checked(input: Input<'_, Self>) -> Result<f32, &'static str> {
        let value = *input.left();
        if value < 0.0 {
            Err("negative")
        } else {
            Ok(value)
        }
    }
    #[output(name = "uncached")]
    fn uncached(input: Input<'_, Self>) -> f32 {
        STATS.with(|s| {
            let s = s.borrow();
            s.inline_calls.set(s.inline_calls.get() + 1);
        });
        *input.right() * 2.0
    }
}
#[organon_theoria::implement]
impl Echo {
    forward!(value => value);
}

#[test]
fn fixedness_uses_actual_reads_and_fixed_errors_remain_values() {
    let reactive = Reactive::new();
    let unused = reactive.create_state(9.0);
    let implementation = reset_meter();
    let meter = Meter::builder()
        .right(unused.value())
        .left(-3.0)
        .choose(1.0)
        .build(&reactive)
        .unwrap();
    assert_eq!(implementation.chosen_calls.get(), 1);
    assert_eq!(meter.chosen_fixed(), Some(&-3.0));
    assert_eq!(meter.checked_fixed(), Some(&Err("negative")));
    assert_eq!(meter.constant_fixed().map(String::as_str), Some("constant"));
    assert!(meter.uncached_fixed().is_none());
    let echo = Echo::builder()
        .value(meter.checked())
        .build(&reactive)
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
    let implementation = reset_meter();
    let meter = Meter::builder()
        .choose(choose.value())
        .left(left.value())
        .right(right.value())
        .build(&reactive)
        .unwrap();
    assert!(meter.chosen_fixed().is_none());
    let value = meter.chosen();
    let read =
        reactive.create_computed(move |cx| *value.read(&organon_theoria::ReadContext::new(cx)));
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
    let checked = reactive.create_computed(move |cx| {
        checked
            .read(&organon_theoria::ReadContext::new(cx))
            .cloned()
    });
    assert_eq!(*checked.snapshot().unwrap(), Err("negative"));
    left.set(7.0).unwrap();
    assert_eq!(*checked.snapshot().unwrap(), Ok(7.0));
}

#[test]
fn interface_policy_preserves_inline_computation_between_elements() {
    let reactive = Reactive::new();
    let right = reactive.create_state(9.0);
    let implementation = reset_meter();
    let meter = Meter::builder()
        .choose(1.0)
        .left(3.0)
        .right(right.value())
        .build(&reactive)
        .unwrap();
    assert_eq!(implementation.inline_calls.get(), 1);
    let source = meter.uncached();
    let twice = Calculation::new(move |cx| *source.read(cx) + *source.read(cx))
        .bind(&reactive, false)
        .unwrap();
    assert_eq!(implementation.inline_calls.get(), 3);
    let native = twice.clone();
    let read =
        reactive.create_computed(move |cx| *native.read(&organon_theoria::ReadContext::new(cx)));
    assert_eq!(*read.snapshot().unwrap(), 36.0);
    assert_eq!(implementation.inline_calls.get(), 3);
    right.set(10.0).unwrap();
    assert_eq!(*read.snapshot().unwrap(), 40.0);
    assert_eq!(implementation.inline_calls.get(), 5);
}

#[test]
fn missing_inputs_fail_before_implementation_or_evaluation() {
    let reactive = Reactive::new();
    let implementation = reset_meter();
    assert!(matches!(
        Meter::builder().build(&reactive),
        Err(organon_theoria::BuildError::MissingInput("choose"))
    ));
    assert_eq!(implementation.chosen_calls.get(), 0);
}

#[test]
fn sources_can_be_cloned_and_borrowed_without_cloning_their_value() {
    struct NotClone(u32);
    let source = Source::from(NotClone(4));
    let duplicate = source.clone();
    let value = Calculation::new(move |cx| duplicate.read(cx).0)
        .bind(&Reactive::new(), false)
        .unwrap();
    assert_eq!(value.as_fixed(), Some(&4));
}

#[test]
fn xml_and_rust_share_initial_evaluation_fixedness_and_inline_sources() {
    let reactive = Reactive::new();
    let right = reactive.create_state(9.0);
    let implementation = reset_meter();
    let meter = Meter::builder()
        .choose(1.0)
        .left(3.0)
        .right(right.value())
        .build(&reactive)
        .unwrap();
    let fixed = xml::Forward::builder()
        .value(meter.chosen())
        .build(&reactive)
        .unwrap();
    assert_eq!(fixed.forwarded_fixed(), Some(&3.0));
    assert_eq!(implementation.inline_calls.get(), 1);
    let dynamic = xml::Forward::builder()
        .value(meter.uncached())
        .build(&reactive)
        .unwrap();
    assert!(dynamic.forwarded_fixed().is_none());
    assert_eq!(implementation.inline_calls.get(), 2);
    let native = dynamic.forwarded();
    let read =
        reactive.create_computed(move |cx| *native.read(&organon_theoria::ReadContext::new(cx)));
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
                Err(organon_theoria::BuildError::Reactive(
                    theoria_reactive::ReactiveError::AccessConflict
                ))
            ));
        })
        .unwrap();
    assert!(xml::Forward::builder().value(1.0).build(&reactive).is_ok());
}
