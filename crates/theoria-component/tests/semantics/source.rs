use std::{cell::RefCell, rc::Rc};

use theoria_component::ComponentSink;
use theoria_event::{ChannelSink, EventSink, OwningSink};

use super::support::{DrainOutcome, TestDriver, TestRuntime};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

fn deliver_exactly<E: 'static>(runtime: &mut TestRuntime<E>, deliveries: usize) {
    assert_eq!(runtime.run_events(deliveries), DrainOutcome::Paused);
}

fn enqueue_root<E: 'static>(runtime: &TestRuntime<E>, event: E) -> bool {
    runtime.send(event)
}

struct RecordedU32(u32);

impl From<&u32> for RecordedU32 {
    fn from(value: &u32) -> Self {
        Self(*value)
    }
}

struct MoveOnlyEvent(String);

enum DynamicSourceCommand {
    Install,
    Publish(u32),
}

#[test]
fn a_component_can_create_a_source_while_handling_an_event() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let input = Rc::new(RefCell::new(None::<ChannelSink<u32>>));
        let handler_input = Rc::clone(&input);
        context.receive(
            move |command: DynamicSourceCommand, context| match command {
                DynamicSourceCommand::Install => {
                    let values = Rc::clone(&mounted_values);
                    let input = Rc::clone(&handler_input);
                    assert!(context.mount_with(move |child| {
                        let target = child.receive(move |value: u32, _context| {
                            values.borrow_mut().push(value);
                        });
                        let (source, sink) = child.event_source::<u32>();
                        assert!(child.connect(source, OwningSink::new(target.clone())));
                        input.replace(Some(sink));
                    }));
                }
                DynamicSourceCommand::Publish(value) => {
                    let input = handler_input.borrow();
                    let input = input.as_ref().expect("source installed");
                    assert!(context.send(input, value).is_ok());
                }
            },
        )
    });

    assert!(enqueue_root(&runtime, DynamicSourceCommand::Install));
    assert!(enqueue_root(&runtime, DynamicSourceCommand::Publish(7)));
    deliver_all(&mut runtime);

    assert_eq!(&*values.borrow(), &[7]);
}

#[test]
fn one_source_supports_repeated_connections_and_cloneable_inputs() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let input_values = Rc::clone(&values);
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let first_values = Rc::clone(&mounted_values);
        let first = context.receive(move |event: String, _context| {
            first_values.borrow_mut().push(event);
        });
        let second_values = Rc::clone(&mounted_values);
        let second = context.receive(move |event: String, _context| {
            second_values.borrow_mut().push(event);
        });
        let (source, input) = context.event_source::<Rc<MoveOnlyEvent>>();
        context.connect(
            source.clone(),
            OwningSink::new(
                first.contramap(|event: Rc<MoveOnlyEvent>| format!("first:{}", event.0)),
            ),
        );
        context.connect(
            source,
            OwningSink::new(
                second.contramap(|event: Rc<MoveOnlyEvent>| format!("second:{}", event.0)),
            ),
        );
        let first = input.clone();
        let second = input;

        context.receive(move |(use_second, event): (bool, MoveOnlyEvent), context| {
            assert!(
                input_values.borrow().is_empty(),
                "publishing must not invoke connected targets recursively"
            );
            let input = if use_second { &second } else { &first };
            assert!(context.send(input, Rc::new(event)).is_ok());
        })
    });

    assert!(enqueue_root(&runtime, (false, MoveOnlyEvent("one".into()))));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["first:one", "second:one"]);

    values.borrow_mut().clear();
    assert!(enqueue_root(&runtime, (true, MoveOnlyEvent("two".into()))));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["first:two", "second:two"]);
}

enum SelectedSource {
    Left(u32),
    Right(u32),
}

#[test]
fn sources_with_the_same_event_type_keep_distinct_identities() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let target = context.receive(move |event: String, _context| {
            mounted_values.borrow_mut().push(event);
        });
        let (left_source, left) = context.event_source::<u32>();
        let (right_source, right) = context.event_source::<u32>();
        context.connect(
            left_source,
            OwningSink::new(target.clone().contramap(|value| format!("left:{value}"))),
        );
        context.connect(
            right_source,
            OwningSink::new(target.contramap(|value| format!("right:{value}"))),
        );

        context.receive(move |event: SelectedSource, context| match event {
            SelectedSource::Left(value) => assert!(context.send(&left, value).is_ok()),
            SelectedSource::Right(value) => assert!(context.send(&right, value).is_ok()),
        })
    });

    enqueue_root(&runtime, SelectedSource::Right(2));
    enqueue_root(&runtime, SelectedSource::Left(1));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["right:2", "left:1"]);
}

#[test]
fn source_input_and_handler_delivery_are_separate_deliveries() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let target = context.receive(move |event: RecordedU32, _context| {
            mounted_values.borrow_mut().push(event.0);
        });
        let (source, input) = context.event_source::<u32>();
        context.connect(
            source,
            OwningSink::new(target.contramap(|value| RecordedU32::from(&value))),
        );
        context.receive(move |event: u32, context| {
            assert!(context.send(&input, event).is_ok());
        })
    });

    assert!(enqueue_root(&runtime, 7_u32));

    assert_eq!(runtime.run_events(1), DrainOutcome::Paused);
    assert!(
        values.borrow().is_empty(),
        "source input only queued publication"
    );

    assert_eq!(runtime.run_events(1), DrainOutcome::Paused);
    assert!(
        values.borrow().is_empty(),
        "source input only queued target"
    );

    assert_eq!(runtime.run_events(1), DrainOutcome::Paused);
    assert_eq!(&*values.borrow(), &[7]);
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

#[derive(Clone)]
struct CloneablePublication(String);

struct OwnedPublication(String);

impl From<CloneablePublication> for OwnedPublication {
    fn from(value: CloneablePublication) -> Self {
        Self(value.0)
    }
}

#[test]
fn an_owning_sink_makes_owned_conversion_explicit() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let mounted_values = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let target = context.receive(move |event: OwnedPublication, _context| {
            mounted_values.borrow_mut().push(event.0);
        });
        let (source, input) = context.event_source::<CloneablePublication>();
        context.connect(
            source,
            OwningSink::new(target.accepting::<CloneablePublication>()),
        );
        context.receive(move |event: CloneablePublication, context| {
            assert!(context.send(&input, event).is_ok());
        })
    });

    assert!(enqueue_root(&runtime, CloneablePublication("owned".into())));
    deliver_all(&mut runtime);
    assert_eq!(&*values.borrow(), &["owned"]);
}

enum CycleCommand {
    Start,
    Stop,
}

#[test]
fn source_cycles_can_be_paused_by_the_caller() {
    let mut runtime = TestRuntime::new(|context| {
        let (left_source, left_sink) = context.event_source::<u32>();
        let (right_source, right_sink) = context.event_source::<u32>();
        context.connect(left_source, OwningSink::new(right_sink.clone()));
        context.connect(right_source, OwningSink::new(left_sink.clone()));
        context.receive(move |event: CycleCommand, context| match event {
            CycleCommand::Start => assert!(context.send(&left_sink, 1_u32).is_ok()),
            CycleCommand::Stop => context.unmount(),
        })
    });

    assert!(enqueue_root(&runtime, CycleCommand::Start));
    deliver_exactly(&mut runtime, 1);
    deliver_exactly(&mut runtime, 8);

    assert!(enqueue_root(&runtime, CycleCommand::Stop));
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Finished);
}

#[test]
fn one_component_can_retain_multiple_typed_inputs() {
    struct Inputs {
        number: ComponentSink<u32>,
        text: ComponentSink<String>,
    }

    let numbers = Rc::new(RefCell::new(Vec::new()));
    let texts = Rc::new(RefCell::new(Vec::new()));
    let number_output = Rc::clone(&numbers);
    let text_output = Rc::clone(&texts);
    let inputs = Rc::new(RefCell::new(None));
    let mounted_inputs = Rc::clone(&inputs);
    let mut runtime = TestDriver::new(move |context| {
        mounted_inputs.replace(Some(Inputs {
            number: context.receive(move |value: u32, _context| {
                number_output.borrow_mut().push(value);
            }),
            text: context.receive(move |value: String, _context| {
                text_output.borrow_mut().push(value);
            }),
        }));
    });
    let Inputs { number, text } = inputs.borrow_mut().take().expect("test inputs");

    assert!(runtime.send(&number, 3_u32).is_ok());
    assert!(runtime.send(&text, String::from("three")).is_ok());
    assert_eq!(
        runtime.run_until_idle().expect("valid Effects"),
        DrainOutcome::Idle
    );
    assert_eq!(&*numbers.borrow(), &[3]);
    assert_eq!(&*texts.borrow(), &["three"]);
}
