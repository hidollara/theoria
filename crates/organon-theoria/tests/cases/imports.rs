use std::cell::Cell;
use theoria_reactive::Reactive;

mod definitions {
    organon_theoria::library!(crate::definitions);
}

thread_local! { static EVALUATIONS: Cell<usize> = const { Cell::new(0) }; }
#[organon_theoria::implement]
impl Scale {
    #[output(name = "scaled")]
    fn scaled(input: Input<'_, Self>) -> f32 {
        EVALUATIONS.set(EVALUATIONS.get() + 1);
        *input.value() * 2.0
    }
    forward!(value => original);
}

#[test]
fn imports_construct_each_instance_once_and_share_multiple_outputs() {
    EVALUATIONS.set(0);
    let reactive = Reactive::new();
    let scene = definitions::Scene::builder()
        .width(4.0)
        .build(&reactive)
        .unwrap();
    assert_eq!(EVALUATIONS.get(), 1);
    assert_eq!(scene.extent_fixed(), Some(&8.0));
    assert_eq!(scene.original_fixed(), Some(&4.0));
    assert_eq!(scene.literal_extent_fixed(), Some(&3.0));
    assert!(scene.path_fixed().unwrap().is_ok());
    let other = definitions::Scene::builder()
        .width(5.0)
        .build(&reactive)
        .unwrap();
    assert_eq!(EVALUATIONS.get(), 2);
    assert_eq!(other.extent_fixed(), Some(&10.0));
    assert_eq!(scene.extent_fixed(), Some(&8.0));
}

#[test]
fn imported_outputs_preserve_reactivity_and_result_recovery() {
    EVALUATIONS.set(0);
    let reactive = Reactive::new();
    let width = reactive.create_state(4.0);
    let scene = definitions::Scene::builder()
        .width(width.value())
        .build(&reactive)
        .unwrap();
    // Each consuming output may evaluate Scale's inline calculation during binding.
    assert!(EVALUATIONS.get() > 0);
    assert!(scene.path_fixed().is_none());
    assert_eq!(scene.literal_extent_fixed(), Some(&3.0));
    let path = scene.path();
    let extent = scene.extent();
    let read = reactive.create_computed(move |cx| {
        (
            path.read(&organon_theoria::ReadContext::new(cx)).cloned(),
            *extent.read(&organon_theoria::ReadContext::new(cx)),
        )
    });
    assert_eq!(read.snapshot().unwrap().1, 8.0);
    width.set(-1.0).unwrap();
    assert!(read.snapshot().unwrap().0.is_err());
    width.set(6.0).unwrap();
    let result = read.snapshot().unwrap();
    assert!(result.0.is_ok());
    assert_eq!(result.1, 12.0);
    let evaluations = EVALUATIONS.get();
    assert!(evaluations > 1);
    read.snapshot().unwrap();
    assert_eq!(EVALUATIONS.get(), evaluations);
}
