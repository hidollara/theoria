//! Win32 message pumping for one GUI thread.

use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    fmt,
    marker::PhantomData,
    rc::{Rc, Weak},
    sync::OnceLock,
    sync::{Arc, Mutex, Weak as SyncWeak},
    time::Duration,
};

use ::windows::{
    Win32::{
        Foundation::{HWND, LPARAM, WAIT_FAILED, WAIT_TIMEOUT, WPARAM},
        System::Threading::GetCurrentThreadId,
        UI::WindowsAndMessaging::{
            DispatchMessageW, GetMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx,
            PM_NOREMOVE, PM_REMOVE, PeekMessageW, PostThreadMessageW, QS_ALLINPUT,
            RegisterWindowMessageW, TranslateMessage, WM_NULL, WM_QUIT, WM_USER,
        },
    },
    core::{Error, Result, w},
};

use crate::event::EventSource;
use crate::event_loop::pump::{EventPump, EventPumpWake, PumpEvent, PumpResult};
use crate::windows::message::{Win32Message, Win32QueuedMessage, Win32ThreadMessage};

/// Drives the Win32 message queue belonging to the current GUI thread.
///
/// One thread queue services every Window created on that thread. This access
/// object also receives messages posted directly to the thread, so neither the
/// queue nor this pump requires a Window to exist. It is deliberately `!Send`
/// and `!Sync`: Win32 associates the queue and its Windows with the thread that
/// created them.
pub(super) struct Win32MessagePump {
    wake: Arc<WakeState>,
    messages: Rc<QueuedMessageState>,
    _thread_affinity: PhantomData<Rc<()>>,
}

impl Win32MessagePump {
    pub(super) fn new() -> Self {
        // A thread message queue is created lazily by User32. Establish it now
        // so a subsequently issued cross-thread posting cannot race the first
        // blocking wait, including when this thread owns no Window.
        unsafe {
            let mut message = MSG::default();
            let _ = PeekMessageW(&mut message, None, WM_USER, WM_USER, PM_NOREMOVE);
        }
        let wake_message = wake_message();
        Self {
            wake: Arc::new(WakeState {
                thread_id: unsafe { GetCurrentThreadId() },
                message: wake_message,
                open: Mutex::new(true),
            }),
            messages: Rc::new(QueuedMessageState::new()),
            _thread_affinity: PhantomData,
        }
    }

    pub(super) fn queued_messages(&self) -> Win32QueuedMessageSource {
        Win32QueuedMessageSource {
            state: Rc::downgrade(&self.messages),
        }
    }

    pub(super) fn thread_messages(&self) -> Win32ThreadMessageSource {
        Win32ThreadMessageSource {
            state: Rc::downgrade(&self.messages),
        }
    }

    fn downgraded_wake(&self) -> SyncWeak<dyn EventPumpWake> {
        let wake: Arc<dyn EventPumpWake> = self.wake.clone();
        Arc::downgrade(&wake)
    }

    /// Waits for and processes one message from the current thread's queue.
    ///
    /// Window messages are dispatched to their Window procedures. A message
    /// without an HWND is returned to the caller as a thread message instead.
    fn wait_native(&mut self) -> Result<PumpEvent> {
        unsafe {
            let mut message = MSG::default();
            let result = GetMessageW(&mut message, None, 0, 0).0;
            if result == -1 {
                return Err(Error::from_thread());
            }
            self.publish(&message);
            if result == 0 {
                return Ok(PumpEvent::ExitRequested(message.wParam.0 as i32));
            }
            Ok(process(message))
        }
    }

    /// Waits until either native input arrives or `timeout` elapses.
    fn wait_for_native(&mut self, timeout: Duration) -> Result<PumpEvent> {
        if timeout.is_zero() {
            return Ok(self.poll_native());
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
            return Ok(PumpEvent::Idle);
        }
        Ok(self.poll_native())
    }

    /// Processes at most one ready message from the current thread's queue.
    ///
    /// Limiting one call to one message lets the owner interleave native input
    /// with other execution domains instead of allowing an active Win32 queue
    /// to monopolize its thread.
    fn poll_native(&mut self) -> PumpEvent {
        unsafe {
            let mut message = MSG::default();
            if !PeekMessageW(&mut message, None, 0, 0, PM_REMOVE).as_bool() {
                return PumpEvent::Idle;
            }
            self.publish(&message);
            if message.message == WM_QUIT {
                return PumpEvent::ExitRequested(message.wParam.0 as i32);
            }
            process(message)
        }
    }

    fn publish(&self, message: &MSG) {
        let is_internal_wake =
            message.hwnd == HWND::default() && message.message == self.wake.message;
        if !is_internal_wake {
            self.messages.publish(message);
        }
    }
}

impl EventPump for Win32MessagePump {
    fn wake_capability(&self) -> SyncWeak<dyn EventPumpWake> {
        self.downgraded_wake()
    }

    fn wait(&mut self) -> PumpResult<PumpEvent> {
        Ok(self.wait_native()?)
    }

    fn wait_for(&mut self, timeout: Duration) -> PumpResult<PumpEvent> {
        Ok(self.wait_for_native(timeout)?)
    }

    fn poll(&mut self) -> PumpResult<PumpEvent> {
        Ok(self.poll_native())
    }
}

impl Drop for Win32MessagePump {
    fn drop(&mut self) {
        *self
            .wake
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = false;
    }
}

impl EventPumpWake for WakeState {
    fn wake(&self) -> std::io::Result<()> {
        let open = self
            .open
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*open {
            return Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "the event loop is unavailable",
            ));
        }
        unsafe { PostThreadMessageW(self.thread_id, self.message, WPARAM(0), LPARAM(0)) }
            .map_err(std::io::Error::other)
    }
}

struct WakeState {
    thread_id: u32,
    message: u32,
    open: Mutex<bool>,
}

fn wake_message() -> u32 {
    static MESSAGE: OnceLock<u32> = OnceLock::new();

    *MESSAGE.get_or_init(|| {
        let message = unsafe { RegisterWindowMessageW(w!("Theoria.EventLoop.Wake")) };
        if message == 0 { WM_NULL } else { message }
    })
}

fn timeout_milliseconds(timeout: Duration) -> u32 {
    let milliseconds = timeout.as_millis();
    let rounded_up = milliseconds + u128::from(!timeout.subsec_nanos().is_multiple_of(1_000_000));
    rounded_up.clamp(1, u128::from(u32::MAX - 1)) as u32
}

fn process(message: MSG) -> PumpEvent {
    if message.hwnd == HWND::default() {
        return PumpEvent::NativeProgress;
    }
    unsafe { dispatch(&message) };
    PumpEvent::NativeProgress
}

/// A cloneable, non-owning Source of messages removed from one Win32 thread queue.
///
/// The owning [`Win32EventLoop`](crate::windows::Win32EventLoop) controls this
/// Source's lifetime. Publications run inline before queued Window messages are
/// translated and dispatched.
#[derive(Clone)]
pub struct Win32QueuedMessageSource {
    state: Weak<QueuedMessageState>,
}

impl EventSource for Win32QueuedMessageSource {
    type Event<'message> = Win32QueuedMessage<'message>;

    fn register<F>(self, receive: F) -> Option<impl FnOnce() + 'static>
    where
        F: for<'message> Fn(Self::Event<'message>) + 'static,
    {
        Some(self.state.upgrade()?.register(receive))
    }
}

impl fmt::Debug for Win32QueuedMessageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Win32QueuedMessageSource")
            .finish_non_exhaustive()
    }
}

/// A cloneable, non-owning Source of queued messages addressed to one Win32 thread.
///
/// Window-directed messages are omitted. `WM_QUIT` remains observable before
/// the EventLoop reports its exit request.
#[derive(Clone)]
pub struct Win32ThreadMessageSource {
    state: Weak<QueuedMessageState>,
}

impl EventSource for Win32ThreadMessageSource {
    type Event<'message> = Win32ThreadMessage<'message>;

    fn register<F>(self, receive: F) -> Option<impl FnOnce() + 'static>
    where
        F: for<'message> Fn(Self::Event<'message>) + 'static,
    {
        Some(self.state.upgrade()?.register(move |message| {
            if let Some(message) = message.thread_message() {
                receive(message);
            }
        }))
    }
}

impl fmt::Debug for Win32ThreadMessageSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Win32ThreadMessageSource")
            .finish_non_exhaustive()
    }
}

struct QueuedMessageState {
    next_subscription: Cell<u64>,
    #[allow(clippy::type_complexity)]
    subscriptions: RefCell<BTreeMap<u64, Rc<dyn for<'message> Fn(Win32QueuedMessage<'message>)>>>,
}

impl QueuedMessageState {
    fn new() -> Self {
        Self {
            next_subscription: Cell::new(0),
            subscriptions: RefCell::new(BTreeMap::new()),
        }
    }

    fn register<F>(self: &Rc<Self>, receive: F) -> impl FnOnce() + 'static + use<F>
    where
        F: for<'message> Fn(Win32QueuedMessage<'message>) + 'static,
    {
        let id = self.next_subscription.get();
        self.next_subscription.set(
            id.checked_add(1)
                .expect("Win32 queued-message subscription identity exhausted"),
        );
        self.subscriptions.borrow_mut().insert(id, Rc::new(receive));

        let state = Rc::downgrade(self);
        move || {
            if let Some(state) = state.upgrade() {
                state.subscriptions.borrow_mut().remove(&id);
            }
        }
    }

    fn publish(&self, message: &MSG) {
        let lifetime = ();
        let queued = Win32QueuedMessage::new(
            (message.hwnd != HWND::default()).then_some(message.hwnd),
            Win32Message::new(message.message, message.wParam, message.lParam, &lifetime),
            message.time,
            message.pt,
        );
        let subscribers = self
            .subscriptions
            .borrow()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for receive in subscribers {
            receive(queued);
        }
    }
}

unsafe fn dispatch(message: &MSG) {
    unsafe {
        let _ = TranslateMessage(message);
        DispatchMessageW(message);
    }
}

#[cfg(test)]
mod tests {
    use super::{PumpEvent, Win32MessagePump, process, timeout_milliseconds};
    use std::{cell::Cell, rc::Rc, time::Duration};

    use windows::Win32::{
        Foundation::{LPARAM, POINT, WPARAM},
        UI::WindowsAndMessaging::{MSG, WM_APP},
    };

    use crate::event::EventSource;

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
    fn a_message_without_a_window_is_published_as_a_thread_message() {
        let pump = Win32MessagePump::new();
        let observed = Rc::new(Cell::new(None));
        let output = Rc::clone(&observed);
        let _unregister = pump
            .thread_messages()
            .register(move |message| {
                output.set(Some((
                    message.message().id(),
                    message.message().wparam().0,
                    message.message().lparam().0,
                    message.posted_at(),
                    message.cursor_position(),
                )));
            })
            .expect("the owning message pump is alive");
        let queued_observed = Rc::new(Cell::new(None));
        let output = Rc::clone(&queued_observed);
        let _unregister_queued = pump
            .queued_messages()
            .register(move |message| {
                output.set(Some((
                    message.destination(),
                    message.message().id(),
                    message.posted_at(),
                )));
            })
            .expect("the owning message pump is alive");
        let message = MSG {
            message: WM_APP + 7,
            wParam: WPARAM(11),
            lParam: LPARAM(13),
            time: 17,
            pt: POINT { x: 19, y: 23 },
            ..Default::default()
        };

        pump.messages.publish(&message);

        assert_eq!(
            observed.get(),
            Some((WM_APP + 7, 11, 13, 17, POINT { x: 19, y: 23 }))
        );
        assert_eq!(queued_observed.get(), Some((None, WM_APP + 7, 17)));
        assert_eq!(process(message), PumpEvent::NativeProgress);
    }

    #[test]
    fn a_waker_interrupts_its_owning_thread_queue() {
        let mut pump = Win32MessagePump::new();
        let waker = pump.downgraded_wake();
        let publications = Rc::new(Cell::new(0));
        let output = Rc::clone(&publications);
        let _unregister = pump
            .thread_messages()
            .register(move |_| output.set(output.get() + 1))
            .expect("the owning message pump is alive");

        std::thread::spawn(move || waker.upgrade().unwrap().wake())
            .join()
            .expect("the waking thread exits normally")
            .expect("the live message pump accepts a cross-thread wake");

        assert!(matches!(
            pump.wait_native().expect("the posted wake can be received"),
            PumpEvent::NativeProgress
        ));
        assert_eq!(publications.get(), 0);
    }

    #[test]
    fn a_waker_does_not_keep_its_message_pump_available() {
        let pump = Win32MessagePump::new();
        let waker = pump.downgraded_wake();
        drop(pump);

        assert!(waker.upgrade().is_none());
    }
}
