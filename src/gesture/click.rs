use crate::{
    geometry::Point,
    input::pointer::{
        PointerButton, PointerButtonChange, PointerEvent, PointerId, PointerKind, PointerPhase,
    },
};

/// A completed press-and-release gesture in one input coordinate space.
///
/// A click does not identify a visual or interactive target. Hit testing and
/// delivery decide which component, if any, receives it later.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Click {
    pub pointer_id: PointerId,
    pub pointer_kind: PointerKind,
    pub is_primary_pointer: bool,
    pub button: PointerButton,
    pub press_position: Point,
    pub release_position: Point,
}

impl From<&Click> for Click {
    fn from(click: &Click) -> Self {
        *click
    }
}

#[derive(Clone, Copy, Debug)]
struct Candidate {
    pointer_id: PointerId,
    pointer_kind: PointerKind,
    is_primary_pointer: bool,
    button: PointerButton,
    press_position: Point,
}

/// Recognizes clicks without assigning them to an interaction target.
///
/// `maximum_movement` is measured in the coordinate space of the supplied
/// [`PointerEvent`]s. Keeping it explicit lets a platform or application
/// choose a DPI- and device-appropriate threshold.
#[derive(Debug)]
pub struct ClickRecognizer {
    maximum_movement_squared: f32,
    candidates: Vec<Candidate>,
}

impl ClickRecognizer {
    /// Creates a recognizer when `maximum_movement` is finite and nonnegative.
    pub fn new(maximum_movement: f32) -> Option<Self> {
        if !maximum_movement.is_finite() || maximum_movement < 0.0 {
            return None;
        }

        Some(Self {
            maximum_movement_squared: maximum_movement * maximum_movement,
            candidates: Vec::new(),
        })
    }

    /// Consumes one primitive pointer observation and returns a completed click.
    ///
    /// Contact devices use their contact `Down`/`Up` lifecycle as the primary
    /// button. Mouse clicks follow explicit button transitions, which may be
    /// reported independently from the pointer lifecycle phase.
    pub fn recognize(&mut self, event: &PointerEvent) -> Option<Click> {
        if event.phase == PointerPhase::Cancel {
            self.cancel_pointer(event.id);
            return None;
        }

        self.cancel_candidates_moved_by(event);

        match event.button_change {
            Some(PointerButtonChange::Pressed(button)) => {
                self.start(event, button);
                None
            }
            Some(PointerButtonChange::Released(button)) => self.finish(event, button),
            None if event.kind != PointerKind::Mouse && event.phase == PointerPhase::Down => {
                self.start(event, PointerButton::Primary);
                None
            }
            None if event.kind != PointerKind::Mouse && event.phase == PointerPhase::Up => {
                self.finish(event, PointerButton::Primary)
            }
            None => None,
        }
    }

    fn start(&mut self, event: &PointerEvent, button: PointerButton) {
        self.remove(event.id, button);
        self.candidates.push(Candidate {
            pointer_id: event.id,
            pointer_kind: event.kind,
            is_primary_pointer: event.is_primary,
            button,
            press_position: event.position,
        });
    }

    fn finish(&mut self, event: &PointerEvent, button: PointerButton) -> Option<Click> {
        let index = self.candidate_index(event.id, button)?;
        let candidate = self.candidates.swap_remove(index);
        Some(Click {
            pointer_id: candidate.pointer_id,
            pointer_kind: candidate.pointer_kind,
            is_primary_pointer: candidate.is_primary_pointer,
            button: candidate.button,
            press_position: candidate.press_position,
            release_position: event.position,
        })
    }

    fn cancel_pointer(&mut self, pointer_id: PointerId) {
        self.candidates
            .retain(|candidate| candidate.pointer_id != pointer_id);
    }

    fn cancel_candidates_moved_by(&mut self, event: &PointerEvent) {
        let maximum = self.maximum_movement_squared;
        self.candidates.retain(|candidate| {
            if candidate.pointer_id != event.id {
                return true;
            }

            let movement = event.position - candidate.press_position;
            let squared = movement.x * movement.x + movement.y * movement.y;
            squared.is_finite() && squared <= maximum
        });
    }

    fn remove(&mut self, pointer_id: PointerId, button: PointerButton) {
        if let Some(index) = self.candidate_index(pointer_id, button) {
            self.candidates.swap_remove(index);
        }
    }

    fn candidate_index(&self, pointer_id: PointerId, button: PointerButton) -> Option<usize> {
        self.candidates
            .iter()
            .position(|candidate| candidate.pointer_id == pointer_id && candidate.button == button)
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

    #[test]
    fn contact_down_and_up_form_a_primary_click() {
        let mut recognizer = ClickRecognizer::new(5.0).unwrap();
        assert_eq!(
            recognizer.recognize(&pointer(
                1,
                PointerKind::Touch,
                PointerPhase::Down,
                Point::new(10.0, 20.0),
                None,
            )),
            None,
        );

        let click = recognizer
            .recognize(&pointer(
                1,
                PointerKind::Touch,
                PointerPhase::Up,
                Point::new(13.0, 24.0),
                None,
            ))
            .unwrap();

        assert_eq!(click.pointer_id, PointerId::new(1));
        assert_eq!(click.pointer_kind, PointerKind::Touch);
        assert!(click.is_primary_pointer);
        assert_eq!(click.button, PointerButton::Primary);
        assert_eq!(click.press_position, Point::new(10.0, 20.0));
        assert_eq!(click.release_position, Point::new(13.0, 24.0));
    }

    #[test]
    fn mouse_buttons_are_recognized_independently_from_contact_phase() {
        let mut recognizer = ClickRecognizer::new(3.0).unwrap();
        assert!(
            recognizer
                .recognize(&pointer(
                    1,
                    PointerKind::Mouse,
                    PointerPhase::Move,
                    Point::new(8.0, 8.0),
                    Some(PointerButtonChange::Pressed(PointerButton::Secondary)),
                ))
                .is_none()
        );

        let click = recognizer
            .recognize(&pointer(
                1,
                PointerKind::Mouse,
                PointerPhase::Move,
                Point::new(9.0, 8.0),
                Some(PointerButtonChange::Released(PointerButton::Secondary)),
            ))
            .unwrap();

        assert_eq!(click.button, PointerButton::Secondary);
    }

    #[test]
    fn excessive_movement_cancels_the_candidate() {
        let mut recognizer = ClickRecognizer::new(5.0).unwrap();
        recognizer.recognize(&pointer(
            1,
            PointerKind::Pen,
            PointerPhase::Down,
            Point::new(0.0, 0.0),
            None,
        ));
        recognizer.recognize(&pointer(
            1,
            PointerKind::Pen,
            PointerPhase::Move,
            Point::new(4.0, 4.0),
            None,
        ));

        assert!(
            recognizer
                .recognize(&pointer(
                    1,
                    PointerKind::Pen,
                    PointerPhase::Up,
                    Point::new(0.0, 0.0),
                    None,
                ))
                .is_none()
        );
    }

    #[test]
    fn cancel_discards_every_button_candidate_for_one_pointer() {
        let mut recognizer = ClickRecognizer::new(5.0).unwrap();
        for button in [PointerButton::Primary, PointerButton::Secondary] {
            recognizer.recognize(&pointer(
                1,
                PointerKind::Mouse,
                PointerPhase::Move,
                Point::new(0.0, 0.0),
                Some(PointerButtonChange::Pressed(button)),
            ));
        }

        recognizer.recognize(&pointer(
            1,
            PointerKind::Mouse,
            PointerPhase::Cancel,
            Point::new(0.0, 0.0),
            None,
        ));

        for button in [PointerButton::Primary, PointerButton::Secondary] {
            assert!(
                recognizer
                    .recognize(&pointer(
                        1,
                        PointerKind::Mouse,
                        PointerPhase::Move,
                        Point::new(0.0, 0.0),
                        Some(PointerButtonChange::Released(button)),
                    ))
                    .is_none()
            );
        }
    }

    #[test]
    fn concurrent_pointers_keep_independent_candidates() {
        let mut recognizer = ClickRecognizer::new(5.0).unwrap();
        for id in [1, 2] {
            recognizer.recognize(&pointer(
                id,
                PointerKind::Touch,
                PointerPhase::Down,
                Point::new(id as f32, 0.0),
                None,
            ));
        }

        let second = recognizer
            .recognize(&pointer(
                2,
                PointerKind::Touch,
                PointerPhase::Up,
                Point::new(2.0, 0.0),
                None,
            ))
            .unwrap();
        let first = recognizer
            .recognize(&pointer(
                1,
                PointerKind::Touch,
                PointerPhase::Up,
                Point::new(1.0, 0.0),
                None,
            ))
            .unwrap();

        assert_eq!(second.pointer_id, PointerId::new(2));
        assert_eq!(first.pointer_id, PointerId::new(1));
    }

    #[test]
    fn invalid_movement_thresholds_are_rejected() {
        assert!(ClickRecognizer::new(-1.0).is_none());
        assert!(ClickRecognizer::new(f32::NAN).is_none());
        assert!(ClickRecognizer::new(f32::INFINITY).is_none());
    }
}
