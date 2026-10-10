use std::any::TypeId;

#[test]
fn geometry_reexports_preserve_type_identity() {
    assert_eq!(
        TypeId::of::<theoria::geometry::Point>(),
        TypeId::of::<theoria_geometry::geometry::Point>(),
    );
    assert_eq!(
        TypeId::of::<theoria_canvas::path::Path>(),
        TypeId::of::<theoria_geometry::path::Path>(),
    );
    assert_eq!(
        TypeId::of::<theoria::transform::Projective2>(),
        TypeId::of::<theoria_geometry::transform::Projective2>(),
    );
}
