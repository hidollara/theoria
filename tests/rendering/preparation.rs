use theoria_canvas::canvas::{Color, ColorValue, Opacity};
use theoria_color::IccRenderingIntent;
use theoria_renderer::color_space::PreparedColors;

#[test]
fn preparing_an_icc_color_does_not_rewrite_authored_data() {
    let profile = theoria_color::IccProfile::from_bytes(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
    )))
    .unwrap();
    let color = Color::new_in_space::<theoria_color::Rgb>(
        profile.color_space(IccRenderingIntent::RelativeColorimetric),
        vec![0.25, 0.5, 0.75],
        Opacity::<f32>::ONE,
    )
    .unwrap();
    let original = color.clone();
    let mut prepared = PreparedColors::default();
    prepared.prepare(&color).unwrap();
    assert_eq!(color, original);
    assert!(prepared.lower(&color)[0] > 0.0);
    assert!(matches!(
        color.value(),
        ColorValue::Dynamic(coordinates, _) if coordinates.as_ref() == [0.25, 0.5, 0.75]
    ));
}
