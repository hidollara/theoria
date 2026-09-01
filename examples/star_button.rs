use std::f32::consts::PI;

use theoria::{
    canvas::{Canvas, CanvasView, Composite, Coverage},
    color::Color,
    component::{Component, ComponentContext},
    font::Font,
    geometry::Point,
    gesture::{
        Click, ClickArea, ClickGesture, ClickRecognizer, Drag, DragArea, DragGesture, DragPhase,
        DragRecognizer,
    },
    input::pointer::{PointerButton, PointerId},
    interaction::HitRegion,
    math::Vector2,
    path::{FillRule, LineCap, LineJoin, Path, StrokeStyle},
    reactive::State,
    text::{Text, TextStyle},
    transform::Affine2,
    window::{Window, WindowContent},
};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 640;
const HIT_TOLERANCE: f32 = 0.25;
const INITIAL_CENTER: Point = Point::new(480.0, 255.0);

struct StarButton {
    count: State<u32>,
    center: State<Point>,
}

impl Component for StarButton {
    type Event = StarButtonEvent;

    fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
        match event {
            StarButtonEvent::Click(click) if click.button == PointerButton::Primary => {
                context.update(&self.count, |count| count.saturating_add(1));
            }
            StarButtonEvent::Drag(drag)
                if drag.button == PointerButton::Primary && drag.phase != DragPhase::Cancelled =>
            {
                let delta = drag.position - drag.previous_position;
                context.update(&self.center, |center| *center + delta);
            }
            _ => {}
        }
    }
}

enum StarButtonEvent {
    Click(Click),
    Drag(Drag),
}

impl From<Click> for StarButtonEvent {
    fn from(click: Click) -> Self {
        Self::Click(click)
    }
}

impl From<Drag> for StarButtonEvent {
    fn from(drag: Drag) -> Self {
        Self::Drag(drag)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let star = star_path(Point::new(0.0, 0.0), 155.0, 68.0);
    let hit_region = HitRegion::fill(star.clone(), FillRule::NonZero);
    let window = Window::new(
        "Theoria — Star Button",
        WIDTH,
        HEIGHT,
        move |context, events| {
            let movement_threshold = events.movement_threshold();
            let (click_source, clicks) = context.event::<Click>();
            let click_gesture = context.mount(move |_context| {
                ClickGesture::new(
                    ClickRecognizer::new(movement_threshold)
                        .expect("the Window provides a valid movement threshold"),
                    clicks,
                )
            });
            let (drag_source, drags) = context.event::<Drag>();
            let drag_gesture = context.mount(move |_context| {
                DragGesture::new(
                    DragRecognizer::new(movement_threshold)
                        .expect("the Window provides a valid movement threshold"),
                    drags,
                )
            });
            let count = context.state(0_u32);
            let count_value = count.value();
            let center = context.state(INITIAL_CENTER);
            let center_value = center.value();
            let button = context.mount(move |_context| StarButton { count, center });

            let click_center = center_value.clone();
            let click_target = button.event_target::<Click>();
            let click_region = hit_region.clone();
            let click_areas = context.computed(move |context| {
                let center = *context.read(&click_center);
                vec![
                    ClickArea::new(click_target.clone(), click_region.clone())
                        .transformed(Affine2::from_translation(Vector2::new(center.x, center.y))),
                ]
            });
            let click_handler = context.mount_handler(move |_context| {
                move |click: Click, context: &mut ComponentContext<'_>| {
                    let areas = context.read_current(&click_areas);
                    if let Some(area) = areas
                        .iter()
                        .rev()
                        .find(|area| area.contains(click.press_position, HIT_TOLERANCE))
                    {
                        let _ = area.try_forward(&click, HIT_TOLERANCE, context);
                    }
                }
            });

            let drag_center = center_value.clone();
            let drag_target = button.event_target::<Drag>();
            let drag_areas = context.computed(move |context| {
                let center = *context.read(&drag_center);
                vec![
                    DragArea::new(drag_target.clone(), hit_region.clone())
                        .transformed(Affine2::from_translation(Vector2::new(center.x, center.y))),
                ]
            });
            let drag_handler = context.mount_handler(move |_context| {
                let mut captures: Vec<(PointerId, PointerButton, DragArea)> = Vec::new();
                move |drag: Drag, context: &mut ComponentContext<'_>| {
                    let key = (drag.pointer_id, drag.button);
                    match drag.phase {
                        DragPhase::Started => {
                            let areas = context.read_current(&drag_areas);
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
                }
            });

            assert!(context.connect(events.pointer(), &click_gesture));
            assert!(context.connect(events.pointer(), &drag_gesture));
            assert!(context.connect(&click_source, &click_handler));
            assert!(context.connect(&drag_source, &drag_handler));

            let canvas_count = count_value.clone();
            let canvas_center = center_value;
            let canvas = context.computed(move |context| {
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
            let size = events.size();
            context.computed(move |context| {
                let canvas = context.read(&canvas).cloned();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive surface extent has a non-degenerate view");
                WindowContent::new(canvas, view, Color::rgb8(6, 9, 14))
            })
        },
    )?;
    window.run()
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

    use theoria::component::{
        ComponentHandle, ComponentRuntime, EventEmitter, TurnBudget, TurnOutcome,
    };
    use theoria::gesture::MovementThreshold;
    use theoria::input::pointer::{
        PointerButtonChange, PointerButtons, PointerEvent, PointerId, PointerKind, PointerPhase,
    };
    use theoria::reactive::Value;

    use super::*;

    struct PointerInput {
        events: EventEmitter<PointerEvent>,
    }

    impl Component for PointerInput {
        type Event = PointerEvent;

        fn receive(&mut self, event: Self::Event, context: &mut ComponentContext<'_>) {
            let _ = context.emit(&self.events, event);
        }
    }

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

    fn click(runtime: &ComponentRuntime, input: &ComponentHandle<PointerInput>, point: Point) {
        assert!(runtime.enqueue(
            input,
            pointer(PointerButtonChange::Pressed(PointerButton::Primary), point,),
        ));
        assert!(runtime.enqueue(
            input,
            pointer(PointerButtonChange::Released(PointerButton::Primary), point,),
        ));
        assert_eq!(runtime.run_turn(TurnBudget::unlimited()), TurnOutcome::Idle);
    }

    #[test]
    fn only_clicks_inside_the_star_increment_the_counter() {
        let star = star_path(Point::new(480.0, 255.0), 155.0, 68.0);
        let hit_region = HitRegion::fill(star.clone(), FillRule::NonZero);
        let observed_count = Rc::new(Cell::new(0));
        let effect_count = Rc::clone(&observed_count);
        let runtime = ComponentRuntime::new();
        let input = runtime.mount(move |context| {
            let (pointer, events) = context.event::<PointerEvent>();
            let (click_source, clicks) = context.event::<Click>();
            let gesture = context.mount(move |_context| {
                ClickGesture::new(
                    ClickRecognizer::new(MovementThreshold::Radial { radius: 8.0 })
                        .expect("valid click movement"),
                    clicks,
                )
            });
            let count = context.state(0_u32);
            let count_value = count.value();
            let center = context.state(INITIAL_CENTER);
            let button = context.mount(move |_context| StarButton { count, center });
            let click_handler = context.mount_handler(move |_context| {
                let areas = Value::from(vec![ClickArea::new(button.event_target(), hit_region)]);
                move |click: Click, context: &mut ComponentContext<'_>| {
                    let areas = context.read_current(&areas);
                    if let Some(area) = areas
                        .iter()
                        .rev()
                        .find(|area| area.contains(click.press_position, HIT_TOLERANCE))
                    {
                        let _ = area.try_forward(&click, HIT_TOLERANCE, context);
                    }
                }
            });
            assert!(context.connect(&pointer, &gesture));
            assert!(context.connect(&click_source, &click_handler));
            context.effect(move |context| {
                effect_count.set(*context.read(&count_value));
            });
            PointerInput { events }
        });
        runtime.flush_effects();

        // Inside the star's outer circle, but just beyond its upper-right
        // concave corner: a bounding box/circle test would accept this.
        click(&runtime, &input, Point::new(534.0, 181.0));
        assert_eq!(observed_count.get(), 0);

        click(&runtime, &input, Point::new(480.0, 255.0));
        assert_eq!(observed_count.get(), 1);
    }
}
