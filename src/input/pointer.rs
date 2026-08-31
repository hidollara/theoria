use crate::geometry::Point;

/// Identifies one pointer for the duration of its platform-defined lifetime.
///
/// An identifier is meaningful only within the input stream that produced it;
/// platforms may reuse its numeric value after a pointer leaves that stream.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PointerId(u64);

impl PointerId {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn value(self) -> u64 {
        self.0
    }
}

/// Broad device category without exposing a platform HID type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerKind {
    Mouse,
    Touch,
    Pen,
    Touchpad,
    Unknown,
}

/// A semantic pointer button shared across device categories.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerButton {
    Primary,
    Secondary,
    Middle,
    Back,
    Forward,
}

/// Buttons held after the transition represented by a [`PointerEvent`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct PointerButtons(u8);

impl PointerButtons {
    pub const fn empty() -> Self {
        Self(0)
    }

    pub const fn contains(self, button: PointerButton) -> bool {
        self.0 & button.bit() != 0
    }

    pub const fn with(self, button: PointerButton) -> Self {
        Self(self.0 | button.bit())
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

impl PointerButton {
    const fn bit(self) -> u8 {
        match self {
            Self::Primary => 1 << 0,
            Self::Secondary => 1 << 1,
            Self::Middle => 1 << 2,
            Self::Back => 1 << 3,
            Self::Forward => 1 << 4,
        }
    }
}

/// The primitive lifecycle phase observed for one pointer.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerPhase {
    Enter,
    Move,
    Down,
    Up,
    Leave,
    Cancel,
}

/// A button transition accompanying a pointer phase.
///
/// Contact-based devices can produce [`PointerPhase::Down`] and
/// [`PointerPhase::Up`] without a button transition. Conversely, a mouse can
/// change a secondary button while its pointer remains in the move phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum PointerButtonChange {
    Pressed(PointerButton),
    Released(PointerButton),
}

/// One pointer observation in its current input coordinate space.
///
/// A platform adapter chooses the initial coordinate space. Routing or
/// projection can then express the same pointer relative to another target.
/// The Windows window adapter starts with client-area coordinates: top-left
/// origin, positive x right, and positive y down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PointerEvent {
    pub id: PointerId,
    pub kind: PointerKind,
    pub position: Point,
    pub is_primary: bool,
    pub buttons: PointerButtons,
    pub phase: PointerPhase,
    pub button_change: Option<PointerButtonChange>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_sets_are_explicit_value_objects() {
        let buttons = PointerButtons::empty()
            .with(PointerButton::Primary)
            .with(PointerButton::Back);

        assert!(buttons.contains(PointerButton::Primary));
        assert!(buttons.contains(PointerButton::Back));
        assert!(!buttons.contains(PointerButton::Secondary));
        assert!(!buttons.is_empty());
    }
}
