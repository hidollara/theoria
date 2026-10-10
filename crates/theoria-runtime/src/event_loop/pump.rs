//! Contract between EventLoop coordination and a platform pump.

use std::{error::Error, io, sync::Weak, time::Duration};

pub type PumpResult<T> = Result<T, Box<dyn Error>>;

/// One platform event observed while driving the current thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PumpEvent {
    NativeProgress,
    Idle,
    ExitRequested(i32),
}

/// Platform operations needed by the EventLoop coordinator.
pub trait EventPump {
    fn wake_capability(&self) -> Weak<dyn EventPumpWake>;
    fn wait(&mut self) -> PumpResult<PumpEvent>;
    fn wait_for(&mut self, timeout: Duration) -> PumpResult<PumpEvent>;
    fn poll(&mut self) -> PumpResult<PumpEvent>;
}

/// Thread-safe wake capability supplied by one platform event pump.
pub trait EventPumpWake: Send + Sync {
    fn wake(&self) -> io::Result<()>;
}
