//! Internal contract between EventLoop coordination and a platform event pump.

use std::{error::Error, io, sync::Weak, time::Duration};

pub(crate) type PumpResult<T> = Result<T, Box<dyn Error>>;

/// One platform event observed while driving the current thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PumpEvent {
    NativeDispatched,
    ThreadMessage(ThreadMessage),
    Idle,
    ExitRequested(i32),
}

/// One event addressed to the thread rather than a native Window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ThreadMessage {
    pub(crate) message: u32,
    pub(crate) wparam: usize,
    pub(crate) lparam: isize,
}

/// Platform operations needed by the EventLoop coordinator.
pub(crate) trait EventPump {
    fn wake_capability(&self) -> Weak<dyn EventPumpWake>;
    fn wait(&mut self) -> PumpResult<PumpEvent>;
    fn wait_for(&mut self, timeout: Duration) -> PumpResult<PumpEvent>;
    fn poll(&mut self) -> PumpResult<PumpEvent>;
}

/// Thread-safe wake capability supplied by one platform event pump.
pub(crate) trait EventPumpWake: Send + Sync {
    fn wake(&self) -> io::Result<()>;
}
