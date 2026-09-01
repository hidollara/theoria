use std::f32::consts::PI;

use theoria::{
    canvas::{Canvas, CanvasView, Composite, Coverage},
    color::Color,
    component::{Component, ComponentContext},
    font::Font,
    geometry::Point,
    gesture::{Click, ClickGesture, ClickRecognizer},
    input::pointer::PointerButton,
    interaction::{HitRegion, InteractionTree},
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
const CLICK_MOVEMENT: f32 = 8.0;

struct StarButton {
    interaction: InteractionTree<()>,
    count: State<u32>,
}

impl Component for StarButton {
    type Event = Click;

    fn receive(&mut self, click: Self::Event, context: &mut ComponentContext<'_>) {
        if click.button != PointerButton::Primary {
            return;
        }

        // This is the deliberately manual routing step. The finished Click is
        // accepted only when both ends of the gesture hit the same star.
        let pressed_inside = self
            .interaction
            .hit_test(click.press_position, HIT_TOLERANCE)
            .is_some();
        let released_inside = self
            .interaction
            .hit_test(click.release_position, HIT_TOLERANCE)
            .is_some();
        if pressed_inside && released_inside {
            context.update(&self.count, |count| count.saturating_add(1));
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let star = star_path(Point::new(480.0, 255.0), 155.0, 68.0);
    let interaction = InteractionTree::target((), HitRegion::fill(star.clone(), FillRule::NonZero));
    let window = Window::new(
        "Theoria — Star Button",
        WIDTH,
        HEIGHT,
        move |context, events| {
            let (click_source, clicks) = context.event::<Click>();
            let gesture = context.mount(move |_context| {
                ClickGesture::new(
                    ClickRecognizer::new(CLICK_MOVEMENT).expect("valid click movement"),
                    clicks,
                )
            });
            let count = context.state(0_u32);
            let count_value = count.value();
            let button = context.mount(move |_context| StarButton { interaction, count });
            assert!(context.subscribe(events.pointer(), &gesture));
            assert!(context.subscribe(&click_source, &button));

            let canvas_count = count_value.clone();
            let canvas = context.computed(move |context| {
                let count = *context.read(&canvas_count);
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
                    .push("Click the star")
                    .layout()
                    .expect("the hint text is valid")
                    .to_canvas(Color::rgb8(150, 167, 198))
                    .expect("the hint glyphs can be realized")
                    .transformed(Affine2::from_translation(Vector2::new(405.0, 535.0)));

                fill.composite(outline, Composite::SourceOver)
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
    use theoria::input::pointer::{
        PointerButtonChange, PointerButtons, PointerEvent, PointerId, PointerKind, PointerPhase,
    };

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
        let interaction =
            InteractionTree::target((), HitRegion::fill(star.clone(), FillRule::NonZero));
        let observed_count = Rc::new(Cell::new(0));
        let effect_count = Rc::clone(&observed_count);
        let runtime = ComponentRuntime::new();
        let input = runtime.mount(move |context| {
            let (pointer, events) = context.event::<PointerEvent>();
            let (click_source, clicks) = context.event::<Click>();
            let gesture = context.mount(move |_context| {
                ClickGesture::new(
                    ClickRecognizer::new(CLICK_MOVEMENT).expect("valid click movement"),
                    clicks,
                )
            });
            let count = context.state(0_u32);
            let count_value = count.value();
            let button = context.mount(move |_context| StarButton { interaction, count });
            assert!(context.subscribe(&pointer, &gesture));
            assert!(context.subscribe(&click_source, &button));
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
