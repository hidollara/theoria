use crate::{
    component::{Component, ComponentContext, EventEmitter},
    input::pointer::PointerEvent,
};

use super::{Drag, DragRecognizer};

/// Component adapter that publishes drags recognized from pointer input.
pub struct DragGesture {
    recognizer: DragRecognizer,
    drags: EventEmitter<Drag>,
}

impl DragGesture {
    pub fn new(recognizer: DragRecognizer, drags: EventEmitter<Drag>) -> Self {
        Self { recognizer, drags }
    }
}

impl Component for DragGesture {
    type Event = PointerEvent;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        if let Some(drag) = self.recognizer.recognize(&event) {
            let _ = context.emit(&self.drags, drag);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        component::{
            Component, ComponentContext, ComponentRuntime, EventEmitter, TurnBudget, TurnOutcome,
        },
        geometry::Point,
        gesture::MovementThreshold,
        input::pointer::{
            PointerButton, PointerButtonChange, PointerButtons, PointerEvent, PointerId,
            PointerKind, PointerPhase,
        },
    };

    use super::*;
    use crate::gesture::DragPhase;

    struct PointerInput(EventEmitter<PointerEvent>);

    impl Component for PointerInput {
        type Event = PointerEvent;

        fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
            let _ = context.emit(&self.0, event);
        }
    }

    struct DragRecorder(Rc<RefCell<Vec<Drag>>>);

    impl Component for DragRecorder {
        type Event = Drag;

        fn receive(&mut self, event: Self::Event, _context: &mut ComponentContext<'_>) {
            self.0.borrow_mut().push(event);
        }
    }

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
        let runtime = ComponentRuntime::new();
        let received = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&received);
        let input = runtime.mount(move |context| {
            let (pointer_source, pointer_events) = context.event::<PointerEvent>();
            let (drag_source, drags) = context.event::<Drag>();
            let gesture = context.mount(move |_context| {
                DragGesture::new(
                    DragRecognizer::new(MovementThreshold::AxisAligned {
                        horizontal: 3.0,
                        vertical: 4.0,
                    })
                    .unwrap(),
                    drags,
                )
            });
            let recorder = context.mount(move |_context| DragRecorder(output));
            assert!(context.connect(&pointer_source, &gesture));
            assert!(context.connect(&drag_source, &recorder));
            PointerInput(pointer_events)
        });

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
            assert!(runtime.enqueue(&input, event));
        }

        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        let received = received.borrow();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0].phase, DragPhase::Started);
        assert_eq!(received[1].phase, DragPhase::Moved);
        assert_eq!(received[2].phase, DragPhase::Ended);
    }
}
