//! Common and queued Win32 message representations.

use std::{fmt, marker::PhantomData};

use ::windows::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};

/// Destination-independent contents of one Win32 message.
///
/// The message identifier determines how `WPARAM` and `LPARAM` are interpreted.
/// Any data referenced by those parameters is valid only for the lifetime
/// carried by this value.
#[derive(Clone, Copy)]
pub struct Win32Message<'message> {
    id: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    lifetime: PhantomData<&'message ()>,
}

impl<'message> Win32Message<'message> {
    pub(crate) const fn new(
        id: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _lifetime: &'message (),
    ) -> Self {
        Self {
            id,
            wparam,
            lparam,
            lifetime: PhantomData,
        }
    }

    /// Returns the Win32 message identifier.
    pub const fn id(&self) -> u32 {
        self.id
    }

    /// Returns the message's raw `WPARAM`.
    pub const fn wparam(&self) -> WPARAM {
        self.wparam
    }

    /// Returns the message's raw `LPARAM`.
    pub const fn lparam(&self) -> LPARAM {
        self.lparam
    }

    /// Interprets `LPARAM` as a reference valid for this message observation.
    ///
    /// # Safety
    ///
    /// The caller must verify from the Win32 contract for [`Self::id`] that
    /// `LPARAM` is a non-null, correctly aligned pointer to a live `T`, and
    /// that reading it does not violate aliasing requirements. This observation
    /// interface never grants mutable access to native message storage.
    pub unsafe fn lparam_ref<T>(&self) -> Option<&'message T> {
        unsafe { (self.lparam.0 as *const T).as_ref() }
    }
}

impl fmt::Debug for Win32Message<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Win32Message")
            .field("id", &self.id)
            .field("wparam", &self.wparam)
            .field("lparam", &self.lparam)
            .finish()
    }
}

/// One message removed from a Win32 thread message queue.
///
/// This corresponds to the public fields of Win32's `MSG` structure. A Window
/// destination is absent for a thread message. This observation happens before
/// a queued Window message is translated and dispatched to its Window
/// procedure.
#[derive(Clone, Copy, Debug)]
pub struct Win32QueuedMessage<'message> {
    destination: Option<HWND>,
    message: Win32Message<'message>,
    posted_at: u32,
    cursor_position: POINT,
}

impl<'message> Win32QueuedMessage<'message> {
    pub(crate) const fn new(
        destination: Option<HWND>,
        message: Win32Message<'message>,
        posted_at: u32,
        cursor_position: POINT,
    ) -> Self {
        Self {
            destination,
            message,
            posted_at,
            cursor_position,
        }
    }

    /// Returns the Window destination, or `None` for a thread message.
    pub const fn destination(&self) -> Option<HWND> {
        self.destination
    }

    /// Returns the destination-independent message contents.
    pub const fn message(&self) -> &Win32Message<'message> {
        &self.message
    }

    /// Returns the Win32 timestamp at which the message was posted.
    pub const fn posted_at(&self) -> u32 {
        self.posted_at
    }

    /// Returns the cursor's screen-coordinate position when the message was posted.
    pub const fn cursor_position(&self) -> POINT {
        self.cursor_position
    }

    /// Narrows this queued message when it is addressed to the thread itself.
    pub const fn thread_message(&self) -> Option<Win32ThreadMessage<'message>> {
        if self.destination.is_none() {
            Some(Win32ThreadMessage { queued: *self })
        } else {
            None
        }
    }
}

/// One queued message addressed to a Win32 thread rather than a Window.
#[derive(Clone, Copy, Debug)]
pub struct Win32ThreadMessage<'message> {
    queued: Win32QueuedMessage<'message>,
}

impl<'message> Win32ThreadMessage<'message> {
    /// Returns the destination-independent message contents.
    pub const fn message(&self) -> &Win32Message<'message> {
        self.queued.message()
    }

    /// Returns the Win32 timestamp at which the message was posted.
    pub const fn posted_at(&self) -> u32 {
        self.queued.posted_at()
    }

    /// Returns the cursor's screen-coordinate position when the message was posted.
    pub const fn cursor_position(&self) -> POINT {
        self.queued.cursor_position()
    }
}
