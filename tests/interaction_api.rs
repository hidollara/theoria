use std::any::TypeId;

#[test]
fn existing_paths_reexport_interaction_and_canvas_types() {
    assert_eq!(
        TypeId::of::<theoria::interaction::HitRegion>(),
        TypeId::of::<theoria_interaction::interaction::HitRegion>(),
    );
    assert_eq!(
        TypeId::of::<theoria::gesture::ClickGesture>(),
        TypeId::of::<theoria_interaction::gesture::ClickGesture>(),
    );
    assert_eq!(
        TypeId::of::<theoria::gesture::DragArea>(),
        TypeId::of::<theoria_interaction::gesture::DragArea>(),
    );
    assert_eq!(
        TypeId::of::<theoria::geometry::Point>(),
        TypeId::of::<theoria_canvas::geometry::Point>(),
    );
    assert_eq!(
        TypeId::of::<theoria::path::Path>(),
        TypeId::of::<theoria_canvas::path::Path>(),
    );
}
