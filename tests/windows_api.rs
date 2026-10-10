#![cfg(target_os = "windows")]

use std::any::TypeId;

#[test]
fn existing_paths_reexport_the_standalone_windows_types() {
    assert_eq!(
        TypeId::of::<theoria::windows::Window>(),
        TypeId::of::<theoria_windows::Window>()
    );
    assert_eq!(
        TypeId::of::<theoria::windows::WindowGroup>(),
        TypeId::of::<theoria_windows::WindowGroup>()
    );
    assert_eq!(
        TypeId::of::<theoria::windows::Win32EventLoop>(),
        TypeId::of::<theoria_windows::Win32EventLoop>()
    );
    assert_eq!(
        TypeId::of::<theoria::windows::Win32WindowMessageSource>(),
        TypeId::of::<theoria_windows::Win32WindowMessageSource>()
    );
}
