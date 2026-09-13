//! Reactive Canvas output and GPU presentation for one Win32 Window.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use crate::{
    component::MountContext,
    reactive::Computed,
    surface::{SurfaceContent, SurfaceSize, Viewport},
    vulkan::{CanvasRenderer, PresentOutcome, Win32Presenter},
};

use crate::windows::{Result, presentation_target::Win32PresentationTarget};

pub(super) struct WindowPresentation {
    presenter: Win32Presenter,
    canvas_renderer: CanvasRenderer,
    output: Rc<RenderOutput>,
    pending: bool,
}

impl WindowPresentation {
    pub(super) fn new(target: Win32PresentationTarget) -> Result<Self> {
        let presenter = Win32Presenter::new(target)?;
        let canvas_renderer = CanvasRenderer::new(presenter.device());
        Ok(Self {
            presenter,
            canvas_renderer,
            output: Rc::new(RenderOutput::default()),
            pending: false,
        })
    }

    pub(super) fn bind(&self, context: &mut MountContext<'_>, content: Computed<SurfaceContent>) {
        let output = Rc::clone(&self.output);
        let effect = context.create_effect(move |context| {
            output
                .content
                .replace(Some(context.read(&content).cloned()));
            output.redraw_needed.set(true);
        });
        context.sync(&effect);
    }

    pub(super) fn mark_pending(&mut self) {
        self.pending = true;
    }

    pub(super) fn resize(&mut self, size: SurfaceSize) -> Result<()> {
        self.presenter.resize(size)
    }

    pub(super) fn has_pending(&self) -> bool {
        self.pending || self.presenter.has_retired_swapchain()
    }

    pub(super) fn take_redraw_request(&self) -> bool {
        !self.pending && self.output.redraw_needed.replace(false)
    }

    pub(super) fn advance(&mut self, size: SurfaceSize, wait: Duration) -> Result<()> {
        if self.pending {
            self.pending = match self.present_latest(size, wait)? {
                PresentationProgress::Complete => false,
                PresentationProgress::Pending => true,
            };
            self.presenter.reap_retired_swapchain(Duration::ZERO)?;
        } else {
            self.presenter.reap_retired_swapchain(wait)?;
        }
        Ok(())
    }

    fn present_latest(
        &mut self,
        size: SurfaceSize,
        wait: Duration,
    ) -> Result<PresentationProgress> {
        if size.width == 0 || size.height == 0 {
            self.output.redraw_needed.set(false);
            return Ok(PresentationProgress::Complete);
        }
        let content = self
            .output
            .content
            .borrow()
            .as_ref()
            .expect("the Window presentation Effect produces initial content")
            .clone();
        let viewport = Viewport::new(0.0, 0.0, size.width as f32, size.height as f32);

        let Some(frame) = self.presenter.begin_frame(wait)? else {
            return Ok(PresentationProgress::Pending);
        };
        let target = frame.render_target(content.clear_color());
        self.canvas_renderer
            .render(target, viewport, content.canvas(), content.view())?;
        let outcome = frame.present()?;
        if matches!(outcome, PresentOutcome::SwapchainInvalidated) {
            return Ok(PresentationProgress::Pending);
        }

        self.output.redraw_needed.set(false);
        Ok(PresentationProgress::Complete)
    }
}

enum PresentationProgress {
    Complete,
    Pending,
}

#[derive(Default)]
struct RenderOutput {
    content: RefCell<Option<SurfaceContent>>,
    redraw_needed: Cell<bool>,
}
