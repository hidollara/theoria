use crate::{
    component::UpdateContext, event::EventSink, geometry::Point, interaction::HitRegion,
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
    pub fn new(sink: impl Into<EventSink<Click>>, region: HitRegion) -> Self {
        Self(GestureArea::new(sink.into(), region))
    }

    /// Places this local area in its containing click coordinate space.
    pub fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        self.0 = self.0.transformed(local_to_parent);
        self
    }

    /// Reports whether this geometry contains a point expressed in the area's
    /// parent coordinate space.
    pub fn contains(&self, point: Point, tolerance: f32) -> bool {
        self.0.contains(point, tolerance)
    }

    /// Forwards a click when both endpoints lie inside this area.
    ///
    /// The queued click contains positions expressed in the region's local
    /// coordinate space. `false` means the area could not accept the complete
    /// click or its destination rejects delivery.
    pub fn try_forward(
        &self,
        click: &Click,
        tolerance: f32,
        context: &mut UpdateContext<'_>,
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
        input::pointer::{PointerButton, PointerId, PointerKind},
        math::Vector2,
        path::{FillRule, Path},
        test_support::TestRuntime,
        transform::Affine2,
    };

    use super::*;

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
    fn frontmost_matching_area_receives_local_click_coordinates() {
        let back_events = Rc::new(RefCell::new(Vec::new()));
        let front_events = Rc::new(RefCell::new(Vec::new()));
        let back_output = Rc::clone(&back_events);
        let front_output = Rc::clone(&front_events);
        let instance = TestRuntime::new(move |context| {
            let back = context.receive(move |event: Click, _context| {
                back_output.borrow_mut().push(event);
            });
            let front = context.receive(move |event: Click, _context| {
                front_output.borrow_mut().push(event);
            });
            let areas = [
                ClickArea::new(back, square(10.0)),
                ClickArea::new(front, square(10.0))
                    .transformed(Affine2::from_translation(Vector2::new(2.0, 3.0))),
            ];
            context.receive(move |click: Click, context| {
                if let Some(area) = areas
                    .iter()
                    .rev()
                    .find(|area| area.contains(click.press_position, 0.01))
                {
                    let _ = area.try_forward(&click, 0.01, context);
                }
            })
        });
        let input = instance.interface().clone();
        let mut runtime = instance.into_driver();

        assert!(
            runtime
                .send(&input, click(Point::new(5.0, 7.0), Point::new(6.0, 8.0)))
                .is_ok()
        );
        runtime.run_until_idle().expect("valid Effects");

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
        let back_events = Rc::new(RefCell::new(Vec::new()));
        let front_events = Rc::new(RefCell::new(Vec::new()));
        let back_output = Rc::clone(&back_events);
        let front_output = Rc::clone(&front_events);
        let instance = TestRuntime::new(move |context| {
            let back = context.receive(move |event: Click, _context| {
                back_output.borrow_mut().push(event);
            });
            let front = context.receive(move |event: Click, _context| {
                front_output.borrow_mut().push(event);
            });
            let areas = [
                ClickArea::new(back, square(30.0)),
                ClickArea::new(front, square(10.0)),
            ];
            context.receive(move |click: Click, context| {
                if let Some(area) = areas
                    .iter()
                    .rev()
                    .find(|area| area.contains(click.press_position, 0.01))
                {
                    let _ = area.try_forward(&click, 0.01, context);
                }
            })
        });
        let input = instance.interface().clone();
        let mut runtime = instance.into_driver();

        assert!(
            runtime
                .send(&input, click(Point::new(5.0, 5.0), Point::new(20.0, 5.0)))
                .is_ok()
        );
        runtime.run_until_idle().expect("valid Effects");
        assert!(front_events.borrow().is_empty());
        assert!(back_events.borrow().is_empty());
    }

    #[test]
    fn singular_transforms_cannot_produce_a_hit() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&events);
        let instance = TestRuntime::new(move |context| {
            let target = context.receive(move |event: Click, _context| {
                output.borrow_mut().push(event);
            });
            let area = ClickArea::new(target, square(10.0))
                .transformed(Affine2::from_scale(Vector2::new(0.0, 1.0)));
            context.receive(move |click: Click, context| {
                let _ = area.try_forward(&click, 0.01, context);
            })
        });
        let input = instance.interface().clone();
        let mut runtime = instance.into_driver();

        assert!(
            runtime
                .send(&input, click(Point::new(0.0, 5.0), Point::new(0.0, 5.0)))
                .is_ok()
        );
        runtime.run_until_idle().expect("valid Effects");
        assert!(events.borrow().is_empty());
    }
}
