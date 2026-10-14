use theoria_reactive::Reactive;

mod bind_sample {
    organon_theoria::library!(crate::bind_sample);
}

#[test]
fn complex_samples_compile_and_keep_named_outputs() {
    let reactive = Reactive::new();
    let bound = bind_sample::CardExample::builder()
        .fonts(vec![])
        .title("title")
        .body("body")
        .build(&reactive)
        .unwrap();
    assert_eq!(bound.caption_fixed().map(String::as_str), Some("title"));
    assert!(bound.hit_path_fixed().unwrap().is_ok());
}
