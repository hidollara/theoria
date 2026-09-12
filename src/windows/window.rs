//! Native Windows implementation of Theoria's window adapter.

use std::{cell::RefCell, ffi::c_void, marker::PhantomData, rc::Rc};

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

use crate::{
    PresentationTarget,
    surface::SurfaceSize,
    window_backend::{WindowBackend, WindowEventBuffer},
};

mod pointer;
mod procedure;
mod state;

use self::{procedure::initialize_windowing, procedure::resize_client_area, state::WindowState};

struct Win32Window {
    hwnd: HWND,
    hinstance: HINSTANCE,
    // User32 may invoke the Window procedure synchronously during a native
    // call. Interior mutability makes that callback access explicit instead of
    // manufacturing a mutable reference beside Win32Window's active borrow.
    state: Box<RefCell<WindowState>>,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl Win32Window {
    fn new(title: &str, width: u32, height: u32, events: Rc<WindowEventBuffer>) -> Result<Self> {
        unsafe {
            let module = GetModuleHandleW(None)?;
            let hinstance = HINSTANCE(module.0);
            initialize_windowing(hinstance)?;

            let state = Box::new(RefCell::new(WindowState::new(
                SurfaceSize { width, height },
                events,
            )));
            let state_ptr = (&*state as *const RefCell<WindowState>).cast::<c_void>();
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
                Some(state_ptr),
            )?;

            let actual_size = match resize_client_area(hwnd, style, extended_style, width, height) {
                Ok(size) => size,
                Err(error) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(error);
                }
            };
            state.borrow_mut().finish_initialization(actual_size);

            Ok(Self {
                hwnd,
                hinstance,
                state,
                _thread_affinity: PhantomData,
            })
        }
    }
}

impl WindowBackend for Win32Window {
    fn show(&self) {
        let _ = unsafe { ShowWindow(self.hwnd, SW_SHOW) };
    }

    fn presentation_target(&self) -> PresentationTarget {
        PresentationTarget::win32(
            self.hwnd.0 as isize,
            self.hinstance.0 as isize,
            self.state.borrow().size(),
        )
    }

    fn size(&self) -> SurfaceSize {
        self.state.borrow().size()
    }

    fn movement_threshold_extents(&self) -> (f32, f32) {
        // Win32 defines the native drag boundary in device pixels for the
        // Window's current DPI.
        let dpi = unsafe { GetDpiForWindow(self.hwnd) };
        let horizontal = unsafe { GetSystemMetricsForDpi(SM_CXDRAG, dpi) }.unsigned_abs() as f32;
        let vertical = unsafe { GetSystemMetricsForDpi(SM_CYDRAG, dpi) }.unsigned_abs() as f32;
        (horizontal, vertical)
    }

    fn request_redraw(&mut self) {
        self.state.borrow_mut().request_redraw();
    }
}

pub(crate) fn create_window(
    title: &str,
    width: u32,
    height: u32,
    events: Rc<WindowEventBuffer>,
) -> Result<Box<dyn WindowBackend>> {
    Ok(Box::new(Win32Window::new(title, width, height, events)?))
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
