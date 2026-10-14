mod panel {
    include!("generated/panel.rs");
}
mod mark {
    include!("generated/mark.rs");
}

mod interpolated {
    include!("generated/interpolated_rect.rs");
}

#[test]
fn partial_interpolation_is_parsed_as_the_destination_attribute_type() {
    use theoria_canvas::{CanvasNode, ColorField, ColorValue, CoverageNode, geometry::Point};
    let mut input = interpolated::Inputs {
        origin: "-2".into(),
        whole: 12.0,
        fraction: "5".into(),
        rgb: "336699".into(),
        alpha: "80".into(),
    };
    let canvas = interpolated::build(&input).unwrap().drawing;
    let CanvasNode::Draw(draw) = canvas.node() else {
        panic!()
    };
    let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
        panic!()
    };
    assert_eq!(
        path.subpaths()[0].segments()[0].end(),
        Point::new(10.5, 0.0)
    );
    let ColorField::Solid(color) = &draw.source else {
        panic!()
    };
    assert_eq!(
        color.value(),
        &ColorValue::U8([0x33, 0x66, 0x99], theoria_canvas::Alpha::U8(0x80))
    );
    for invalid in ["oops", "-1", "NaN", "inf"] {
        input.fraction = invalid.into();
        assert!(interpolated::build(&input).is_err());
    }
    input.fraction = "5".into();
    input.whole = -12.0;
    assert!(interpolated::build(&input).is_err());
}
#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use theoria_canvas::{
        CanvasNode, ColorField, ColorValue, Coverage, CoverageNode, color::DisplayP3,
    };
    use theoria_reactive::Reactive;

    use super::{mark, panel};
    use theoria_canvas::{Canvas, Color, Opacity, color::Srgb, geometry::Point};

    #[test]
    fn a_drawing_without_inputs_builds_a_composite() {
        let canvas = mark::build(&mark::Inputs {}).unwrap().drawing;
        assert!(matches!(canvas.node(), CanvasNode::Composite { .. }));
    }

    fn inputs() -> panel::Inputs {
        panel::Inputs {
            width: 200.0,
            fill: Color::new(DisplayP3, [0.2_f32, 0.8, 0.3], Opacity::<f32>::ONE),
            mark: Canvas::draw(
                Color::new(Srgb, [255_u8, 255, 255], Opacity::<u8>::ZERO),
                Coverage::all(),
            ),
        }
    }

    fn background_width(canvas: &Canvas) -> f32 {
        let CanvasNode::Transform { content, .. } = canvas.node() else {
            panic!()
        };
        let CanvasNode::Composite { backdrop, .. } = content.node() else {
            panic!()
        };
        let CanvasNode::Draw(draw) = backdrop.node() else {
            panic!()
        };
        let CoverageNode::Fill { path, .. } = draw.coverage.node() else {
            panic!()
        };
        path.subpaths()[0].segments()[0].end().x
    }

    #[test]
    fn generated_canvas_preserves_colors_composition_order_and_shared_input() {
        let inputs = inputs();
        let canvas = panel::build(&inputs).unwrap().drawing;
        let CanvasNode::Transform { content, transform } = canvas.node() else {
            panic!()
        };
        let position = transform.try_transform_point(Point::new(0.0, 0.0)).unwrap();
        assert_eq!(position, Point::new(32.0, 32.0));
        let CanvasNode::Composite {
            backdrop,
            source,
            composition,
        } = content.node()
        else {
            panic!()
        };
        assert_eq!(
            composition.composite_space(),
            &theoria_canvas::color::ColorSpace::from(theoria_canvas::color::LinearSrgb)
        );
        let CanvasNode::Draw(draw) = backdrop.node() else {
            panic!()
        };
        let ColorField::Solid(color) = &draw.source else {
            panic!()
        };
        assert_eq!(
            color.value(),
            &ColorValue::U8([24, 36, 53], theoria_canvas::Alpha::U8(255))
        );
        let CanvasNode::Composite {
            backdrop, source, ..
        } = source.node()
        else {
            panic!()
        };
        let CanvasNode::Draw(draw) = backdrop.node() else {
            panic!()
        };
        let ColorField::Solid(color) = &draw.source else {
            panic!()
        };
        assert_eq!(color, &inputs.fill);
        let CanvasNode::Transform { content, .. } = source.node() else {
            panic!()
        };
        assert!(std::ptr::eq(content.node(), inputs.mark.node()));
        assert_eq!(background_width(&canvas), 200.0);
    }

    #[test]
    fn dynamic_invalid_dimensions_are_errors_not_panics() {
        for width in [-1.0, f32::NAN, f32::INFINITY] {
            let mut inputs = inputs();
            inputs.width = width;
            assert!(panel::build(&inputs).is_err());
        }
        let mut inputs = inputs();
        inputs.width = 0.0;
        assert!(panel::build(&inputs).is_ok());
    }

    #[test]
    fn rust_controls_reactivity_and_only_reads_the_active_dependency() {
        let reactive = Reactive::new();
        let expanded = reactive.create_state(false);
        let width = reactive.create_state(240.0_f32);
        let evaluations = Rc::new(Cell::new(0));
        let count = evaluations.clone();
        let expanded_value = expanded.value();
        let width_value = width.value();
        let fixed = inputs();
        let canvas = reactive.create_computed(move |cx| {
            count.set(count.get() + 1);
            let width = if *cx.read(&expanded_value) {
                *cx.read(&width_value)
            } else {
                120.0
            };
            panel::build(&panel::Inputs {
                width,
                ..fixed.clone()
            })
            .map(|outputs| outputs.drawing)
        });
        assert_eq!(
            background_width(canvas.snapshot().unwrap().as_ref().unwrap()),
            120.0
        );
        width.set(300.0).unwrap();
        assert_eq!(
            background_width(canvas.snapshot().unwrap().as_ref().unwrap()),
            120.0
        );
        assert_eq!(evaluations.get(), 1);
        expanded.set(true).unwrap();
        assert_eq!(
            background_width(canvas.snapshot().unwrap().as_ref().unwrap()),
            300.0
        );
        width.set(320.0).unwrap();
        assert_eq!(
            background_width(canvas.snapshot().unwrap().as_ref().unwrap()),
            320.0
        );
        assert_eq!(evaluations.get(), 3);
    }
}
