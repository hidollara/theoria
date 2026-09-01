use crate::{
    component::{Component, ComponentContext, EventTarget},
    geometry::Point,
    interaction::HitRegion,
    transform::Projective2,
};

use super::Click;

struct Target {
    receiver: EventTarget<Click>,
    region: HitRegion,
    local_to_parent: Projective2,
}

/// Routes completed clicks to the frontmost matching click target.
///
/// The propagator owns the spatial lookup needed specifically by click
/// delivery. Its targets may be heterogeneous component types; only their
/// shared ability to receive [`Click`] is retained.
pub struct ClickPropagator {
    targets: Vec<Target>,
    tolerance: f32,
}

impl ClickPropagator {
    /// Creates an empty propagator when the curve-flattening tolerance is
    /// finite and positive.
    pub fn new(tolerance: f32) -> Option<Self> {
        if !tolerance.is_finite() || tolerance <= 0.0 {
            return None;
        }
        Some(Self {
            targets: Vec::new(),
            tolerance,
        })
    }

    /// Adds a target in front of all targets already present.
    pub fn with_target(self, receiver: EventTarget<Click>, region: HitRegion) -> Self {
        self.with_transformed_target(receiver, region, Projective2::IDENTITY)
    }

    /// Adds local interaction geometry placed in the propagator's coordinate
    /// space. The new target is in front of all targets already present.
    pub fn with_transformed_target(
        mut self,
        receiver: EventTarget<Click>,
        region: HitRegion,
        local_to_parent: impl Into<Projective2>,
    ) -> Self {
        self.targets.push(Target {
            receiver,
            region,
            local_to_parent: local_to_parent.into(),
        });
        self
    }

    /// Places all currently registered local targets in a parent coordinate
    /// space.
    pub fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        let local_to_parent = local_to_parent.into();
        for target in &mut self.targets {
            target.local_to_parent = target.local_to_parent.then(local_to_parent);
        }
        self
    }

    fn hit_test(&self, point: Point) -> Option<Hit> {
        if !point.is_finite() {
            return None;
        }

        self.targets
            .iter()
            .enumerate()
            .rev()
            .find_map(|(index, target)| {
                if !target.receiver.is_active() {
                    return None;
                }
                let parent_to_local = target.local_to_parent.try_inverse()?;
                let local_position = parent_to_local.try_transform_point(point)?;
                let local_scale = maximum_scale_at(parent_to_local, point)?;
                let local_tolerance = self.tolerance * local_scale;
                if !local_tolerance.is_finite() || local_tolerance <= 0.0 {
                    return None;
                }
                target
                    .region
                    .contains(local_position, local_tolerance)
                    .then_some(Hit {
                        index,
                        local_position,
                    })
            })
    }
}

impl Component for ClickPropagator {
    type Event = Click;

    fn receive(&mut self, click: Self::Event, context: &mut ComponentContext<'_>) {
        let Some(pressed) = self.hit_test(click.press_position) else {
            return;
        };
        let Some(released) = self.hit_test(click.release_position) else {
            return;
        };
        if pressed.index != released.index {
            return;
        }

        let target = &self.targets[pressed.index].receiver;
        let local_click = Click {
            press_position: pressed.local_position,
            release_position: released.local_position,
            ..click
        };
        let _ = context.send_to(target, local_click);
    }
}

#[derive(Clone, Copy)]
struct Hit {
    index: usize,
    local_position: Point,
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
        component::{ComponentRuntime, TurnBudget, TurnOutcome},
        input::pointer::{PointerButton, PointerId, PointerKind},
        math::Vector2,
        path::{FillRule, Path},
        transform::Affine2,
    };

    use super::*;

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
    fn frontmost_target_receives_local_coordinates() {
        let runtime = ComponentRuntime::new();
        let back_events = Rc::new(RefCell::new(Vec::new()));
        let front_events = Rc::new(RefCell::new(Vec::new()));
        let back_output = Rc::clone(&back_events);
        let front_output = Rc::clone(&front_events);
        let propagator = runtime.mount(move |context| {
            let back = context.mount(move |_context| Recorder(back_output));
            let front = context.mount(move |_context| Recorder(front_output));
            ClickPropagator::new(0.01)
                .unwrap()
                .with_target(back.event_target(), square(10.0))
                .with_transformed_target(
                    front.event_target(),
                    square(10.0),
                    Affine2::from_translation(Vector2::new(2.0, 3.0)),
                )
        });

        assert!(runtime.enqueue(
            &propagator,
            click(Point::new(5.0, 7.0), Point::new(6.0, 8.0)),
        ));
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
    fn press_and_release_must_hit_the_same_target() {
        let runtime = ComponentRuntime::new();
        let left_events = Rc::new(RefCell::new(Vec::new()));
        let right_events = Rc::new(RefCell::new(Vec::new()));
        let left_output = Rc::clone(&left_events);
        let right_output = Rc::clone(&right_events);
        let propagator = runtime.mount(move |context| {
            let left = context.mount(move |_context| Recorder(left_output));
            let right = context.mount(move |_context| Recorder(right_output));
            ClickPropagator::new(0.01)
                .unwrap()
                .with_target(left.event_target(), square(10.0))
                .with_transformed_target(
                    right.event_target(),
                    square(10.0),
                    Affine2::from_translation(Vector2::new(20.0, 0.0)),
                )
        });

        assert!(runtime.enqueue(
            &propagator,
            click(Point::new(5.0, 5.0), Point::new(25.0, 5.0)),
        ));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        assert!(left_events.borrow().is_empty());
        assert!(right_events.borrow().is_empty());
    }

    #[test]
    fn singular_transforms_cannot_produce_a_hit() {
        let runtime = ComponentRuntime::new();
        let events = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&events);
        let propagator = runtime.mount(move |context| {
            let receiver = context.mount(move |_context| Recorder(output));
            ClickPropagator::new(0.01).unwrap().with_transformed_target(
                receiver.event_target(),
                square(10.0),
                Affine2::from_scale(Vector2::new(0.0, 1.0)),
            )
        });

        assert!(runtime.enqueue(
            &propagator,
            click(Point::new(0.0, 5.0), Point::new(0.0, 5.0)),
        ));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
        assert!(events.borrow().is_empty());
    }
}
