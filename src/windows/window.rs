//! Native Windows implementation of Theoria's window adapter.

use std::{ffi::c_void, marker::PhantomData, rc::Rc};

use ::windows::{
    Win32::{
        Foundation::{HINSTANCE, HWND},
        System::LibraryLoader::GetModuleHandleW,
        UI::{
            HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi},
            WindowsAndMessaging::{
                CW_USEDEFAULT, CreateWindowExW, DestroyWindow, SM_CXDRAG, SM_CYDRAG, SW_SHOW,
                ShowWindow, WINDOW_EX_STYLE, WS_OVERLAPPEDWINDOW,
            },
        },
    },
    core::{PCWSTR, Result, w},
};

use crate::target::PresentationTarget;

use crate::{surface::SurfaceSize, window::native_event::NativeWindowEvent};

mod pointer;
mod procedure;
mod state;

use self::{procedure::initialize_windowing, procedure::resize_client_area, state::WindowState};

pub(crate) struct Win32Window {
    hwnd: HWND,
    hinstance: HINSTANCE,
    state: Box<WindowState>,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl Win32Window {
    pub(crate) fn new(title: &str, width: u32, height: u32) -> Result<Self> {
        unsafe {
            let module = GetModuleHandleW(None)?;
            let hinstance = HINSTANCE(module.0);
            initialize_windowing(hinstance)?;

            let mut state = Box::new(WindowState::new(SurfaceSize { width, height }));
            let state_ptr = (&mut *state as *mut WindowState).cast::<c_void>();
            let title: Vec<u16> = title.encode_utf16().chain(Some(0)).collect();

            let style = WS_OVERLAPPEDWINDOW;
            let extended_style = WINDOW_EX_STYLE::default();
            let hwnd = CreateWindowExW(
                extended_style,
                w!("TheoriaWindow"),
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
            state.finish_initialization(actual_size);
            let _ = ShowWindow(hwnd, SW_SHOW);

            Ok(Self {
                hwnd,
                hinstance,
                state,
                _thread_affinity: PhantomData,
            })
        }
    }

    pub(crate) fn presentation_target(&self) -> PresentationTarget {
        PresentationTarget::win32(self.hwnd.0 as isize, self.hinstance.0 as isize, self.size())
    }

    pub(crate) fn size(&self) -> SurfaceSize {
        self.state.size()
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

    pub(crate) fn has_pending_event(&self) -> bool {
        self.state.has_pending_event()
    }

    /// Takes one semantic event previously produced by this Window procedure.
    pub(crate) fn pop_event(&mut self) -> Option<NativeWindowEvent> {
        self.state.pop_event()
    }
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
