//! Native Windows implementation of Theoria's window adapter.

use std::{collections::VecDeque, ffi::c_void, mem::size_of};

use ::windows::{
    Win32::{
        Foundation::{E_INVALIDARG, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{
                AdjustWindowRectExForDpi, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                GetDpiForWindow, GetSystemMetricsForDpi, SetProcessDpiAwarenessContext,
            },
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetClientRect,
                GetMessageW, GetWindowLongPtrW, MSG, PM_REMOVE, PeekMessageW, RegisterClassExW,
                SM_CXDRAG, SM_CYDRAG, SW_SHOW, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
                SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT,
                WM_POINTERCAPTURECHANGED, WM_POINTERDOWN, WM_POINTERENTER, WM_POINTERHWHEEL,
                WM_POINTERLEAVE, WM_POINTERUP, WM_POINTERUPDATE, WM_POINTERWHEEL, WM_QUIT, WM_SIZE,
                WNDCLASSEXW, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{Error, PCWSTR, Result, w},
};

use crate::target::PresentationTarget;

use crate::{
    surface::{SurfaceEvent, SurfaceSize},
    window::NativeWindowEvent,
};

mod pointer;

struct WindowState {
    size: SurfaceSize,
    events: VecDeque<NativeWindowEvent>,
}

impl WindowState {
    fn request_redraw(&mut self) {
        let redraw = NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested);
        if !self.events.contains(&redraw) {
            self.events.push_back(redraw);
        }
    }

    fn resize(&mut self, size: SurfaceSize) {
        self.size = size;

        // A renderer only needs the newest surface size. Keep the resize ahead
        // of its redraw so stale intermediate window sizes cannot build up
        // while an expensive frame is being prepared.
        self.events
            .retain(|event| *event != NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested));
        if let Some(event) = self
            .events
            .iter_mut()
            .find(|event| matches!(event, NativeWindowEvent::Surface(SurfaceEvent::Resized(_))))
        {
            *event = NativeWindowEvent::Surface(SurfaceEvent::Resized(size));
        } else {
            self.events
                .push_back(NativeWindowEvent::Surface(SurfaceEvent::Resized(size)));
        }
        self.request_redraw();
    }
}

pub(crate) struct Win32Window {
    hwnd: HWND,
    hinstance: HINSTANCE,
    state: Box<WindowState>,
}

impl Win32Window {
    pub(crate) fn new(title: &str, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
            pointer::enable()?;

            let module = GetModuleHandleW(None)?;
            let hinstance = HINSTANCE(module.0);
            let class_name = w!("TheoriaWindow");
            let class = WNDCLASSEXW {
                cbSize: size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW | CS_OWNDC,
                lpfnWndProc: Some(window_proc),
                hInstance: hinstance,
                lpszClassName: class_name,
                ..Default::default()
            };
            if RegisterClassExW(&class) == 0 {
                return Err(Error::from_thread());
            }

            let mut state = Box::new(WindowState {
                size: SurfaceSize { width, height },
                events: VecDeque::new(),
            });
            let state_ptr = (&mut *state as *mut WindowState).cast::<c_void>();
            let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();

            let style = WS_OVERLAPPEDWINDOW;
            let extended_style = WINDOW_EX_STYLE::default();
            let hwnd = CreateWindowExW(
                extended_style,
                class_name,
                PCWSTR(title.as_ptr()),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,
                height as i32,
                None,
                None,
                Some(hinstance),
                Some(state_ptr.cast_const()),
            )?;

            let actual_size = match resize_client_area(hwnd, style, extended_style, width, height) {
                Ok(size) => size,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            state.size = actual_size;
            state.events.clear();
            state.request_redraw();
            let _ = ShowWindow(hwnd, SW_SHOW);

            Ok(Self {
                hwnd,
                hinstance,
                state,
            })
        }
    }

    pub(crate) fn presentation_target(&self) -> PresentationTarget {
        PresentationTarget::win32(self.hwnd.0 as isize, self.hinstance.0 as isize, self.size())
    }

    pub(crate) fn size(&self) -> SurfaceSize {
        self.state.size
    }

    pub(crate) fn movement_threshold_extents(&self) -> (f32, f32) {
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let horizontal = unsafe { GetSystemMetricsForDpi(SM_CXDRAG, dpi) }.unsigned_abs() as f32;
        let vertical = unsafe { GetSystemMetricsForDpi(SM_CYDRAG, dpi) }.unsigned_abs() as f32;
        (horizontal, vertical)
    }

    /// Queues one semantic redraw request, coalescing repeated requests.
    pub(crate) fn request_redraw(&mut self) {
        self.state.request_redraw();
    }

    /// Returns the next semantic window event, blocking only while the queue is empty.
    pub(crate) fn next_event(&mut self) -> Result<Option<NativeWindowEvent>> {
        unsafe {
            loop {
                // Drain the native queue before returning semantic work. This
                // lets resize/paint bursts collapse into one redraw instead of
                // blocking message dispatch for one expensive frame each.
                let mut message = MSG::default();
                while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                    if message.message == WM_QUIT {
                        return Ok(None);
                    }
                    let _ = TranslateMessage(&message);
                    DispatchMessageW(&message);
                }

                if let Some(event) = self.state.events.pop_front() {
                    return Ok(Some(event));
                }

                let result = GetMessageW(&mut message, None, 0, 0).0;
                if result == -1 {
                    return Err(Error::from_thread());
                }
                if result == 0 {
                    return Ok(None);
                }
                let _ = TranslateMessage(&message);
                DispatchMessageW(&message);
            }
        }
    }
}

fn resize_client_area(
    hwnd: HWND,
    style: ::windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE,
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

impl Drop for Win32Window {
    fn drop(&mut self) {
        unsafe {
            if !self.hwnd.is_invalid() {
                let _ = DestroyWindow(self.hwnd);
            }
        }
    }
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

    let state_ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut WindowState;
    let state = unsafe { state_ptr.as_mut() };

    match message {
        WM_SIZE => {
            if let Some(state) = state {
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
                state.request_redraw();
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
                state.events.push_back(NativeWindowEvent::Pointer(event));
            }
            LRESULT(0)
        }
        WM_POINTERWHEEL | WM_POINTERHWHEEL => {
            if let Some(state) = state
                && let Some(event) = pointer::scroll_event(hwnd, message, wparam)
            {
                state.events.push_back(NativeWindowEvent::Scroll(event));
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            if let Some(state) = state {
                state.events.push_back(NativeWindowEvent::CloseRequested);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_requests_are_coalesced() {
        let mut state = WindowState {
            size: SurfaceSize {
                width: 100,
                height: 100,
            },
            events: VecDeque::new(),
        };

        state.request_redraw();
        state.request_redraw();

        assert_eq!(
            state.events,
            VecDeque::from([NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested)])
        );
    }

    #[test]
    fn resize_bursts_keep_only_the_latest_size_and_one_redraw() {
        let mut state = WindowState {
            size: SurfaceSize {
                width: 100,
                height: 100,
            },
            events: VecDeque::from([NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested)]),
        };

        state.resize(SurfaceSize {
            width: 200,
            height: 150,
        });
        state.resize(SurfaceSize {
            width: 320,
            height: 240,
        });

        assert_eq!(state.size.width, 320);
        assert_eq!(state.size.height, 240);
        assert_eq!(
            state.events,
            VecDeque::from([
                NativeWindowEvent::Surface(SurfaceEvent::Resized(state.size)),
                NativeWindowEvent::Surface(SurfaceEvent::RedrawRequested),
            ])
        );
    }
}
