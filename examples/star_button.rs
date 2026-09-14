use std::{cell::RefCell, f32::consts::PI, rc::Rc};

use theoria::{
    canvas::{Canvas, CanvasView, Composite, Coverage},
    color::Color,
    component::{Component, MountContext},
    event::ChannelSource,
    font::Font,
    geometry::Point,
    gesture::{
        Click, ClickArea, ClickGesture, ClickRecognizer, Drag, DragArea, DragGesture, DragPhase,
        DragRecognizer,
    },
    input::pointer::{MovementThreshold, PointerButton, PointerEvent, PointerId},
    interaction::HitRegion,
    math::Vector2,
    path::{FillRule, LineCap, LineJoin, Path, StrokeStyle},
    reactive::State,
    surface::SurfaceContent,
    text::{Text, TextStyle},
    transform::Affine2,
    windows::{self, Window},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 640;
const HIT_TOLERANCE: f32 = 0.25;
const INITIAL_CENTER: Point = Point::new(480.0, 255.0);

struct StarButton {
    count: State<u32>,
    center: State<Point>,
    pointers: ChannelSource<PointerEvent>,
    movement_threshold: MovementThreshold,
    hit_region: HitRegion,
}

impl Component for StarButton {
    fn mount(self, context: &mut MountContext<'_>) {
        let Self {
            count,
            center,
            pointers,
            movement_threshold,
            hit_region,
        } = self;
        let click = context.receive(move |click: Click, context| {
            if click.button == PointerButton::Primary {
                context.modify(&count, |count| *count = count.saturating_add(1));
            }
        });
        let center_value = center.value();
        let drag = context.receive(move |drag: Drag, context| {
            if drag.button == PointerButton::Primary && drag.phase != DragPhase::Cancelled {
                let delta = drag.position - drag.previous_position;
                context.modify(&center, |center| *center = *center + delta);
            }
        });

        let click_center = center_value.clone();
        let click_region = hit_region.clone();
        let click_areas = context.create_computed(move |context| {
            let center = *context.read(&click_center);
            vec![
                ClickArea::new(click.clone(), click_region.clone())
                    .transformed(Affine2::from_translation(Vector2::new(center.x, center.y))),
            ]
        });
        let committed_click_areas = Rc::new(RefCell::new(Vec::new()));
        let click_area_output = Rc::clone(&committed_click_areas);
        let effect = context.create_effect(move |context| {
            click_area_output.replace(context.read(&click_areas).iter().cloned().collect());
        });
        context.sync(&effect);
        let clicks = context.receive(move |click: Click, context| {
            let areas = committed_click_areas.borrow();
            if let Some(area) = areas
                .iter()
                .rev()
                .find(|area| area.contains(click.press_position, HIT_TOLERANCE))
            {
                let _ = area.try_forward(&click, HIT_TOLERANCE, context);
            }
        });

        let drag_center = center_value;
        let drag_areas = context.create_computed(move |context| {
            let center = *context.read(&drag_center);
            vec![
                DragArea::new(drag.clone(), hit_region.clone())
                    .transformed(Affine2::from_translation(Vector2::new(center.x, center.y))),
            ]
        });
        let committed_drag_areas = Rc::new(RefCell::new(Vec::new()));
        let drag_area_output = Rc::clone(&committed_drag_areas);
        let effect = context.create_effect(move |context| {
            drag_area_output.replace(context.read(&drag_areas).iter().cloned().collect());
        });
        context.sync(&effect);
        let mut captures = Vec::<(PointerId, PointerButton, DragArea)>::new();
        let drags = context.receive(move |drag: Drag, context| {
            let key = (drag.pointer_id, drag.button);
            match drag.phase {
                DragPhase::Started => {
                    let areas = committed_drag_areas.borrow();
                    if let Some(area) = areas
                        .iter()
                        .rev()
                        .find(|area| area.accepts_start(&drag, HIT_TOLERANCE))
                        .cloned()
                        && area.forward(&drag, context)
                    {
                        captures.push((key.0, key.1, area));
                    }
                }
                DragPhase::Moved => {
                    if let Some((_, _, area)) = captures
                        .iter()
                        .find(|(pointer, button, _)| (*pointer, *button) == key)
                    {
                        let _ = area.forward(&drag, context);
                    }
                }
                DragPhase::Ended | DragPhase::Cancelled => {
                    if let Some(index) = captures
                        .iter()
                        .position(|(pointer, button, _)| (*pointer, *button) == key)
                    {
                        let (_, _, area) = captures.swap_remove(index);
                        let _ = area.forward(&drag, context);
                    }
                }
            }
        });

        context.mount(ClickGesture::new(
            ClickRecognizer::new(movement_threshold)
                .expect("the Window provides a valid movement threshold"),
            pointers.clone(),
            clicks,
        ));
        context.mount(DragGesture::new(
            DragRecognizer::new(movement_threshold)
                .expect("the Window provides a valid movement threshold"),
            pointers,
            drags,
        ));
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let star = star_path(Point::new(0.0, 0.0), 155.0, 68.0);
    let hit_region = HitRegion::fill(star.clone(), FillRule::NonZero);
    let window = Window::new(
        &event_loop_handle,
        "Theoria — Star Button",
        WIDTH,
        HEIGHT,
        move |context, window| {
            let movement_threshold = window.movement_threshold();
            let count = context.create_state(0_u32);
            let count_value = count.value();
            let center = context.create_state(INITIAL_CENTER);
            let center_value = center.value();
            context.mount(StarButton {
                count,
                center,
                pointers: window.pointer().clone(),
                movement_threshold,
                hit_region,
            });

            let canvas_count = count_value.clone();
            let canvas_center = center_value;
            let canvas = context.create_computed(move |context| {
                let count = *context.read(&canvas_count);
                let center = *context.read(&canvas_center);
                let star_color = match count % 3 {
                    0 => Color::rgb8(255, 188, 54),
                    1 => Color::rgb8(83, 220, 255),
                    _ => Color::rgb8(255, 103, 181),
                };
                let fill = Canvas::draw(star_color, Coverage::fill(star.clone()));
                let outline = Canvas::draw(
                    Color::rgb8(255, 246, 214),
                    Coverage::stroke(
                        star.clone(),
                        StrokeStyle::new(8.0)
                            .expect("the fixed outline width is valid")
                            .with_cap(LineCap::Round)
                            .with_join(LineJoin::Round),
                    ),
                );

                let fonts = [font.clone()];
                let counter = Text::horizontal(TextStyle::new(&fonts, 44.0))
                    .push(format!("Clicks: {count}"))
                    .layout()
                    .expect("the counter text is valid")
                    .to_canvas(Color::rgb8(235, 242, 255))
                    .expect("the counter glyphs can be realized")
                    .transformed(Affine2::from_translation(Vector2::new(385.0, 470.0)));
                let hint = Text::horizontal(TextStyle::new(&fonts, 22.0))
                    .push("Click or drag the star")
                    .layout()
                    .expect("the hint text is valid")
                    .to_canvas(Color::rgb8(150, 167, 198))
                    .expect("the hint glyphs can be realized")
                    .transformed(Affine2::from_translation(Vector2::new(375.0, 535.0)));

                fill.composite(outline, Composite::SourceOver)
                    .transformed(Affine2::from_translation(Vector2::new(center.x, center.y)))
                    .composite(counter, Composite::SourceOver)
                    .composite(hint, Composite::SourceOver)
            });
            let size = window.surface().size();
            context.create_computed(move |context| {
                let canvas = context.read(&canvas).cloned();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive surface extent has a non-degenerate view");
                SurfaceContent::new(canvas, view, Color::rgb8(6, 9, 14))
            })
        },
    )?;
    window.show();
    event_loop.run()
}

fn star_path(center: Point, outer_radius: f32, inner_radius: f32) -> Path {
    Path::polygon((0..10).map(|index| {
        let radius = if index % 2 == 0 {
            outer_radius
        } else {
            inner_radius
        };
        let angle = -PI * 0.5 + index as f32 * PI / 5.0;
        Point::new(
            center.x + radius * angle.cos(),
            center.y + radius * angle.sin(),
        )
    }))
}

#[cfg(test)]
mod tests {
    use std::{cell::Cell, rc::Rc};

    use theoria::input::pointer::MovementThreshold;
    use theoria::input::pointer::{
        PointerButtonChange, PointerButtons, PointerEvent, PointerId, PointerKind, PointerPhase,
    };
    use theoria::{
        component::{ComponentInstance, MountContext},
        event::{DispatchQueue, EventTarget},
    };

    use super::*;

    fn pointer(change: PointerButtonChange, position: Point) -> PointerEvent {
        PointerEvent {
            id: PointerId::new(1),
            kind: PointerKind::Mouse,
            position,
            is_primary: true,
            buttons: match change {
                PointerButtonChange::Pressed(button) => PointerButtons::empty().with(button),
                PointerButtonChange::Released(_) => PointerButtons::empty(),
            },
            phase: PointerPhase::Move,
            button_change: Some(change),
        }
    }

    struct TestDriver {
        queue: DispatchQueue,
        components: ComponentInstance,
    }

    impl TestDriver {
        fn new(build: impl FnOnce(&mut MountContext<'_>)) -> Self {
            let queue = DispatchQueue::new();
            let components = ComponentInstance::new(queue.dispatcher(), build);
            Self { queue, components }
        }

        fn run_until_idle(&mut self) {
            while self.queue.run_next().is_some() {}
            self.components.flush_effects().expect("valid Effects");
        }
    }

    fn click(runtime: &mut TestDriver, input: &EventTarget<PointerEvent>, point: Point) {
        assert!(
            runtime
                .components
                .send(
                    input,
                    pointer(PointerButtonChange::Pressed(PointerButton::Primary), point,),
                )
                .is_ok()
        );
        assert!(
            runtime
                .components
                .send(
                    input,
                    pointer(PointerButtonChange::Released(PointerButton::Primary), point,),
                )
                .is_ok()
        );
        runtime.run_until_idle();
    }

    #[test]
    fn only_clicks_inside_the_star_increment_the_counter() {
        let star = star_path(Point::new(0.0, 0.0), 155.0, 68.0);
        let hit_region = HitRegion::fill(star.clone(), FillRule::NonZero);
        let observed_count = Rc::new(Cell::new(0));
        let effect_count = Rc::clone(&observed_count);
        let input = Rc::new(RefCell::new(None));
        let mounted_input = Rc::clone(&input);
        let mut runtime = TestDriver::new(move |context| {
            let (pointer, events) = context.event_source::<PointerEvent>();
            let count = context.create_state(0_u32);
            let count_value = count.value();
            let center = context.create_state(INITIAL_CENTER);
            context.mount(StarButton {
                count,
                center,
                pointers: pointer,
                movement_threshold: MovementThreshold::Radial { radius: 8.0 },
                hit_region,
            });
            let effect = context.create_effect(move |context| {
                effect_count.set(*context.read(&count_value));
            });
            context.sync(&effect);
            mounted_input.replace(Some(context.receive(
                move |event: PointerEvent, context| {
                    let _ = context.send(&events, event);
                },
            )));
        });
        let input = input.borrow_mut().take().expect("test pointer input");
        runtime.run_until_idle();

        // Inside the star's outer circle, but just beyond its upper-right
        // concave corner: a bounding box/circle test would accept this.
        click(&mut runtime, &input, Point::new(534.0, 181.0));
        assert_eq!(observed_count.get(), 0);

        click(&mut runtime, &input, Point::new(480.0, 255.0));
        assert_eq!(observed_count.get(), 1);
    }
}
