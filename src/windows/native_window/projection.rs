//! Built-in projection from raw Win32 messages into portable Window events.

use ::windows::Win32::UI::WindowsAndMessaging::{
    WM_CLOSE, WM_PAINT, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERENTER,
    WM_POINTERHWHEEL, WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE, WM_POINTERWHEEL, WM_SIZE,
};

use crate::{
    surface::SurfaceSize,
    windows::{
        event_buffer::{Win32WindowEvent, WindowEventBuffer},
        native_window::{message::Win32WindowMessage, pointer},
    },
};

pub(super) fn observe(events: &WindowEventBuffer, message: Win32WindowMessage<'_>) {
    let contents = message.message();
    match contents.id() {
        WM_SIZE => {
            let packed = contents.lparam().0 as u32;
            events.resize(SurfaceSize {
                width: packed & 0xffff,
                height: packed >> 16,
            });
        }
        WM_PAINT => events.request_redraw(),
        WM_POINTERDOWN
        | WM_POINTERUP
        | WM_POINTERUPDATE
        | WM_POINTERENTER
        | WM_POINTERLEAVE
        | WM_POINTERCAPTURECHANGED => {
            if let Some(event) =
                pointer::pointer_event(message.window(), contents.id(), contents.wparam())
            {
                events.push(Win32WindowEvent::Pointer(event));
            }
        }
        WM_POINTERWHEEL | WM_POINTERHWHEEL => {
            if let Some(event) =
                pointer::scroll_event(message.window(), contents.id(), contents.wparam())
            {
                events.push(Win32WindowEvent::Scroll(event));
            }
        }
        WM_CLOSE => events.push(Win32WindowEvent::CloseRequested),
        _ => {}
    }
}
