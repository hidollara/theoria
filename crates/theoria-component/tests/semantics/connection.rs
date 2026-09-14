use std::{cell::Cell, cell::RefCell, rc::Rc};

use theoria_event::{Channel, ChannelSink, OwningSink};

use super::support::{DrainOutcome, TestRuntime};

fn deliver_all<E: 'static>(runtime: &mut TestRuntime<E>) {
    assert_eq!(runtime.run_until_idle(), DrainOutcome::Idle);
}

enum ConnectionCommand {
    Disconnect,
    Publish(u32),
}

#[test]
fn unmounting_a_connection_owner_disconnects_it() {
    let values = Rc::new(RefCell::new(Vec::new()));
    let output = Rc::clone(&values);
    let mut runtime = TestRuntime::new(move |context| {
        let (values_source, values_input) = context.event_source::<u32>();
        let (disconnect_source, disconnect_input) = context.event_source::<()>();
        let target = context.receive(move |value: u32, _context| {
            output.borrow_mut().push(value);
        });

        context.mount_with(move |child| {
            assert!(child.connect(values_source, OwningSink::new(target.clone())));
            let disconnect = child.receive(|(): (), context| context.unmount());
            assert!(child.connect(disconnect_source, OwningSink::new(disconnect)));
        });

        context.receive(move |command, context| match command {
            ConnectionCommand::Disconnect => assert!(context.send(&disconnect_input, ()).is_ok()),
            ConnectionCommand::Publish(value) => {
                assert!(context.send(&values_input, value).is_ok())
            }
        })
    });

    assert!(runtime.send(ConnectionCommand::Disconnect));
    deliver_all(&mut runtime);
    assert!(runtime.send(ConnectionCommand::Publish(7)));
    deliver_all(&mut runtime);
    assert!(values.borrow().is_empty());
}

#[test]
fn mounting_rejects_a_source_whose_channel_was_dropped() {
    let channel = Channel::<u32>::new();
    let source = channel.source();
    drop(channel);

    let connected = Rc::new(Cell::new(true));
    let mounted_connected = Rc::clone(&connected);
    let _runtime = TestRuntime::new(move |context| {
        let target = context.receive(|_: u32, _context| {});
        mounted_connected.set(context.connect(source, OwningSink::new(target)));
        context.receive(|(): (), _context| {})
    });

    assert!(!connected.get());
}

#[test]
fn dynamically_mounted_component_rejects_a_dropped_source() {
    let channel = Channel::<u32>::new();
    let source = channel.source();
    drop(channel);

    let connected = Rc::new(Cell::new(None));
    let handled_connected = Rc::clone(&connected);
    let mut source = Some(source);
    let mut runtime = TestRuntime::new(move |context| {
        context.receive(move |(): (), context| {
            let source = source.take().expect("test mounts one child");
            let connected = Rc::clone(&handled_connected);
            assert!(context.mount_with(move |child| {
                let target = child.receive(|_: u32, _context| {});
                connected.set(Some(child.connect(source, OwningSink::new(target))));
            }));
        })
    });

    assert!(runtime.send(()));
    deliver_all(&mut runtime);
    assert_eq!(connected.get(), Some(false));
}

enum ChannelOwnerCommand {
    Unmount,
    Probe(u32),
}

#[test]
fn a_component_event_input_does_not_keep_its_channel_owner_component_alive() {
    let exported_input = Rc::new(RefCell::new(None::<ChannelSink<u32>>));
    let input_output = Rc::clone(&exported_input);
    let result = Rc::new(Cell::new(None));
    let probe_result = Rc::clone(&result);

    let mut runtime = TestRuntime::new(move |context| {
        let (unmount_source, unmount_input) = context.event_source::<()>();
        let (probe_source, probe_input) = context.event_source::<u32>();

        context.mount_with(move |channel_owner| {
            let (_source, input) = channel_owner.event_source::<u32>();
            input_output.replace(Some(input));
            let unmount = channel_owner.receive(|(): (), context| context.unmount());
            assert!(channel_owner.connect(unmount_source, OwningSink::new(unmount)));
        });

        let input = exported_input
            .borrow_mut()
            .take()
            .expect("channel-owning component exports its non-owning input for this test");
        context.mount_with(move |probe| {
            let target = probe.receive(move |value: u32, context| {
                probe_result.set(Some(context.send(&input, value).is_ok()));
            });
            assert!(probe.connect(probe_source, OwningSink::new(target)));
        });

        context.receive(move |command, context| match command {
            ChannelOwnerCommand::Unmount => assert!(context.send(&unmount_input, ()).is_ok()),
            ChannelOwnerCommand::Probe(value) => {
                assert!(context.send(&probe_input, value).is_ok())
            }
        })
    });

    assert!(runtime.send(ChannelOwnerCommand::Unmount));
    deliver_all(&mut runtime);
    assert!(runtime.send(ChannelOwnerCommand::Probe(1)));
    deliver_all(&mut runtime);
    assert_eq!(result.get(), Some(true));
}
