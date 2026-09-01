use crate::{
    component::{ComponentContext, EventTarget},
    geometry::Point,
    interaction::HitRegion,
    transform::Projective2,
};

use super::Click;

/// A click destination bound to local interaction geometry.
///
/// An area does not define stacking or routing policy. A component handling a
/// click can order a collection of areas, choose one, and ask that area to
/// forward the click in its local coordinate space.
#[derive(Clone, Debug)]
pub struct ClickArea {
    target: EventTarget<Click>,
    region: HitRegion,
    local_to_parent: Projective2,
}

impl ClickArea {
    pub fn new(target: EventTarget<Click>, region: HitRegion) -> Self {
        Self {
            target,
            region,
            local_to_parent: Projective2::IDENTITY,
        }
    }

    /// Places this local area in its containing click coordinate space.
    pub fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        self.local_to_parent = self.local_to_parent.then(local_to_parent.into());
        self
    }

    /// Reports whether this active destination contains a point expressed in
    /// the area's parent coordinate space.
    pub fn contains(&self, point: Point, tolerance: f32) -> bool {
        self.local_hit(point, tolerance).is_some()
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
        let Some(press_position) = self.local_hit(click.press_position, tolerance) else {
            return false;
        };
        let Some(release_position) = self.local_hit(click.release_position, tolerance) else {
            return false;
        };
        context.send_to(
            &self.target,
            Click {
                press_position,
                release_position,
                ..*click
            },
        )
    }

    fn local_hit(&self, point: Point, tolerance: f32) -> Option<Point> {
        if !self.target.is_active()
            || !point.is_finite()
            || !tolerance.is_finite()
            || tolerance <= 0.0
        {
            return None;
        }

        let parent_to_local = self.local_to_parent.try_inverse()?;
        let local_position = parent_to_local.try_transform_point(point)?;
        let local_scale = maximum_scale_at(parent_to_local, point)?;
        let local_tolerance = tolerance * local_scale;
        if !local_tolerance.is_finite() || local_tolerance <= 0.0 {
            return None;
        }
        self.region
            .contains(local_position, local_tolerance)
            .then_some(local_position)
    }
}

fn maximum_scale_at(transform: Projective2, point: Point) -> Option<f32> {
    let [[a, b, c], [d, e, f], [g, h, i]] = transform.rows();
    let numerator_x = a * point.x + b * point.y + c;
    let numerator_y = d * point.x + e * point.y + f;
    let denominator = g * point.x + h * point.y + i;
    if !denominator.is_finite() || denominator.abs() <= f32::EPSILON {
        return None;
    }

    let denominator_squared = denominator * denominator;
    let j00 = (a * denominator - g * numerator_x) / denominator_squared;
    let j01 = (b * denominator - h * numerator_x) / denominator_squared;
    let j10 = (d * denominator - g * numerator_y) / denominator_squared;
    let j11 = (e * denominator - h * numerator_y) / denominator_squared;
    if ![j00, j01, j10, j11].into_iter().all(f32::is_finite) {
        return None;
    }

    let column0_squared = j00 * j00 + j10 * j10;
    let column1_squared = j01 * j01 + j11 * j11;
    let column_dot = j00 * j01 + j10 * j11;
    let discriminant =
        ((column0_squared - column1_squared).powi(2) + 4.0 * column_dot * column_dot).sqrt();
    let largest_eigenvalue = 0.5 * (column0_squared + column1_squared + discriminant);
    let scale = largest_eigenvalue.max(0.0).sqrt();
    scale.is_finite().then_some(scale)
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
