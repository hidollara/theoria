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
fn an_empty_sequence_is_valid_with_many_children() {
    let reactive = theoria_reactive::Reactive::new();
    let scene = ui::Empty::builder().build(&reactive).unwrap();
    assert_eq!(scene.text_fixed().map(String::as_str), Some("[/]"));
}

#[test]
fn optional_children_forward_zero_or_one_element() {
    let reactive = theoria_reactive::Reactive::new();
    let scene = ui::OptionalScene::builder().build(&reactive).unwrap();
    assert_eq!(scene.empty_fixed().map(String::as_str), Some(""));
    assert_eq!(scene.single_fixed().map(String::as_str), Some("x"));
}

#[test]
fn empty_and_degenerate_polygons_follow_theoria_paths() {
    use theoria_canvas::geometry::Point;
    use theoria_canvas::path::Path;
    let reactive = theoria_reactive::Reactive::new();
    let scene = ui::DegeneratePolygons::builder().build(&reactive).unwrap();
    let empty = scene.empty_fixed().unwrap().as_ref().unwrap();
    let point = scene.point_fixed().unwrap().as_ref().unwrap();
    let line = scene.line_fixed().unwrap().as_ref().unwrap();
    assert!(empty.is_empty());
    assert_eq!(
        point.subpaths(),
        Path::polygon([Point::new(1.0, 2.0)]).subpaths()
    );
    assert_eq!(
        line.subpaths(),
        Path::polygon([Point::new(1.0, 2.0), Point::new(3.0, 4.0)]).subpaths()
    );
}

fn solid_rgb(canvas: &theoria_canvas::Canvas) -> [u8; 3] {
    use theoria_canvas::{CanvasNode, ColorField, ColorValue};
    let CanvasNode::Draw(draw) = canvas.node() else {
        panic!("expected a draw")
    };
    let ColorField::Solid(color) = &draw.source else {
        panic!("expected a solid color")
    };
    let ColorValue::U8(rgb, _) = color.value() else {
        panic!("expected RGB8")
    };
    *rgb
}

fn composition_layers(canvas: &theoria_canvas::Canvas) -> Vec<[u8; 3]> {
    use theoria_canvas::{CanvasNode, Composite, color::Srgb};
    match canvas.node() {
        CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } => {
            assert_eq!(composition.composite(), Composite::SourceOver);
            assert_eq!(composition.composite_space(), &Srgb.into());
            let mut layers = composition_layers(backdrop);
            layers.push(solid_rgb(source));
            layers
        }
        _ => vec![solid_rgb(canvas)],
    }
}

#[test]
fn source_over_handles_empty_single_and_ordered_layers() {
    use theoria_canvas::{CanvasNode, Color, ColorField, Opacity, color::Srgb};
    let reactive = theoria_reactive::Reactive::new();
    let color = Color::new(Srgb, [255u8, 0, 0], Opacity::<u8>::ONE);
    let scene = ui::DrawingChildren::builder()
        .color(color)
        .build(&reactive)
        .unwrap();
    let empty = scene.empty_fixed().unwrap().as_ref().unwrap();
    let CanvasNode::Draw(draw) = empty.node() else {
        panic!("expected transparent draw")
    };
    let ColorField::Solid(color) = &draw.source else {
        panic!()
    };
    assert_eq!(color.value().alpha().as_f32(), 0.0);
    let original = scene.original_fixed().unwrap().as_ref().unwrap();
    let single = scene.single_fixed().unwrap().as_ref().unwrap();
    assert_eq!(solid_rgb(single), solid_rgb(original));
    assert_eq!(
        composition_layers(scene.pair_fixed().unwrap().as_ref().unwrap()),
        [[255, 0, 0], [0, 255, 0]]
    );
    assert_eq!(
        composition_layers(scene.many_fixed().unwrap().as_ref().unwrap()),
        [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
    );
}

#[test]
fn source_over_layers_retain_reactive_dependencies() {
    use theoria_canvas::{Color, Opacity, color::Srgb};
    let reactive = theoria_reactive::Reactive::new();
    let color = reactive.create_state(Color::new(Srgb, [255u8, 0, 0], Opacity::<u8>::ONE));
    let scene = ui::DrawingChildren::builder()
        .color(color.value())
        .build(&reactive)
        .unwrap();
    assert!(scene.empty_fixed().unwrap().is_ok());
    let drawing = scene.many();
    let layers = reactive.create_computed(move |cx| {
        composition_layers(drawing.read(&ReadContext::new(cx)).as_ref().unwrap())
    });
    assert_eq!(
        *layers.snapshot().unwrap(),
        [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
    );
    color
        .set(Color::new(Srgb, [255u8, 255, 0], Opacity::<u8>::ONE))
        .unwrap();
    assert_eq!(
        *layers.snapshot().unwrap(),
        [[255, 255, 0], [0, 255, 0], [0, 0, 255]]
    );
}
