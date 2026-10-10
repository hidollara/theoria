use std::any::TypeId;

#[test]
fn existing_paths_keep_the_standalone_vulkan_types() {
    assert_eq!(
        TypeId::of::<theoria::vulkan::CanvasRenderer>(),
        TypeId::of::<theoria_vulkan::CanvasRenderer>(),
    );
    assert_eq!(
        TypeId::of::<theoria::vulkan::ImageSurface>(),
        TypeId::of::<theoria_vulkan::ImageSurface>(),
    );
    assert_eq!(
        TypeId::of::<theoria::windows::Win32PresentationTarget>(),
        TypeId::of::<theoria_vulkan::Win32PresentationTarget>(),
    );
    assert_eq!(
        TypeId::of::<theoria::surface::SurfaceContent>(),
        TypeId::of::<theoria_renderer::SurfaceContent>(),
    );
}
