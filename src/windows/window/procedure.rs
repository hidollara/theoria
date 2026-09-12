//! Win32 Window class initialization and message procedure.

use std::{cell::RefCell, mem::size_of, sync::OnceLock};

use ::windows::{
    Win32::{
        Foundation::{E_INVALIDARG, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
        UI::{
            HiDpi::{
                AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetDpiForWindow, SetProcessDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                CREATESTRUCTW, DefWindowProcW, GWLP_USERDATA, GetClientRect, GetWindowLongPtrW,
                RegisterClassExW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER, SetWindowLongPtrW,
                SetWindowPos, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_DESTROY, WM_ERASEBKGND,
                WM_NCCREATE, WM_PAINT, WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERENTER,
                WM_POINTERHWHEEL, WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE, WM_POINTERWHEEL,
                WM_SIZE, WNDCLASSEXW,
            },
        },
    },
    core::{Error, Result, w},
};

use crate::{surface::SurfaceSize, window::backend::WindowBackendEvent};

use super::{pointer, state::WindowState};

/// Initializes process-wide pointer input and the shared Window class once.
pub(super) fn initialize_windowing(hinstance: HINSTANCE) -> Result<()> {
    static INITIALIZATION: OnceLock<Result<()>> = OnceLock::new();

    INITIALIZATION
        .get_or_init(|| unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            pointer::enable()?;
            // WM_SIZE explicitly stages a semantic redraw, and Vulkan does not
            // require a persistent GDI device context for the Window.
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(window_proc),
                hInstance: hinstance,
                lpszClassName: w!("TheoriaWindow"),
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                Err(Error::from_thread())
            } else {
                Ok(())
            }
        })
        .clone()
}

pub(super) fn resize_client_area(
    hwnd: HWND,
    style: WINDOW_STYLE,
    extended_style: WINDOW_EX_STYLE,
    width: u32,
    height: u32,
) -> Result<SurfaceSize> {
    let width = i32::try_from(width)
        .map_err(|_| Error::new(E_INVALIDARG, "surface width exceeds the Win32 range"))?;
    let height = i32::try_from(height)
        .map_err(|_| Error::new(E_INVALIDARG, "surface height exceeds the Win32 range"))?;
    let mut outer = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: height,
    };
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    unsafe { AdjustWindowRectExForDpi(&mut outer, style, false, extended_style, dpi) }?;
    unsafe {
        SetWindowPos(
            hwnd,
            None,
            0,
            0,
            outer.right - outer.left,
            outer.bottom - outer.top,
            SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
        )
    }?;

    let mut client = RECT::default();
    unsafe { GetClientRect(hwnd, &mut client) }?;
    Ok(SurfaceSize {
        width: (client.right - client.left).max(0) as u32,
        height: (client.bottom - client.top).max(0) as u32,
    })
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        let create = unsafe { &*(lparam.0 as *const CREATESTRUCTW) };
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as isize) };
    }

    let state_ptr =
        unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const RefCell<WindowState>;
    let state = unsafe { state_ptr.as_ref() };

    match message {
        WM_SIZE => {
            if let Some(state) = state {
                let mut state = state.borrow_mut();
                let packed = lparam.0 as u32;
                state.resize(SurfaceSize {
                    width: packed & 0xffff,
                    height: packed >> 16,
                });
            }
            LRESULT(0)
        }
        WM_PAINT => {
            if let Some(state) = state {
                state.borrow_mut().request_redraw();
            }
            let mut paint = PAINTSTRUCT::default();
            unsafe {
                BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
            }
            LRESULT(0)
        }
        WM_POINTERDOWN
        | WM_POINTERUP
        | WM_POINTERUPDATE
        | WM_POINTERENTER
        | WM_POINTERLEAVE
        | WM_POINTERCAPTURECHANGED => {
            if let Some(state) = state
                && let Some(event) = pointer::pointer_event(hwnd, message, wparam)
            {
                state.borrow_mut().push(WindowBackendEvent::Pointer(event));
            }
            LRESULT(0)
        }
        WM_POINTERWHEEL | WM_POINTERHWHEEL => {
            if let Some(state) = state
                && let Some(event) = pointer::scroll_event(hwnd, message, wparam)
            {
                state.borrow_mut().push(WindowBackendEvent::Scroll(event));
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            if let Some(state) = state {
                state.borrow_mut().push(WindowBackendEvent::CloseRequested);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}
