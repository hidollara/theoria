//! Win32 message pumping for one GUI thread.

use std::{marker::PhantomData, rc::Rc, time::Duration};

use ::windows::{
    Win32::{
        Foundation::{WAIT_FAILED, WAIT_TIMEOUT},
        UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
            PM_REMOVE, PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
        },
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

    /// Waits until either native input arrives or `timeout` elapses.
    pub(crate) fn wait_for(&mut self, timeout: Duration) -> Result<MessagePumpProgress> {
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
            return Ok(MessagePumpProgress::Idle);
        }
        Ok(self.poll())
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

fn timeout_milliseconds(timeout: Duration) -> u32 {
    let milliseconds = timeout.as_millis();
    let rounded_up = milliseconds + u128::from(!timeout.subsec_nanos().is_multiple_of(1_000_000));
    rounded_up.clamp(1, u128::from(u32::MAX - 1)) as u32
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

#[cfg(test)]
mod tests {
    use super::timeout_milliseconds;
    use std::time::Duration;

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
}
