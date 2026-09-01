use crate::{
    geometry::Point,
    input::pointer::{
        PointerButton, PointerButtonChange, PointerEvent, PointerId, PointerKind, PointerPhase,
    },
};

use super::MovementThreshold;

/// The lifecycle stage of a recognized drag.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum DragPhase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// One observation in a drag gesture, expressed in one input coordinate space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Drag {
    pub pointer_id: PointerId,
    pub pointer_kind: PointerKind,
    pub is_primary_pointer: bool,
    pub button: PointerButton,
    pub phase: DragPhase,
    pub origin: Point,
    pub previous_position: Point,
    pub position: Point,
}

impl From<&Drag> for Drag {
    fn from(drag: &Drag) -> Self {
        *drag
    }
}

#[derive(Clone, Copy, Debug)]
struct Session {
    pointer_id: PointerId,
    pointer_kind: PointerKind,
    is_primary_pointer: bool,
    button: PointerButton,
    origin: Point,
    previous_position: Point,
    dragging: bool,
}

/// Recognizes independent drag lifecycles without assigning interaction targets.
///
/// At most one button owns a drag session for each pointer. Additional button
/// presses are ignored until that pointer's current session ends. The movement
/// threshold is measured in the coordinate space of the supplied pointer events.
#[derive(Debug)]
pub struct DragRecognizer {
    movement_threshold: MovementThreshold,
    sessions: Vec<Session>,
}

impl DragRecognizer {
    pub fn new(movement_threshold: MovementThreshold) -> Option<Self> {
        movement_threshold.is_valid().then_some(Self {
            movement_threshold,
            sessions: Vec::new(),
        })
    }

    /// Consumes one primitive pointer observation and returns at most one drag event.
    pub fn recognize(&mut self, event: &PointerEvent) -> Option<Drag> {
        if event.phase == PointerPhase::Cancel {
            return self.cancel(event);
        }

        match event.button_change {
            Some(PointerButtonChange::Pressed(button)) => {
                self.start(event, button);
                return None;
            }
            Some(PointerButtonChange::Released(button)) => {
                return self.finish(event, button);
            }
            None => {}
        }

        if event.kind != PointerKind::Mouse {
            match event.phase {
                PointerPhase::Down => {
                    self.start(event, PointerButton::Primary);
                    return None;
                }
                PointerPhase::Up => return self.finish(event, PointerButton::Primary),
                _ => {}
            }
        }

        (event.phase == PointerPhase::Move)
            .then(|| self.advance(event))
            .flatten()
    }

    fn start(&mut self, event: &PointerEvent, button: PointerButton) {
        if self.session_index(event.id).is_some() || !event.position.is_finite() {
            return;
        }
        self.sessions.push(Session {
            pointer_id: event.id,
            pointer_kind: event.kind,
            is_primary_pointer: event.is_primary,
            button,
            origin: event.position,
            previous_position: event.position,
            dragging: false,
        });
    }

    fn advance(&mut self, event: &PointerEvent) -> Option<Drag> {
        let index = self.session_index(event.id)?;
        if !event.position.is_finite() {
            return self.cancel_index(index, event.position);
        }

        let session = &mut self.sessions[index];
        if !session.dragging {
            if !self
                .movement_threshold
                .exceeded(session.origin, event.position)
            {
                return None;
            }
            session.dragging = true;
            let drag = session.event(DragPhase::Started, event.position);
            session.previous_position = event.position;
            return Some(drag);
        }

        let drag = session.event(DragPhase::Moved, event.position);
        session.previous_position = event.position;
        Some(drag)
    }

    fn finish(&mut self, event: &PointerEvent, button: PointerButton) -> Option<Drag> {
        let index = self.session_index(event.id)?;
        if self.sessions[index].button != button {
            return None;
        }
        let session = self.sessions.swap_remove(index);
        session.dragging.then(|| {
            let phase = if event.position.is_finite() {
                DragPhase::Ended
            } else {
                DragPhase::Cancelled
            };
            let position = if event.position.is_finite() {
                event.position
            } else {
                session.previous_position
            };
            session.event(phase, position)
        })
    }

    fn cancel(&mut self, event: &PointerEvent) -> Option<Drag> {
        let index = self.session_index(event.id)?;
        self.cancel_index(index, event.position)
    }

    fn cancel_index(&mut self, index: usize, position: Point) -> Option<Drag> {
        let session = self.sessions.swap_remove(index);
        session.dragging.then(|| {
            let position = if position.is_finite() {
                position
            } else {
                session.previous_position
            };
            session.event(DragPhase::Cancelled, position)
        })
    }

    fn session_index(&self, pointer_id: PointerId) -> Option<usize> {
        self.sessions
            .iter()
            .position(|session| session.pointer_id == pointer_id)
    }
}

impl Session {
    fn event(self, phase: DragPhase, position: Point) -> Drag {
        Drag {
            pointer_id: self.pointer_id,
            pointer_kind: self.pointer_kind,
            is_primary_pointer: self.is_primary_pointer,
            button: self.button,
            phase,
            origin: self.origin,
            previous_position: self.previous_position,
            position,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::input::pointer::PointerButtons;

    fn pointer(
        id: u64,
        kind: PointerKind,
        phase: PointerPhase,
        position: Point,
        button_change: Option<PointerButtonChange>,
    ) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(id),
            kind,
            position,
            is_primary: id == 1,
            buttons: PointerButtons::empty(),
            phase,
            button_change,
        }
    }

    fn touch(id: u64, phase: PointerPhase, position: Point) -> PointerEvent {
        pointer(id, PointerKind::Touch, phase, position, None)
    }

    #[test]
    fn movement_crosses_the_threshold_then_ends() {
        let mut recognizer = DragRecognizer::new(MovementThreshold::AxisAligned {
            horizontal: 3.0,
            vertical: 4.0,
        })
        .unwrap();
        assert_eq!(
            recognizer.recognize(&touch(1, PointerPhase::Down, Point::new(0.0, 0.0))),
            None
        );
        assert_eq!(
            recognizer.recognize(&touch(1, PointerPhase::Move, Point::new(3.0, 4.0))),
            None,
        );
        assert_eq!(
            recognizer
                .recognize(&touch(1, PointerPhase::Move, Point::new(3.1, 4.0)))
                .unwrap()
                .phase,
            DragPhase::Started,
        );
        let moved = recognizer
            .recognize(&touch(1, PointerPhase::Move, Point::new(8.0, 6.0)))
            .unwrap();
        assert_eq!(moved.phase, DragPhase::Moved);
        assert_eq!(moved.previous_position, Point::new(3.1, 4.0));
        assert_eq!(moved.position, Point::new(8.0, 6.0));

        let ended = recognizer
            .recognize(&touch(1, PointerPhase::Up, Point::new(20.0, 30.0)))
            .unwrap();
        assert_eq!(ended.phase, DragPhase::Ended);
        assert_eq!(ended.origin, Point::new(0.0, 0.0));
        assert_eq!(ended.previous_position, Point::new(8.0, 6.0));
        assert_eq!(ended.position, Point::new(20.0, 30.0));
    }

    #[test]
    fn releasing_before_the_threshold_is_not_a_drag() {
        let mut recognizer =
            DragRecognizer::new(MovementThreshold::Radial { radius: 5.0 }).unwrap();
        recognizer.recognize(&touch(1, PointerPhase::Down, Point::new(0.0, 0.0)));
        recognizer.recognize(&touch(1, PointerPhase::Move, Point::new(1.0, 1.0)));
        assert_eq!(
            recognizer.recognize(&touch(1, PointerPhase::Up, Point::new(1.0, 1.0))),
            None,
        );
    }

    #[test]
    fn cancellation_reports_an_active_drag_and_discards_a_candidate() {
        let mut recognizer =
            DragRecognizer::new(MovementThreshold::Radial { radius: 1.0 }).unwrap();
        recognizer.recognize(&touch(1, PointerPhase::Down, Point::new(0.0, 0.0)));
        recognizer.recognize(&touch(1, PointerPhase::Move, Point::new(2.0, 0.0)));
        assert_eq!(
            recognizer
                .recognize(&touch(1, PointerPhase::Cancel, Point::new(3.0, 0.0)))
                .unwrap()
                .phase,
            DragPhase::Cancelled,
        );

        recognizer.recognize(&touch(2, PointerPhase::Down, Point::new(0.0, 0.0)));
        assert_eq!(
            recognizer.recognize(&touch(2, PointerPhase::Cancel, Point::new(0.0, 0.0))),
            None,
        );
    }

    #[test]
    fn concurrent_pointers_keep_independent_sessions() {
        let mut recognizer =
            DragRecognizer::new(MovementThreshold::Radial { radius: 1.0 }).unwrap();
        for id in [1, 2] {
            recognizer.recognize(&touch(id, PointerPhase::Down, Point::new(0.0, 0.0)));
        }
        let second = recognizer
            .recognize(&touch(2, PointerPhase::Move, Point::new(2.0, 0.0)))
            .unwrap();
        let first = recognizer
            .recognize(&touch(1, PointerPhase::Move, Point::new(3.0, 0.0)))
            .unwrap();
        assert_eq!(second.pointer_id, PointerId::new(2));
        assert_eq!(first.pointer_id, PointerId::new(1));
    }

    #[test]
    fn a_mouse_drag_remembers_its_initiating_button() {
        let mut recognizer =
            DragRecognizer::new(MovementThreshold::Radial { radius: 1.0 }).unwrap();
        recognizer.recognize(&pointer(
            1,
            PointerKind::Mouse,
            PointerPhase::Move,
            Point::new(0.0, 0.0),
            Some(PointerButtonChange::Pressed(PointerButton::Secondary)),
        ));
        let started = recognizer
            .recognize(&pointer(
                1,
                PointerKind::Mouse,
                PointerPhase::Move,
                Point::new(2.0, 0.0),
                None,
            ))
            .unwrap();
        assert_eq!(started.button, PointerButton::Secondary);
        let ended = recognizer
            .recognize(&pointer(
                1,
                PointerKind::Mouse,
                PointerPhase::Move,
                Point::new(3.0, 0.0),
                Some(PointerButtonChange::Released(PointerButton::Secondary)),
            ))
            .unwrap();
        assert_eq!(ended.phase, DragPhase::Ended);
    }

    #[test]
    fn invalid_thresholds_are_rejected() {
        assert!(DragRecognizer::new(MovementThreshold::Radial { radius: -1.0 }).is_none());
        assert!(
            DragRecognizer::new(MovementThreshold::AxisAligned {
                horizontal: 1.0,
                vertical: f32::INFINITY,
            })
            .is_none()
        );
    }
}
