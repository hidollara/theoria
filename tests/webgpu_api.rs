#![cfg(feature = "webgpu")]

use std::any::TypeId;

#[test]
fn existing_paths_keep_the_standalone_renderer_and_shared_extent_types() {
    assert_eq!(
        TypeId::of::<theoria::webgpu::CanvasRenderer>(),
        TypeId::of::<theoria_webgpu::CanvasRenderer>(),
    );
    assert_eq!(
        TypeId::of::<theoria::webgpu::RenderTarget<'static>>(),
        TypeId::of::<theoria_webgpu::RenderTarget<'static>>(),
    );
    assert_eq!(
        TypeId::of::<theoria::surface::SurfaceSize>(),
        TypeId::of::<theoria_renderer::SurfaceSize>(),
    );
}
