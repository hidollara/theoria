use std::any::TypeId;

#[test]
fn existing_paths_keep_the_standalone_font_and_text_types() {
    assert_eq!(
        TypeId::of::<theoria::font::Font>(),
        TypeId::of::<theoria_text::font::Font>(),
    );
    assert_eq!(
        TypeId::of::<theoria::text::Text>(),
        TypeId::of::<theoria_text::text::Text>(),
    );
    assert_eq!(
        TypeId::of::<theoria::text::TextStyle>(),
        TypeId::of::<theoria_text::text::TextStyle>(),
    );
}
