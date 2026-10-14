use organon::{Implementation, output};
use std::cell::Cell;
use theoria_reactive::Reactive;

mod definitions {
    organon::import!("tests/fixtures/scene.xml", implementations { Scale = crate::ScaleImpl });
}

thread_local! { static BUILDS: Cell<usize> = const { Cell::new(0) }; }
struct ScaleImpl;
impl Implementation<definitions::Scale> for ScaleImpl {
    fn build(&self, input: definitions::Input<definitions::Scale>) -> definitions::Scale {
        BUILDS.with(|n| n.set(n.get() + 1));
        let value = input.value().clone();
        let original = value.clone();
        definitions::Scale {
            scaled: output(move |cx| *value.read(cx) * 2.0),
            original: output(move |cx| *original.read(cx)),
        }
    }
}

#[test]
fn imports_construct_each_instance_once_and_share_multiple_outputs() {
    BUILDS.set(0);
    let reactive = Reactive::new();
    let scene = definitions::Scene::builder()
        .width(4.0)
        .build(&reactive)
        .unwrap();
    assert_eq!(BUILDS.get(), 1);
    assert_eq!(scene.extent_fixed(), Some(&8.0));
    assert_eq!(scene.original_fixed(), Some(&4.0));
    assert_eq!(scene.literal_extent_fixed(), Some(&3.0));
    assert!(scene.path_fixed().unwrap().is_ok());
    let other = definitions::Scene::builder()
        .width(5.0)
        .build(&reactive)
        .unwrap();
    assert_eq!(BUILDS.get(), 2);
    assert_eq!(other.extent_fixed(), Some(&10.0));
    assert_eq!(scene.extent_fixed(), Some(&8.0));
}

#[test]
fn imported_outputs_preserve_reactivity_and_result_recovery() {
    BUILDS.set(0);
    let reactive = Reactive::new();
    let width = reactive.create_state(4.0);
    let scene = definitions::Scene::builder()
        .width(width.value())
        .build(&reactive)
        .unwrap();
    assert_eq!(BUILDS.get(), 1);
    assert!(scene.path_fixed().is_none());
    assert_eq!(scene.literal_extent_fixed(), Some(&3.0));
    let path = scene.path();
    let extent = scene.extent();
    let read = reactive.create_computed(move |cx| (cx.read(&path).cloned(), *cx.read(&extent)));
    assert_eq!(read.snapshot().unwrap().1, 8.0);
    width.set(-1.0).unwrap();
    assert!(read.snapshot().unwrap().0.is_err());
    width.set(6.0).unwrap();
    let result = read.snapshot().unwrap();
    assert!(result.0.is_ok());
    assert_eq!(result.1, 12.0);
    assert_eq!(BUILDS.get(), 1);
}
