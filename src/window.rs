use std::{collections::VecDeque, ffi::c_void, mem::size_of};

use windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        Graphics::Gdi::{BeginPaint, EndPaint, PAINTSTRUCT},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext},
            WindowsAndMessaging::{
                CREATESTRUCTW, CS_HREDRAW, CS_OWNDC, CS_VREDRAW, CW_USEDEFAULT, CreateWindowExW,
                DefWindowProcW, DestroyWindow, DispatchMessageW, GWLP_USERDATA, GetMessageW,
                GetWindowLongPtrW, MSG, PM_REMOVE, PeekMessageW, PostQuitMessage, RegisterClassExW,
                SW_SHOW, SetWindowLongPtrW, ShowWindow, TranslateMessage, WINDOW_EX_STYLE,
                WM_CLOSE, WM_DESTROY, WM_ERASEBKGND, WM_NCCREATE, WM_PAINT, WM_QUIT, WM_SIZE,
                WNDCLASSEXW, WS_OVERLAPPEDWINDOW, WS_VISIBLE,
            },
        },
    },
    core::{Error, Result, w},
};

use crate::target::PresentationTarget;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowSize {
    pub width: u32,
    pub height: u32,
}

struct WindowState {
    size: WindowSize,
    events: VecDeque<WindowEvent>,
}

impl WindowState {
    fn request_redraw(&mut self) {
        if !self.events.contains(&WindowEvent::RedrawRequested) {
            self.events.push_back(WindowEvent::RedrawRequested);
        }
    }

    fn resize(&mut self, size: WindowSize) {
        self.size = size;

        // A renderer only needs the newest surface size. Keep the resize ahead
        // of its redraw so stale intermediate window sizes cannot build up
        // while an expensive frame is being prepared.
        self.events
            .retain(|event| *event != WindowEvent::RedrawRequested);
        if let Some(event) = self
            .events
            .iter_mut()
            .find(|event| matches!(event, WindowEvent::Resized(_)))
        {
            *event = WindowEvent::Resized(size);
        } else {
            self.events.push_back(WindowEvent::Resized(size));
        }
        self.request_redraw();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowEvent {
    Resized(WindowSize),
    RedrawRequested,
    CloseRequested,
}

pub struct Window {
    hwnd: HWND,
    hinstance: HINSTANCE,
    state: Box<WindowState>,
}

impl Window {
    pub fn new(_title: &str, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

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
                size: WindowSize { width, height },
                events: VecDeque::new(),
            });
            let state_ptr = (&mut *state as *mut WindowState).cast::<c_void>();

            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                class_name,
                w!("Theoria — Vulkan Canvas"),
                WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                width as i32,
                height as i32,
                None,
                None,
                Some(hinstance),
                Some(state_ptr.cast_const()),
            )?;
            let _ = ShowWindow(hwnd, SW_SHOW);
            state.request_redraw();

            Ok(Self {
                hwnd,
                hinstance,
                state,
            })
        }
    }

    pub fn presentation_target(&self) -> PresentationTarget {
        PresentationTarget::win32(self.hwnd.0 as isize, self.hinstance.0 as isize, self.size())
    }

    pub fn size(&self) -> WindowSize {
        self.state.size
    }

    /// Returns the next semantic window event, blocking only while the queue is empty.
    pub fn next_event(&mut self) -> Result<Option<WindowEvent>> {
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

impl Drop for Window {
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
                state.resize(WindowSize {
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
        WM_ERASEBKGND => LRESULT(1),
        WM_CLOSE => {
            if let Some(state) = state {
                state.events.push_back(WindowEvent::CloseRequested);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redraw_requests_are_coalesced() {
        let mut state = WindowState {
            size: WindowSize {
                width: 100,
                height: 100,
            },
            events: VecDeque::new(),
        };

        state.request_redraw();
        state.request_redraw();

        assert_eq!(state.events, VecDeque::from([WindowEvent::RedrawRequested]));
    }

    #[test]
    fn resize_bursts_keep_only_the_latest_size_and_one_redraw() {
        let mut state = WindowState {
            size: WindowSize {
                width: 100,
                height: 100,
            },
            events: VecDeque::from([WindowEvent::RedrawRequested]),
        };

        state.resize(WindowSize {
            width: 200,
            height: 150,
        });
        state.resize(WindowSize {
            width: 320,
            height: 240,
        });

        assert_eq!(state.size.width, 320);
        assert_eq!(state.size.height, 240);
        assert_eq!(
            state.events,
            VecDeque::from([
                WindowEvent::Resized(state.size),
                WindowEvent::RedrawRequested,
            ])
        );
    }
}
