//! Win32 message pumping for one GUI thread.

use std::{
    io,
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex, Weak},
    time::Duration,
};

use ::windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WAIT_FAILED, WAIT_TIMEOUT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
            PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT,
            TranslateMessage, WM_NULL, WM_QUIT, WM_USER,
        },
    },
    core::{Error, Result},
};

/// Drives the Win32 message queue belonging to the current GUI thread.
///
/// One thread queue services every Window created on that thread. This access
/// object also receives messages posted directly to the thread, so neither the
/// queue nor this pump requires a Window to exist. It is deliberately `!Send`
/// and `!Sync`: Win32 associates the queue and its Windows with the thread that
/// created them.
pub(crate) struct WindowsMessagePump {
    wake: Arc<WakeState>,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl WindowsMessagePump {
    pub(crate) fn new() -> Self {
        // A thread message queue is created lazily by User32. Establish it now
        // so a subsequently issued cross-thread posting cannot race the first
        // blocking wait, including when this thread owns no Window.
        unsafe {
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
        }
        Self {
            wake: Arc::new(WakeState {
                thread_id: unsafe { GetCurrentThreadId() },
                open: Mutex::new(true),
            }),
            _thread_affinity: PhantomData,
        }
    }

    /// Returns a thread-safe, non-owning capability that interrupts a wait.
    pub(crate) fn waker(&self) -> MessagePumpWaker {
        MessagePumpWaker {
            state: Arc::downgrade(&self.wake),
        }
    }

    /// Waits for and processes one message from the current thread's queue.
    ///
    /// Window messages are dispatched to their Window procedures. A message
    /// without an HWND is returned to the caller as a thread message instead.
    pub(crate) fn wait(&mut self) -> Result<MessagePumpEvent> {
        unsafe {
            let mut message = MSG::default();
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == -1 {
                return Err(Error::from_thread());
            }
            if result == 0 {
                return Ok(MessagePumpEvent::Quit(message.wParam.0 as i32));
            }
            Ok(process(message))
        }
    }

    /// Waits until either native input arrives or `timeout` elapses.
    pub(crate) fn wait_for(&mut self, timeout: Duration) -> Result<MessagePumpEvent> {
        if timeout.is_zero() {
            return Ok(self.poll());
        }
        let result = unsafe {
            MsgWaitForMultipleObjectsEx(
                None,
                timeout_milliseconds(timeout),
                QS_ALLINPUT,
                MWMO_INPUTAVAILABLE,
            )
        };
        if result == WAIT_FAILED {
            return Err(Error::from_thread());
        }
        if result == WAIT_TIMEOUT {
            return Ok(MessagePumpEvent::Idle);
        }
        Ok(self.poll())
    }

    /// Processes at most one ready message from the current thread's queue.
    ///
    /// Limiting one call to one message lets the owner interleave native input
    /// with other execution domains instead of allowing an active Win32 queue
    /// to monopolize its thread.
    pub(crate) fn poll(&mut self) -> MessagePumpEvent {
        unsafe {
            let mut message = MSG::default();
            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                return MessagePumpEvent::Idle;
            }
            if message.message == WM_QUIT {
                return MessagePumpEvent::Quit(message.wParam.0 as i32);
            }
            process(message)
        }
    }
}

impl Drop for WindowsMessagePump {
    fn drop(&mut self) {
        *self
            .wake
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }
}

/// A thread-safe, non-owning capability for interrupting a message-pump wait.
#[derive(Clone)]
pub(crate) struct MessagePumpWaker {
    state: Weak<WakeState>,
}

impl MessagePumpWaker {
    pub(crate) fn wake(&self) -> io::Result<()> {
        let Some(state) = self.state.upgrade() else {
            return Err(unavailable_wake_error());
        };
        let open = state
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*open {
            return Err(unavailable_wake_error());
        }
        unsafe { PostThreadMessageW(state.thread_id, WM_NULL, WPARAM(0), LPARAM(0)) }
            .map_err(io::Error::other)
    }
}

struct WakeState {
    thread_id: u32,
    open: Mutex<bool>,
}

fn unavailable_wake_error() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "the event loop is unavailable")
}

fn timeout_milliseconds(timeout: Duration) -> u32 {
    let milliseconds = timeout.as_millis();
    let rounded_up = milliseconds + u128::from(!timeout.subsec_nanos().is_multiple_of(1_000_000));
    rounded_up.clamp(1, u128::from(u32::MAX - 1)) as u32
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MessagePumpEvent {
    WindowDispatched,
    ThreadMessage(ThreadMessage),
    Idle,
    Quit(i32),
}

/// One posted message that belongs to a thread rather than a Window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadMessage {
    pub(crate) message: u32,
    pub(crate) wparam: usize,
    pub(crate) lparam: isize,
}

fn process(message: MSG) -> MessagePumpEvent {
    if message.hwnd == HWND::default() {
        return MessagePumpEvent::ThreadMessage(ThreadMessage {
            message: message.message,
            wparam: message.wParam.0,
            lparam: message.lParam.0,
        });
    }
    unsafe { dispatch(&message) };
    MessagePumpEvent::WindowDispatched
}

unsafe fn dispatch(message: &MSG) {
    unsafe {
        let _ = TranslateMessage(message);
        DispatchMessageW(message);
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MessagePumpEvent, ThreadMessage, WindowsMessagePump, process, timeout_milliseconds,
    };
    use std::time::Duration;

    use windows::Win32::{
        Foundation::{LPARAM, WPARAM},
        UI::WindowsAndMessaging::{MSG, WM_APP, WM_NULL},
    };

    #[test]
    fn finite_waits_round_up_to_the_next_millisecond() {
        assert_eq!(timeout_milliseconds(Duration::from_nanos(1)), 1);
        assert_eq!(timeout_milliseconds(Duration::from_millis(1)), 1);
        assert_eq!(timeout_milliseconds(Duration::from_micros(1_001)), 2);
    }

    #[test]
    fn finite_waits_never_use_the_infinite_timeout_value() {
        assert_eq!(timeout_milliseconds(Duration::MAX), u32::MAX - 1);
    }

    #[test]
    fn a_message_without_a_window_is_returned_to_the_thread_owner() {
        let message = MSG {
            message: WM_APP + 7,
            wParam: WPARAM(11),
            lParam: LPARAM(13),
            ..Default::default()
        };

        assert!(matches!(
            process(message),
            MessagePumpEvent::ThreadMessage(ThreadMessage {
                message,
                wparam: 11,
                lparam: 13,
            }) if message == WM_APP + 7
        ));
    }

    #[test]
    fn a_waker_interrupts_its_owning_thread_queue() {
        let mut pump = WindowsMessagePump::new();
        let waker = pump.waker();

        std::thread::spawn(move || waker.wake())
            .join()
            .expect("the waking thread exits normally")
            .expect("the live message pump accepts a cross-thread wake");

        assert!(matches!(
            pump.wait().expect("the posted wake can be received"),
            MessagePumpEvent::ThreadMessage(ThreadMessage {
                message: WM_NULL,
                wparam: 0,
                lparam: 0,
            })
        ));
    }

    #[test]
    fn a_waker_does_not_keep_its_message_pump_available() {
        let pump = WindowsMessagePump::new();
        let waker = pump.waker();
        drop(pump);

        assert_eq!(
            waker.wake().unwrap_err().kind(),
            std::io::ErrorKind::BrokenPipe
        );
    }
}
