//! Platform-independent Window observations.

/// A platform-independent request to close a Window.
///
/// Subscribers may observe the request. The built-in Win32 Window also
/// installs an ordinary Component subscription that unmounts it as the
/// default, non-cancellable response.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CloseRequested;
