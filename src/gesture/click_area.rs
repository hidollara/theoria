use crate::{
    component::{ComponentContext, EventTarget},
    geometry::Point,
    interaction::HitRegion,
    transform::Projective2,
};

use super::{Click, area::GestureArea};

/// A click destination bound to local interaction geometry.
///
/// An area does not define stacking or routing policy. A component handling a
/// click can order a collection of areas, choose one, and ask that area to
/// forward the click in its local coordinate space.
#[derive(Clone, Debug)]
pub struct ClickArea(GestureArea<Click>);

impl ClickArea {
    pub fn new(target: EventTarget<Click>, region: HitRegion) -> Self {
        Self(GestureArea::new(target, region))
    }

    /// Places this local area in its containing click coordinate space.
    pub fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        self.0 = self.0.transformed(local_to_parent);
        self
    }

    /// Reports whether this active destination contains a point expressed in
    /// the area's parent coordinate space.
    pub fn contains(&self, point: Point, tolerance: f32) -> bool {
        self.0.contains(point, tolerance)
    }

    /// Forwards a click when both endpoints lie inside this area.
    ///
    /// The queued click contains positions expressed in the region's local
    /// coordinate space. `false` means the area could not accept the complete
    /// click or its destination is no longer active.
    pub fn try_forward(
        &self,
        click: &Click,
        tolerance: f32,
        context: &mut ComponentContext<'_>,
    ) -> bool {
        let Some(press_position) = self.0.hit(click.press_position, tolerance) else {
            return false;
        };
        let Some(release_position) = self.0.hit(click.release_position, tolerance) else {
            return false;
        };
        self.0.send(
            Click {
                press_position,
                release_position,
                ..*click
            },
            context,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        component::{
            Component, ComponentContext, ComponentRuntime, EventEmitter, TurnBudget, TurnOutcome,
        },
        input::pointer::{PointerButton, PointerId, PointerKind},
        math::Vector2,
        path::{FillRule, Path},
        reactive::Value,
        transform::Affine2,
    };

    use super::*;

    struct Input(EventEmitter<Click>);

    impl Component for Input {
        type Event = Click;

        fn receive(&mut self, event: Click, context: &mut ComponentContext<'_>) {
            assert!(context.emit(&self.0, event));
        }
    }

    struct Recorder(Rc<RefCell<Vec<Click>>>);

    impl Component for Recorder {
        type Event = Click;

        fn receive(&mut self, event: Click, _context: &mut ComponentContext<'_>) {
            self.0.borrow_mut().push(event);
        }
    }

    fn square(size: f32) -> HitRegion {
        HitRegion::fill(
            Path::polygon([
                Point::new(0.0, 0.0),
                Point::new(size, 0.0),
                Point::new(size, size),
                Point::new(0.0, size),
            ]),
            FillRule::NonZero,
        )
    }

    fn click(press_position: Point, release_position: Point) -> Click {
        Click {
            pointer_id: PointerId::new(1),
            pointer_kind: PointerKind::Mouse,
            is_primary_pointer: true,
            button: PointerButton::Primary,
            press_position,
            release_position,
        }
    }

    #[test]
    fn anonymous_handler_selects_frontmost_area_and_forwards_local_coordinates() {
        let runtime = ComponentRuntime::new();
        let back_events = Rc::new(RefCell::new(Vec::new()));
        let front_events = Rc::new(RefCell::new(Vec::new()));
        let back_output = Rc::clone(&back_events);
        let front_output = Rc::clone(&front_events);
        let input = runtime.mount(move |context| {
            let (clicks, emitter) = context.event::<Click>();
            let back = context.mount(move |_context| Recorder(back_output));
            let front = context.mount(move |_context| Recorder(front_output));
            let areas = Value::from(vec![
                ClickArea::new(back.event_target(), square(10.0)),
                ClickArea::new(front.event_target(), square(10.0))
                    .transformed(Affine2::from_translation(Vector2::new(2.0, 3.0))),
            ]);
            let handler = context.mount_handler(move |_context| {
                move |click: Click, context: &mut ComponentContext<'_>| {
                    let areas = context.read_current(&areas);
                    if let Some(area) = areas
                        .iter()
                        .rev()
                        .find(|area| area.contains(click.press_position, 0.01))
                    {
                        let _ = area.try_forward(&click, 0.01, context);
                    }
                }
            });
            assert!(context.connect(&clicks, &handler));
            Input(emitter)
        });

        assert!(runtime.enqueue(&input, click(Point::new(5.0, 7.0), Point::new(6.0, 8.0)),));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);

        assert!(back_events.borrow().is_empty());
        assert_eq!(front_events.borrow().len(), 1);
        assert_eq!(
            front_events.borrow()[0].press_position,
            Point::new(3.0, 4.0)
        );
        assert_eq!(
            front_events.borrow()[0].release_position,
            Point::new(4.0, 5.0)
        );
    }

    #[test]
    fn selected_press_area_absorbs_a_click_released_outside_it() {
        let runtime = ComponentRuntime::new();
        let back_events = Rc::new(RefCell::new(Vec::new()));
        let front_events = Rc::new(RefCell::new(Vec::new()));
        let back_output = Rc::clone(&back_events);
        let front_output = Rc::clone(&front_events);
        let input = runtime.mount(move |context| {
            let (clicks, emitter) = context.event::<Click>();
            let back = context.mount(move |_context| Recorder(back_output));
            let front = context.mount(move |_context| Recorder(front_output));
            let areas = Value::from(vec![
                ClickArea::new(back.event_target(), square(30.0)),
                ClickArea::new(front.event_target(), square(10.0)),
            ]);
            let handler = context.mount_handler(move |_context| {
                move |click: Click, context: &mut ComponentContext<'_>| {
                    let areas = context.read_current(&areas);
                    if let Some(area) = areas
                        .iter()
                        .rev()
                        .find(|area| area.contains(click.press_position, 0.01))
                    {
                        let _ = area.try_forward(&click, 0.01, context);
                    }
                }
            });
            assert!(context.connect(&clicks, &handler));
            Input(emitter)
        });

        assert!(runtime.enqueue(&input, click(Point::new(5.0, 5.0), Point::new(20.0, 5.0)),));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        assert!(front_events.borrow().is_empty());
        assert!(back_events.borrow().is_empty());
    }

    #[test]
    fn singular_transforms_cannot_produce_a_hit() {
        let runtime = ComponentRuntime::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&events);
        let input = runtime.mount(move |context| {
            let (clicks, emitter) = context.event::<Click>();
            let receiver = context.mount(move |_context| Recorder(output));
            let area = ClickArea::new(receiver.event_target(), square(10.0))
                .transformed(Affine2::from_scale(Vector2::new(0.0, 1.0)));
            let handler = context.mount_handler(move |_context| {
                move |click: Click, context: &mut ComponentContext<'_>| {
                    let _ = area.try_forward(&click, 0.01, context);
                }
            });
            assert!(context.connect(&clicks, &handler));
            Input(emitter)
        });

        assert!(runtime.enqueue(&input, click(Point::new(0.0, 5.0), Point::new(0.0, 5.0)),));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        assert!(events.borrow().is_empty());
    }
}
