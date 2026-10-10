#![cfg(target_os = "windows")]

use std::{cell::Cell, rc::Rc};

use theoria_event::OwningSink;
use theoria_runtime::event_loop::ComponentParticipant;

#[test]
fn cross_thread_events_are_published_through_the_native_pump() {
    let mut event_loop = theoria_windows::event_loop();
    let (bridge, sender) = event_loop.handle().bridge::<u32>().unwrap();
    let received = Rc::new(Cell::new(None));
    let output = Rc::clone(&received);
    let (_participant, ()) = ComponentParticipant::mount(|context| {
        let handler = context.receive(move |event: u32, _| output.set(Some(event)));
        assert!(context.connect_inline(bridge.source(), OwningSink::new(handler)));
    });
    std::thread::spawn(move || sender.send(42))
        .join()
        .unwrap()
        .unwrap();
    event_loop.run().unwrap();
    assert_eq!(received.get(), Some(42));
}
