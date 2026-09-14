use crate::{
    component::{Component, MountContext},
    event::{ChannelSource, EventSink, OwningSink},
    input::pointer::PointerEvent,
};

use super::{Click, ClickRecognizer, area::GestureSink};

/// Component adapter that publishes clicks recognized from pointer input.
///
/// The output Sink is supplied by the component that composes the input
/// pipeline. It may publish through an Event Source or deliver directly to
/// another Component, keeping routing separate from the recognition algorithm.
pub struct ClickGesture {
    recognizer: ClickRecognizer,
    pointers: ChannelSource<PointerEvent>,
    clicks: GestureSink<Click>,
}

impl ClickGesture {
    pub fn new<S>(
        recognizer: ClickRecognizer,
        pointers: ChannelSource<PointerEvent>,
        clicks: S,
    ) -> Self
    where
        S: 'static,
        for<'sink> &'sink S: EventSink<Click>,
        for<'sink> <&'sink S as EventSink<Click>>::Dispatch: 'static,
    {
        Self {
            recognizer,
            pointers,
            clicks: GestureSink::new(clicks),
        }
    }
}

impl Component for ClickGesture {
    fn mount(self, context: &mut MountContext<'_>) {
        let Self {
            mut recognizer,
            pointers,
            clicks,
        } = self;
        let input = context.receive(move |event: PointerEvent, context| {
            if let Some(click) = recognizer.recognize(&event) {
                let _ = context.send(&clicks, click);
            }
        });
        assert!(context.connect(pointers, OwningSink::new(input)));
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        event::EventTarget,
        geometry::Point,
        input::pointer::{
            MovementThreshold, PointerButton, PointerButtonChange, PointerButtons, PointerEvent,
            PointerId, PointerKind, PointerPhase,
        },
        test_support::TestRuntime,
    };

    use super::*;

    struct InputPipeline {
        input: EventTarget<PointerEvent>,
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
        let received = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&received);

        let instance = TestRuntime::new(move |context: &mut MountContext<'_>| {
            let (pointer_source, pointer_events) = context.event_source::<PointerEvent>();
            let (click_source, clicks) = context.event_source::<Click>();
            context.mount(ClickGesture::new(
                ClickRecognizer::new(MovementThreshold::Radial { radius: 4.0 }).unwrap(),
                pointer_source,
                clicks,
            ));
            let recorder = context.receive(move |event: Click, _context| {
                output.borrow_mut().push(event);
            });

            context.connect(click_source, OwningSink::new(recorder));

            let input = context.receive(move |event: PointerEvent, context| {
                let _ = context.send(&pointer_events, event);
            });
            InputPipeline { input }
        });
        let input = instance.interface().input.clone();
        let mut runtime = instance.into_driver();

        assert!(
            runtime
                .send(
                    &input,
                    mouse_event(
                        PointerButtonChange::Pressed(PointerButton::Primary),
                        Point::new(10.0, 20.0),
                    ),
                )
                .is_ok()
        );
        assert!(
            runtime
                .send(
                    &input,
                    mouse_event(
                        PointerButtonChange::Released(PointerButton::Primary),
                        Point::new(12.0, 20.0),
                    ),
                )
                .is_ok()
        );

        runtime.run_until_idle().expect("valid Effects");
        assert_eq!(received.borrow().len(), 1);
        assert_eq!(
            received.borrow()[0].release_position,
            Point::new(12.0, 20.0)
        );
    }
}
