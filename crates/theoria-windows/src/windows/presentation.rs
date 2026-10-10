//! Presentation scheduling observed by the native Window host.

use std::time::Duration;

use crate::surface::SurfaceSize;

use super::Result;

pub(super) trait WindowPresentation {
    fn mark_pending(&mut self);
    fn resize(&mut self, size: SurfaceSize);
    fn has_pending(&self) -> bool;
    fn take_redraw_request(&self) -> bool;
    fn advance(&mut self, size: SurfaceSize, wait: Duration) -> Result<()>;
}

impl WindowPresentation for () {
    fn mark_pending(&mut self) {}

    fn resize(&mut self, _size: SurfaceSize) {}

    fn has_pending(&self) -> bool {
        false
    }

    fn take_redraw_request(&self) -> bool {
        false
    }

    fn advance(&mut self, _size: SurfaceSize, _wait: Duration) -> Result<()> {
        Ok(())
    }
}
