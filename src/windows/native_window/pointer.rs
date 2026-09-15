//! Translates the Win32 pointer message family into Theoria input events.
use ::windows::{
    Win32::{
        Foundation::{HWND, POINT, WPARAM},
        Graphics::Gdi::ScreenToClient,
        UI::{
            Input::Pointer::{
                EnableMouseInPointer, GetPointerInfo, POINTER_CHANGE_FIFTHBUTTON_DOWN,
                POINTER_CHANGE_FIFTHBUTTON_UP, POINTER_CHANGE_FIRSTBUTTON_DOWN,
                POINTER_CHANGE_FIRSTBUTTON_UP, POINTER_CHANGE_FOURTHBUTTON_DOWN,
                POINTER_CHANGE_FOURTHBUTTON_UP, POINTER_CHANGE_SECONDBUTTON_DOWN,
                POINTER_CHANGE_SECONDBUTTON_UP, POINTER_CHANGE_THIRDBUTTON_DOWN,
                POINTER_CHANGE_THIRDBUTTON_UP, POINTER_FLAG_CANCELED, POINTER_FLAG_FIFTHBUTTON,
                POINTER_FLAG_FIRSTBUTTON, POINTER_FLAG_FOURTHBUTTON, POINTER_FLAG_PRIMARY,
                POINTER_FLAG_SECONDBUTTON, POINTER_FLAG_THIRDBUTTON, POINTER_FLAGS, POINTER_INFO,
            },
            WindowsAndMessaging::{
                POINTER_INPUT_TYPE, PT_MOUSE, PT_PEN, PT_TOUCH, PT_TOUCHPAD, WHEEL_DELTA,
                WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERENTER, WM_POINTERHWHEEL,
                WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE, WM_POINTERWHEEL,
            },
        },
    },
    core::Result,
};

use crate::{
    geometry::Point,
    input::{
        pointer::{
            PointerButton, PointerButtonChange, PointerButtons, PointerEvent, PointerId,
            PointerKind, PointerPhase,
        },
        scroll::{ScrollDelta, ScrollEvent},
    },
    math::Vector2,
};

pub(super) const fn is_pointer_message(message: u32) -> bool {
    matches!(
        message,
        WM_POINTERDOWN
            | WM_POINTERUP
            | WM_POINTERUPDATE
            | WM_POINTERENTER
            | WM_POINTERLEAVE
            | WM_POINTERCAPTURECHANGED
            | WM_POINTERWHEEL
            | WM_POINTERHWHEEL
    )
}

pub(super) fn enable() -> Result<()> {
    unsafe { EnableMouseInPointer(true) }
}

pub(super) fn pointer_event(hwnd: HWND, message: u32, wparam: WPARAM) -> Option<PointerEvent> {
    let (info, position) = pointer_info(hwnd, wparam)?;
    let phase = pointer_phase(message, &info)?;
    let button_change = if phase == PointerPhase::Cancel {
        None
    } else {
        pointer_button_change(&info)
    };

    Some(PointerEvent {
        id: PointerId::new(u64::from(info.pointerId)),
        kind: pointer_kind(info.pointerType),
        position,
        is_primary: has_pointer_flag(&info, POINTER_FLAG_PRIMARY),
        buttons: pointer_buttons(&info),
        phase,
        button_change,
    })
}

pub(super) fn scroll_event(hwnd: HWND, message: u32, wparam: WPARAM) -> Option<ScrollEvent> {
    let (_, position) = pointer_info(hwnd, wparam)?;
    let steps = f32::from(wheel_delta(wparam)) / WHEEL_DELTA as f32;
    let delta = match message {
        WM_POINTERWHEEL => Vector2::new(0.0, -steps),
        WM_POINTERHWHEEL => Vector2::new(steps, 0.0),
        _ => return None,
    };

    Some(ScrollEvent {
        position,
        delta: ScrollDelta::Steps(delta),
    })
}

fn pointer_info(hwnd: HWND, wparam: WPARAM) -> Option<(POINTER_INFO, Point)> {
    let pointer_id = (wparam.0 as u32) & 0xffff;
    let mut info = POINTER_INFO::default();
    unsafe { GetPointerInfo(pointer_id, &mut info) }.ok()?;

    let mut position = POINT {
        x: info.ptPixelLocation.x,
        y: info.ptPixelLocation.y,
    };
    if !unsafe { ScreenToClient(hwnd, &mut position) }.as_bool() {
        return None;
    }

    Some((info, Point::new(position.x as f32, position.y as f32)))
}

fn wheel_delta(wparam: WPARAM) -> i16 {
    ((wparam.0 >> 16) as u16) as i16
}

fn pointer_kind(kind: POINTER_INPUT_TYPE) -> PointerKind {
    match kind {
        PT_MOUSE => PointerKind::Mouse,
        PT_TOUCH => PointerKind::Touch,
        PT_PEN => PointerKind::Pen,
        PT_TOUCHPAD => PointerKind::Touchpad,
        _ => PointerKind::Unknown,
    }
}

fn pointer_buttons(info: &POINTER_INFO) -> PointerButtons {
    [
        (POINTER_FLAG_FIRSTBUTTON, PointerButton::Primary),
        (POINTER_FLAG_SECONDBUTTON, PointerButton::Secondary),
        (POINTER_FLAG_THIRDBUTTON, PointerButton::Middle),
        (POINTER_FLAG_FOURTHBUTTON, PointerButton::Back),
        (POINTER_FLAG_FIFTHBUTTON, PointerButton::Forward),
    ]
    .into_iter()
    .filter(|(flag, _)| has_pointer_flag(info, *flag))
    .fold(PointerButtons::empty(), |buttons, (_, button)| {
        buttons.with(button)
    })
}

fn pointer_phase(message: u32, info: &POINTER_INFO) -> Option<PointerPhase> {
    if has_pointer_flag(info, POINTER_FLAG_CANCELED) || message == WM_POINTERCAPTURECHANGED {
        return Some(PointerPhase::Cancel);
    }

    match message {
        WM_POINTERDOWN => Some(PointerPhase::Down),
        WM_POINTERUP => Some(PointerPhase::Up),
        WM_POINTERUPDATE => Some(PointerPhase::Move),
        WM_POINTERENTER => Some(PointerPhase::Enter),
        WM_POINTERLEAVE => Some(PointerPhase::Leave),
        _ => None,
    }
}

fn pointer_button_change(info: &POINTER_INFO) -> Option<PointerButtonChange> {
    match info.ButtonChangeType {
        POINTER_CHANGE_FIRSTBUTTON_DOWN => {
            Some(PointerButtonChange::Pressed(PointerButton::Primary))
        }
        POINTER_CHANGE_FIRSTBUTTON_UP => {
            Some(PointerButtonChange::Released(PointerButton::Primary))
        }
        POINTER_CHANGE_SECONDBUTTON_DOWN => {
            Some(PointerButtonChange::Pressed(PointerButton::Secondary))
        }
        POINTER_CHANGE_SECONDBUTTON_UP => {
            Some(PointerButtonChange::Released(PointerButton::Secondary))
        }
        POINTER_CHANGE_THIRDBUTTON_DOWN => {
            Some(PointerButtonChange::Pressed(PointerButton::Middle))
        }
        POINTER_CHANGE_THIRDBUTTON_UP => Some(PointerButtonChange::Released(PointerButton::Middle)),
        POINTER_CHANGE_FOURTHBUTTON_DOWN => Some(PointerButtonChange::Pressed(PointerButton::Back)),
        POINTER_CHANGE_FOURTHBUTTON_UP => Some(PointerButtonChange::Released(PointerButton::Back)),
        POINTER_CHANGE_FIFTHBUTTON_DOWN => {
            Some(PointerButtonChange::Pressed(PointerButton::Forward))
        }
        POINTER_CHANGE_FIFTHBUTTON_UP => {
            Some(PointerButtonChange::Released(PointerButton::Forward))
        }
        _ => None,
    }
}

fn has_pointer_flag(info: &POINTER_INFO, flag: POINTER_FLAGS) -> bool {
    info.pointerFlags.0 & flag.0 != 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_flags_become_platform_independent_button_state() {
        let info = POINTER_INFO {
            pointerFlags: POINTER_FLAG_FIRSTBUTTON
                | POINTER_FLAG_THIRDBUTTON
                | POINTER_FLAG_FIFTHBUTTON,
            ..Default::default()
        };

        let buttons = pointer_buttons(&info);

        assert!(buttons.contains(PointerButton::Primary));
        assert!(buttons.contains(PointerButton::Middle));
        assert!(buttons.contains(PointerButton::Forward));
        assert!(!buttons.contains(PointerButton::Secondary));
        assert!(!buttons.contains(PointerButton::Back));
    }

    #[test]
    fn pointer_phase_and_button_change_are_independent() {
        let info = POINTER_INFO {
            ButtonChangeType: POINTER_CHANGE_SECONDBUTTON_DOWN,
            ..Default::default()
        };

        assert_eq!(
            pointer_phase(WM_POINTERUPDATE, &info),
            Some(PointerPhase::Move)
        );
        assert_eq!(
            pointer_button_change(&info),
            Some(PointerButtonChange::Pressed(PointerButton::Secondary))
        );
    }

    #[test]
    fn canceled_flag_wins_over_a_button_transition() {
        let info = POINTER_INFO {
            pointerFlags: POINTER_FLAG_CANCELED,
            ButtonChangeType: POINTER_CHANGE_FIRSTBUTTON_UP,
            ..Default::default()
        };

        assert_eq!(
            pointer_phase(WM_POINTERUP, &info),
            Some(PointerPhase::Cancel)
        );
    }

    #[test]
    fn capture_loss_cancels_without_a_paired_pointer_up() {
        let info = POINTER_INFO::default();

        assert_eq!(
            pointer_phase(WM_POINTERCAPTURECHANGED, &info),
            Some(PointerPhase::Cancel)
        );
    }

    #[test]
    fn wheel_delta_preserves_the_signed_high_word() {
        let positive = WPARAM((120_u32 << 16 | 7) as usize);
        let negative = WPARAM(((-120_i16 as u16 as u32) << 16 | 7) as usize);

        assert_eq!(wheel_delta(positive), 120);
        assert_eq!(wheel_delta(negative), -120);
    }
}
