use organon_theoria::ReadContext;
use std::cell::Cell;

mod ui {
    organon_theoria::library!(crate::ui);
}

thread_local! { static EVALUATIONS: Cell<usize> = const { Cell::new(0) }; }

#[organon_theoria::implement]
impl Count {
    #[output(name = "text")]
    #[output(name = "text", interface = "Label")]
    fn text(input: Input<'_, Self>) -> String {
        let text = input.text().to_string();
        EVALUATIONS.with(|count| count.set(count.get() + 1));
        text
    }
}

#[organon_theoria::implement]
impl Join {
    #[output(name = "text")]
    fn text(input: Input<'_, Self>) -> String {
        input
            .children()
            .iter()
            .map(|child| input.read(&child.text()).to_string())
            .collect()
    }
}

#[test]
fn native_and_interface_outputs_share_one_computation_per_element() {
    use organon_theoria::Provide;

    EVALUATIONS.with(|count| count.set(0));
    let reactive = theoria_reactive::Reactive::new();
    let input = reactive.create_state("first".to_owned());
    let count = ui::Count::builder()
        .text(input.value())
        .build(&reactive)
        .unwrap();
    let label: ui::types::Label = count.provide();
    let direct = count.text();
    let projected = label.text();
    let read = reactive.create_computed(move |cx| {
        let cx = ReadContext::new(cx);
        (
            direct.read(&cx).into_cloned(),
            projected.read(&cx).into_cloned(),
        )
    });
    assert_eq!(*read.snapshot().unwrap(), ("first".into(), "first".into()));
    EVALUATIONS.with(|count| assert_eq!(count.get(), 1));
    input.set("next".into()).unwrap();
    assert_eq!(*read.snapshot().unwrap(), ("next".into(), "next".into()));
    EVALUATIONS.with(|count| assert_eq!(count.get(), 2));

    let other = ui::Count::builder()
        .text("other".to_owned())
        .build(&reactive)
        .unwrap();
    assert_eq!(other.text_fixed().unwrap(), "other");
    EVALUATIONS.with(|count| assert_eq!(count.get(), 3));
}

#[test]
fn ordered_splices_share_children_and_preserve_reactive_updates() {
    EVALUATIONS.with(|count| count.set(0));
    let reactive = theoria_reactive::Reactive::new();
    let label = reactive.create_state("a".to_owned());
    let scene = ui::Scene::builder()
        .label(label.value())
        .build(&reactive)
        .unwrap();
    let source = scene.text();
    let result =
        reactive.create_computed(move |cx| source.read(&ReadContext::new(cx)).into_cloned());
    assert_eq!(&*result.snapshot().unwrap(), "[ab/ab]");
    assert_eq!(&*result.snapshot().unwrap(), "[ab/ab]");
    EVALUATIONS.with(|count| assert_eq!(count.get(), 5));
    label.set("z".into()).unwrap();
    assert_eq!(&*result.snapshot().unwrap(), "[zb/zb]");
    EVALUATIONS.with(|count| assert_eq!(count.get(), 6));
}

#[test]
fn an_empty_sequence_is_valid_without_a_minimum() {
    let reactive = theoria_reactive::Reactive::new();
    let scene = ui::Empty::builder().build(&reactive).unwrap();
    assert_eq!(scene.text_fixed().map(String::as_str), Some("[/]"));
}
