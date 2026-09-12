//! Private Windows platform implementations.

pub(crate) mod message_pump;
mod window;

pub(crate) use window::create_window;
