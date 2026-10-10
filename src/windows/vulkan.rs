//! Vulkan connection for the backend-independent Win32 Window frontend.

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use crate::{
    component::MountContext,
    event_loop::EventLoopHandle,
    reactive::Computed,
    surface::{SurfaceContent, SurfaceSize, Viewport},
    windows::{
        Result, capabilities::WindowCapabilities, group::WindowGroupContext,
        native_window::Win32Window, presentation::WindowPresentation, window::Window,
    },
};

use theoria_vulkan::{CanvasRenderer, PresentOutcome, Win32PresentationTarget, Win32Presenter};

struct VulkanPresentation {
    presenter: Win32Presenter,
    canvas_renderer: CanvasRenderer,
    output: Rc<RenderOutput>,
    pending: bool,
}

impl Window {
    /// Creates a hidden Win32 Window with Vulkan presentation and mounts its tree.
    ///
    /// The returned Computed value supplies the Window's Canvas output.
    /// Call [`Window::show`] when the Window is ready to become visible.
    pub fn new(
        event_loop: &EventLoopHandle,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities) -> Computed<SurfaceContent>,
    ) -> Result<Self> {
        Self::mount(
            event_loop,
            title,
            width,
            height,
            VulkanPresentation::new,
            |context, capabilities, presentation| {
                let content = build(context, capabilities);
                presentation.bind(context, content);
            },
        )
    }
}

impl WindowGroupContext<'_, '_> {
    /// Creates a hidden Window with Vulkan presentation and mounts its subtree.
    pub fn mount_window(
        &mut self,
        title: &str,
        width: u32,
        height: u32,
        build: impl FnOnce(&mut MountContext<'_>, &WindowCapabilities) -> Computed<SurfaceContent>,
    ) -> Result<()> {
        self.mount_window_with(
            title,
            width,
            height,
            VulkanPresentation::new,
            |context, capabilities, presentation| {
                let content = build(context, capabilities);
                presentation.bind(context, content);
            },
        )
    }
}

impl VulkanPresentation {
    fn new(window: &Win32Window) -> Result<Self> {
        let (hwnd, hinstance) = window.raw_handles();
        // WindowHost owns the native Window and drops presentation first.
        let target = unsafe {
            Win32PresentationTarget::from_raw(hwnd.0 as isize, hinstance.0 as isize, window.size())
        };
        let presenter = Win32Presenter::new(target)?;
        let canvas_renderer = CanvasRenderer::new(presenter.device());
        Ok(Self {
            presenter,
            canvas_renderer,
            output: Rc::new(RenderOutput::default()),
            pending: false,
        })
    }

    fn bind(&self, context: &mut MountContext<'_>, content: Computed<SurfaceContent>) {
        let output = Rc::clone(&self.output);
        let effect = context.create_effect(move |context| {
            output
                .content
                .replace(Some(context.read(&content).cloned()));
            output.redraw_needed.set(true);
        });
        context.sync(&effect);
    }
}

impl WindowPresentation for VulkanPresentation {
    fn mark_pending(&mut self) {
        self.pending = true;
    }

    fn resize(&mut self, size: SurfaceSize) {
        self.presenter.resize(size);
    }

    fn has_pending(&self) -> bool {
        self.pending || self.presenter.has_retired_swapchain()
    }

    fn take_redraw_request(&self) -> bool {
        !self.pending && self.output.redraw_needed.replace(false)
    }

    fn advance(&mut self, size: SurfaceSize, wait: Duration) -> Result<()> {
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
}

impl VulkanPresentation {
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
