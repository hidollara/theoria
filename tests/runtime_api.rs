use std::any::TypeId;

#[test]
fn existing_paths_reexport_runtime_types() {
    assert_eq!(
        TypeId::of::<theoria::event_loop::EventLoop>(),
        TypeId::of::<theoria_runtime::event_loop::EventLoop>()
    );
    assert_eq!(
        TypeId::of::<theoria::animation::FrameClock>(),
        TypeId::of::<theoria_runtime::animation::FrameClock>()
    );
    assert_eq!(
        TypeId::of::<theoria::input::pointer::PointerEvent>(),
        TypeId::of::<theoria_runtime::input::pointer::PointerEvent>()
    );
    assert_eq!(
        TypeId::of::<theoria::surface::Surface>(),
        TypeId::of::<theoria_runtime::surface::Surface>()
    );
    assert_eq!(
        TypeId::of::<theoria::window::CloseRequested>(),
        TypeId::of::<theoria_runtime::window::CloseRequested>()
    );
}
