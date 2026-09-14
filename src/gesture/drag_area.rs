use crate::{
    component::UpdateContext, event::EventTarget, interaction::HitRegion, transform::Projective2,
};

use super::{Drag, DragPhase, area::GestureArea};

/// A drag destination bound to local interaction geometry.
///
/// An owner selects an area when a drag starts and retains that area for the
/// rest of the drag. [`Self::forward`] deliberately does not hit-test later
/// positions, so a captured drag continues outside its original region.
#[derive(Clone, Debug)]
pub struct DragArea(GestureArea<Drag>);

impl DragArea {
    pub fn new(sink: impl Into<EventTarget<Drag>>, region: HitRegion) -> Self {
        Self(GestureArea::new(sink.into(), region))
    }

    pub fn transformed(mut self, local_to_parent: impl Into<Projective2>) -> Self {
        self.0 = self.0.transformed(local_to_parent);
        self
    }

    /// Reports whether the origin of a newly started drag lies in this area.
    pub fn accepts_start(&self, drag: &Drag, tolerance: f32) -> bool {
        drag.phase == DragPhase::Started && self.0.contains(drag.origin, tolerance)
    }

    /// Forwards one captured drag in this area's local coordinate space.
    pub fn forward(&self, drag: &Drag, context: &mut UpdateContext<'_>) -> bool {
        let Some(origin) = self.0.map_point(drag.origin) else {
            return false;
        };
        let Some(previous_position) = self.0.map_point(drag.previous_position) else {
            return false;
        };
        let Some(position) = self.0.map_point(drag.position) else {
            return false;
        };
        self.0.send(
            Drag {
                origin,
                previous_position,
                position,
                ..*drag
            },
            context,
        )
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{
        component::UpdateContext,
        geometry::Point,
        input::pointer::{PointerButton, PointerId, PointerKind},
        path::{FillRule, Path},
        test_support::TestRuntime,
        transform::Affine2,
    };

    use super::*;
    use crate::math::Vector2;

    struct Capture {
        areas: Vec<DragArea>,
        captured: Option<DragArea>,
    }

    impl Capture {
        fn handle(&mut self, drag: Drag, context: &mut UpdateContext<'_>) {
            match drag.phase {
                DragPhase::Started => {
                    self.captured = self
                        .areas
                        .iter()
                        .rev()
                        .find(|area| area.accepts_start(&drag, 0.25))
                        .cloned();
                    if let Some(area) = &self.captured {
                        let _ = area.forward(&drag, context);
                    }
                }
                DragPhase::Moved => {
                    if let Some(area) = &self.captured {
                        let _ = area.forward(&drag, context);
                    }
                }
                DragPhase::Ended | DragPhase::Cancelled => {
                    if let Some(area) = self.captured.take() {
                        let _ = area.forward(&drag, context);
                    }
                }
            }
        }
    }

    fn square() -> HitRegion {
        HitRegion::fill(
            Path::polygon([
                Point::new(0.0, 0.0),
                Point::new(10.0, 0.0),
                Point::new(10.0, 10.0),
                Point::new(0.0, 10.0),
            ]),
            FillRule::NonZero,
        )
    }

    fn drag(phase: DragPhase, previous_position: Point, position: Point) -> Drag {
        Drag {
            pointer_id: PointerId::new(1),
            pointer_kind: PointerKind::Mouse,
            is_primary_pointer: true,
            button: PointerButton::Primary,
            phase,
            origin: Point::new(105.0, 105.0),
            previous_position,
            position,
        }
    }

    #[test]
    fn a_started_drag_remains_captured_outside_the_area() {
        let received = Rc::new(RefCell::new(Vec::new()));
        let output = Rc::clone(&received);
        let instance = TestRuntime::new(move |context| {
            let recorder = context.receive(move |event: Drag, _context| {
                output.borrow_mut().push(event);
            });
            let area = DragArea::new(recorder, square())
                .transformed(Affine2::from_translation(Vector2::new(100.0, 100.0)));
            let mut capture = Capture {
                areas: vec![area],
                captured: None,
            };
            context.receive(move |drag, context| capture.handle(drag, context))
        });
        let input = instance.interface().clone();
        let mut runtime = instance.into_driver();

        for drag in [
            drag(
                DragPhase::Started,
                Point::new(105.0, 105.0),
                Point::new(120.0, 120.0),
            ),
            drag(
                DragPhase::Moved,
                Point::new(120.0, 120.0),
                Point::new(140.0, 150.0),
            ),
            drag(
                DragPhase::Ended,
                Point::new(140.0, 150.0),
                Point::new(160.0, 170.0),
            ),
        ] {
            assert!(runtime.send(&input, drag).is_ok());
        }

        runtime.run_until_idle().expect("valid Effects");
        let received = received.borrow();
        assert_eq!(received.len(), 3);
        assert_eq!(received[0].origin, Point::new(5.0, 5.0));
        assert_eq!(received[1].position, Point::new(40.0, 50.0));
        assert_eq!(received[2].position, Point::new(60.0, 70.0));
    }
}
