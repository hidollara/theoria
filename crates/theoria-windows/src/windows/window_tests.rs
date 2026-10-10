use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use ::windows::Win32::{
    Foundation::{HWND, LPARAM, WPARAM},
    UI::WindowsAndMessaging::{IsWindow, SendMessageW, WM_CLOSE, WM_SIZE},
};

use crate::{event::EventSource, event_loop::EventLoopStatus, surface::SurfaceSize};

use super::{Result, Window, WindowGroup, event_loop, presentation::WindowPresentation};

#[test]
fn window_without_presentation_mounts_and_closes_its_sources_on_drop() {
    let mut event_loop = event_loop();
    let mut messages = None;
    let initialized = Rc::new(Cell::new(false));
    let initialized_effect = Rc::clone(&initialized);
    let window = Window::without_presentation(
        &event_loop.handle(),
        "backend-independent test",
        64,
        48,
        |context, capabilities| {
            messages = Some(capabilities.native_messages().clone());
            let size = capabilities.surface().size().clone();
            let effect = context.create_effect(move |context| {
                // User32 may enlarge small requests to the minimum Window size.
                let size = context.read(&size);
                assert!(size.width > 0 && size.height > 0);
                initialized_effect.set(true);
            });
            context.sync(&effect);
        },
    )
    .unwrap();
    assert!(initialized.get());
    assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Active);
    drop(window);
    assert!(messages.unwrap().register(|_| {}).is_none());
    assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
}

#[test]
fn window_group_supports_backend_independent_children() {
    let mut event_loop = event_loop();
    let mut messages = Vec::new();
    let group = WindowGroup::new(&event_loop.handle(), |group| {
        for _ in 0..2 {
            group.mount_window_without_presentation("group test", 64, 48, |_, capabilities| {
                messages.push(capabilities.native_messages().clone());
            })?;
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Active);
    drop(group);
    assert!(
        messages
            .into_iter()
            .all(|source| source.register(|_| {}).is_none())
    );
    assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
}

struct TestPresentation {
    hwnd: HWND,
    pending: bool,
    sizes: Rc<RefCell<Vec<SurfaceSize>>>,
    presented: Rc<Cell<usize>>,
    dropped: Rc<Cell<bool>>,
}

impl WindowPresentation for TestPresentation {
    fn mark_pending(&mut self) {
        self.pending = true;
    }
    fn resize(&mut self, size: SurfaceSize) {
        self.sizes.borrow_mut().push(size);
    }
    fn has_pending(&self) -> bool {
        self.pending
    }
    fn take_redraw_request(&self) -> bool {
        false
    }
    fn advance(&mut self, _size: SurfaceSize, _wait: Duration) -> Result<()> {
        if self.pending {
            self.presented.set(self.presented.get() + 1);
            self.pending = false;
        }
        Ok(())
    }
}

impl Drop for TestPresentation {
    fn drop(&mut self) {
        assert!(unsafe { IsWindow(Some(self.hwnd)) }.as_bool());
        self.dropped.set(true);
    }
}

#[test]
fn host_forwards_resize_redraw_and_close_and_drops_presentation_before_window() {
    let mut event_loop = event_loop();
    let mut hwnd = HWND::default();
    let sizes = Rc::new(RefCell::new(Vec::new()));
    let presented = Rc::new(Cell::new(0));
    let dropped = Rc::new(Cell::new(false));
    let window = Window::mount(
        &event_loop.handle(),
        "presentation contract test",
        64,
        48,
        |native| {
            hwnd = native.raw_handles().0;
            Ok(TestPresentation {
                hwnd,
                pending: false,
                sizes: Rc::clone(&sizes),
                presented: Rc::clone(&presented),
                dropped: Rc::clone(&dropped),
            })
        },
        |_, _, _| {},
    )
    .unwrap();
    event_loop.poll().unwrap();
    assert_eq!(presented.get(), 1);
    unsafe {
        SendMessageW(
            hwnd,
            WM_SIZE,
            Some(WPARAM(0)),
            Some(LPARAM((80 << 16) | 100)),
        );
    }
    for _ in 0..4 {
        event_loop.poll().unwrap();
    }
    assert_eq!(
        *sizes.borrow(),
        [SurfaceSize {
            width: 100,
            height: 80
        }]
    );
    assert!(presented.get() >= 2);
    unsafe {
        SendMessageW(hwnd, WM_CLOSE, None, None);
    }
    for _ in 0..4 {
        event_loop.poll().unwrap();
    }
    assert!(dropped.get());
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
    assert_eq!(event_loop.poll().unwrap(), EventLoopStatus::Empty);
    drop(window);
}

#[test]
fn failed_presentation_creation_destroys_the_native_window() {
    let event_loop = event_loop();
    let mut hwnd = HWND::default();
    let result = Window::mount::<()>(
        &event_loop.handle(),
        "failed presentation test",
        64,
        48,
        |native| {
            hwnd = native.raw_handles().0;
            Err(std::io::Error::other("presentation initialization failed").into())
        },
        |_, _, _| panic!("build must not run after presentation failure"),
    );
    assert!(result.is_err());
    assert!(!unsafe { IsWindow(Some(hwnd)) }.as_bool());
}
