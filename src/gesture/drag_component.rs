use crate::{
    component::{Component, MountContext},
    event::{ChannelSource, ErasedEventSink, EventSink, OwningSink},
    input::pointer::PointerEvent,
};

use super::{Drag, DragRecognizer};

/// Component adapter that sends recognized drags to a composed output Sink.
pub struct DragGesture {
    recognizer: DragRecognizer,
    pointers: ChannelSource<PointerEvent>,
    drags: ErasedEventSink<Drag>,
}

impl DragGesture {
    pub fn new<S>(
        recognizer: DragRecognizer,
        pointers: ChannelSource<PointerEvent>,
        drags: S,
    ) -> Self
    where
        S: EventSink<Drag> + 'static,
        S::Dispatch: 'static,
    {
        Self {
            recognizer,
            pointers,
            drags: ErasedEventSink::new(drags),
        }
    }
}

impl Component for DragGesture {
    fn mount(self, context: &mut MountContext<'_>) {
        let Self {
            mut recognizer,
            pointers,
            drags,
        } = self;
        let input = context.receive(move |event: PointerEvent, context| {
            if let Some(drag) = recognizer.recognize(&event) {
                let _ = context.send(&drags, drag);
            }
        });
        assert!(context.connect(pointers, OwningSink::new(input)));
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        component::MountContext,
        geometry::Point,
        input::pointer::{
            MovementThreshold, PointerButton, PointerButtonChange, PointerButtons, PointerEvent,
            PointerId, PointerKind, PointerPhase,
        },
        test_support::TestRuntime,
    };

    use super::*;
    use crate::gesture::DragPhase;

    fn mouse(position: Point, button_change: Option<PointerButtonChange>) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(1),
            kind: PointerKind::Mouse,
            position,
            is_primary: true,
            buttons: match button_change {
                Some(PointerButtonChange::Pressed(button)) => PointerButtons::empty().with(button),
                _ => PointerButtons::empty(),
            },
            phase: PointerPhase::Move,
            button_change,
        }
    }

    #[test]
    fn pointer_source_drives_a_complete_drag_through_component_delivery() {
        let received = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&received);
        let instance = TestRuntime::new(move |context: &mut MountContext<'_>| {
            let (pointer_source, pointer_events) = context.event_source::<PointerEvent>();
            let (drag_source, drags) = context.event_source::<Drag>();
            context.mount(DragGesture::new(
                DragRecognizer::new(MovementThreshold::AxisAligned {
                    horizontal: 3.0,
                    vertical: 4.0,
                })
                .unwrap(),
                pointer_source,
                drags,
            ));
            let recorder = context.receive(move |event: Drag, _context| {
                output.borrow_mut().push(event);
            });
            context.connect(drag_source, OwningSink::new(recorder));
            context.receive(move |event: PointerEvent, context| {
                let _ = context.send(&pointer_events, event);
            })
        });
        let input = instance.interface().clone();
        let mut runtime = instance.into_driver();

        for event in [
            mouse(
                Point::new(10.0, 20.0),
                Some(PointerButtonChange::Pressed(PointerButton::Primary)),
            ),
            mouse(Point::new(13.0, 24.0), None),
            mouse(Point::new(14.0, 24.0), None),
            mouse(Point::new(20.0, 30.0), None),
            mouse(
                Point::new(30.0, 40.0),
                Some(PointerButtonChange::Released(PointerButton::Primary)),
            ),
        ] {
            assert!(runtime.send(&input, event).is_ok());
        }

        runtime.run_until_idle().expect("valid Effects");
        let received = received.borrow();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0].phase, DragPhase::Started);
        assert_eq!(received[1].phase, DragPhase::Moved);
        assert_eq!(received[2].phase, DragPhase::Ended);
    }
}
