use std::{
    cell::{Cell, RefCell},
    panic::{AssertUnwindSafe, catch_unwind},
    rc::Rc,
};

use theoria_component::UpdateContext;

use super::support::{DrainOutcome, HandlesEvent, TestRuntime, receive};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

struct Counter {
    value: theoria_reactive::State<i32>,
}

impl HandlesEvent for Counter {
    type Event = i32;

    fn handle(&mut self, event: i32, context: &mut UpdateContext<'_>) {
        context.modify(&self.value, |value| *value += event);
    }
}

#[test]
fn events_update_component_owned_state_before_effects_run() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let mut runtime = TestRuntime::new(|context| {
        let value = context.create_state(0);
        let observed_value = value.value();
        let effect = context.create_effect(move |context| {
            effect_observed
                .borrow_mut()
                .push(*context.read(&observed_value));
        });
        context.sync(&effect);
        receive(context, Counter { value })
    });
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);

    assert!(enqueue_root(&runtime, 2));
    assert!(enqueue_root(&runtime, 3));
    deliver_all(&mut runtime);
    assert_eq!(&*observed.borrow(), &[0, 5]);
}

#[test]
fn a_root_handler_can_capture_inferred_reactive_state() {
    let observed = Rc::new(RefCell::new(Vec::new()));
    let effect_observed = Rc::clone(&observed);
    let mut runtime = TestRuntime::new_handler(|context| {
        let count = context.create_state(0_u32);
        let count_value = count.value();
        let effect = context.create_effect(move |context| {
            effect_observed
                .borrow_mut()
                .push(*context.read(&count_value));
        });
        context.sync(&effect);

        move |increment: u32, context: &mut UpdateContext<'_>| {
            context.modify(&count, |count| *count += increment);
        }
    });

    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
    assert!(enqueue_root(&runtime, 3_u32));
    deliver_all(&mut runtime);

    assert_eq!(&*observed.borrow(), &[0, 3]);
}

#[test]
fn explicit_effect_requests_preserve_repetition() {
    let runs = Rc::new(Cell::new(0));
    let effect_runs = Rc::clone(&runs);
    let mut runtime = TestRuntime::new_handler(|context| {
        let effect = context.create_effect(move |_context| effect_runs.set(effect_runs.get() + 1));
        move |requests: usize, context: &mut UpdateContext<'_>| {
            for _ in 0..requests {
                context.request_effect(&effect);
            }
        }
    });

    assert!(enqueue_root(&runtime, 2));
    deliver_all(&mut runtime);
    assert_eq!(runs.get(), 2);
}

#[test]
fn a_synchronized_effect_can_belong_to_an_independent_reactive() {
    let foreign = theoria_reactive::Reactive::new();
    let runs = Rc::new(Cell::new(0));
    let count = Rc::clone(&runs);
    let effect = foreign.create_effect(move |_| count.set(count.get() + 1));
    let mut instance = TestRuntime::new(|cx| {
        cx.sync(&effect);
        cx.receive(|_: (), _: &mut UpdateContext<'_>| {})
    });
    assert_eq!(instance.run_until_idle(), DrainOutcome::Idle);
    assert_eq!(runs.get(), 1);
}

#[test]
fn requested_effects_run_through_their_own_reactives() {
    let foreign = theoria_reactive::Reactive::new();
    let foreign_runs = Rc::new(Cell::new(0));
    let foreign_count = Rc::clone(&foreign_runs);
    let foreign_effect = foreign.create_effect(move |_| foreign_count.set(foreign_count.get() + 1));
    let runs = Rc::new(Cell::new(0));
    let count = Rc::clone(&runs);
    let mut instance = TestRuntime::new(|cx| {
        let local = cx.create_effect(move |_| count.set(count.get() + 1));
        cx.receive(move |_: (), cx: &mut UpdateContext<'_>| {
            cx.request_effect(&foreign_effect);
            cx.request_effect(&local);
        })
    });
    assert!(instance.send(()));

    assert_eq!(instance.run_until_idle(), DrainOutcome::Idle);
    assert_eq!(foreign_runs.get(), 1);
    assert_eq!(runs.get(), 1);
}

#[test]
fn component_mutation_panics_for_foreign_state_without_invoking_modify() {
    let foreign = theoria_reactive::Reactive::new();
    let state = foreign.create_state(1);
    let state_reader = state.value();
    let observed = foreign.create_computed(move |cx| *cx.read(&state_reader));
    let target = state.clone();
    let update_invoked = Rc::new(Cell::new(false));
    let handler_update_invoked = Rc::clone(&update_invoked);
    let mut runtime = TestRuntime::new_handler(move |_| {
        move |_: (), cx: &mut UpdateContext<'_>| {
            cx.modify(&target, |value| {
                handler_update_invoked.set(true);
                *value += 1;
            });
        }
    });
    assert!(enqueue_root(&runtime, ()));
    assert!(catch_unwind(AssertUnwindSafe(|| deliver_all(&mut runtime))).is_err());
    assert!(!update_invoked.get());
    assert_eq!(*observed.snapshot().unwrap(), 1);
}
