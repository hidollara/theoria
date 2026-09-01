use crate::{
    component::{Component, ComponentContext, EventEmitter},
    input::pointer::PointerEvent,
};

use super::{Click, ClickRecognizer};

/// Component adapter that publishes clicks recognized from pointer input.
///
/// The output emitter is supplied by the component that composes the input
/// pipeline. This keeps event-source identity and lifetime separate from the
/// replaceable recognition algorithm.
pub struct ClickGesture {
    recognizer: ClickRecognizer,
    clicks: EventEmitter<Click>,
}

impl ClickGesture {
    pub fn new(recognizer: ClickRecognizer, clicks: EventEmitter<Click>) -> Self {
        Self { recognizer, clicks }
    }
}

impl Component for ClickGesture {
    type Event = PointerEvent;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        if let Some(click) = self.recognizer.recognize(&event) {
            let _ = context.emit(&self.clicks, click);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        component::{
            Component, ComponentContext, ComponentHandle, ComponentRuntime, EventEmitter,
            TurnBudget, TurnOutcome,
        },
        geometry::Point,
        gesture::MovementThreshold,
        input::pointer::{
            PointerButton, PointerButtonChange, PointerButtons, PointerEvent, PointerId,
            PointerKind, PointerPhase,
        },
    };

    use super::*;

    struct PointerInput {
        pointer_events: EventEmitter<PointerEvent>,
    }

    impl Component for PointerInput {
        type Event = PointerEvent;

        fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
            let _ = context.emit(&self.pointer_events, event);
        }
    }

    struct ClickRecorder(Rc<RefCell<Vec<Click>>>);

    impl Component for ClickRecorder {
        type Event = Click;

        fn receive(&mut self, event: Self::Event, _context: &mut ComponentContext<'_>) {
            self.0.borrow_mut().push(event);
        }
    }

    struct InputPipeline {
        input: ComponentHandle<PointerInput>,
    }

    impl Component for InputPipeline {
        type Event = PointerEvent;

        fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
            let _ = context.send(&self.input, event);
        }
    }

    fn mouse_event(change: PointerButtonChange, position: Point) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(1),
            kind: PointerKind::Mouse,
            position,
            is_primary: true,
            buttons: match change {
                PointerButtonChange::Pressed(button) => PointerButtons::empty().with(button),
                PointerButtonChange::Released(_) => PointerButtons::empty(),
            },
            phase: PointerPhase::Move,
            button_change: Some(change),
        }
    }

    #[test]
    fn pointer_source_drives_click_source_through_component_delivery() {
        let runtime = ComponentRuntime::new();
        let received = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&received);

        let pipeline = runtime.mount(move |context| {
            let (pointer_source, pointer_events) = context.event::<PointerEvent>();
            let (click_source, clicks) = context.event::<Click>();
            let gesture = context.mount(move |_context| {
                ClickGesture::new(
                    ClickRecognizer::new(MovementThreshold::Radial { radius: 4.0 }).unwrap(),
                    clicks,
                )
            });
            let recorder = context.mount(move |_context| ClickRecorder(output));

            assert!(context.connect(&pointer_source, &gesture));
            assert!(context.connect(&click_source, &recorder));

            let input = context.mount(move |_context| PointerInput { pointer_events });
            InputPipeline { input }
        });

        assert!(runtime.enqueue(
            &pipeline,
            mouse_event(
                PointerButtonChange::Pressed(PointerButton::Primary),
                Point::new(10.0, 20.0),
            ),
        ));
        assert!(runtime.enqueue(
            &pipeline,
            mouse_event(
                PointerButtonChange::Released(PointerButton::Primary),
                Point::new(12.0, 20.0),
            ),
        ));

        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        assert_eq!(received.borrow().len(), 1);
        assert_eq!(
            received.borrow()[0].release_position,
            Point::new(12.0, 20.0)
        );
    }
}
