//! Win32 message pumping for one GUI thread.

use std::{marker::PhantomData, rc::Rc};

use ::windows::{
    Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
    },
    core::{Error, Result},
};

/// Drives the Win32 message queue belonging to the current GUI thread.
///
/// One thread queue services every Window created on that thread. This access
/// object is deliberately `!Send` and `!Sync`: Win32 associates the queue and
/// its Windows with the thread that created them.
pub(crate) struct WindowsMessagePump {
    _thread_affinity: PhantomData<Rc<()>>,
}

impl WindowsMessagePump {
    pub(crate) fn new() -> Self {
        Self {
            _thread_affinity: PhantomData,
        }
    }

    /// Waits for and dispatches one message from the current thread's queue.
    pub(crate) fn wait(&mut self) -> Result<MessagePumpProgress> {
        unsafe {
            let mut message = MSG::default();
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == -1 {
                return Err(Error::from_thread());
            }
            if result == 0 {
                return Ok(MessagePumpProgress::Finished);
            }
            dispatch(&message);
            Ok(MessagePumpProgress::Dispatched)
        }
    }

    /// Dispatches every message currently ready on the current thread's queue.
    pub(crate) fn poll(&mut self) -> MessagePumpProgress {
        unsafe {
            let mut dispatched = false;
            let mut message = MSG::default();
            while PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                if message.message == WM_QUIT {
                    return MessagePumpProgress::Finished;
                }
                dispatch(&message);
                dispatched = true;
            }
            if dispatched {
                MessagePumpProgress::Dispatched
            } else {
                MessagePumpProgress::Idle
            }
        }
    }
}

pub(crate) enum MessagePumpProgress {
    Dispatched,
    Idle,
    Finished,
}

unsafe fn dispatch(message: &MSG) {
    unsafe {
        let _ = TranslateMessage(message);
        DispatchMessageW(message);
    }
}
