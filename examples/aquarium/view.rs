use std::time::{Duration, Instant};

use theoria::color::{Opacity, Srgb};
use theoria::{
    canvas::{Canvas, CanvasColor, Composite, Composition, Coverage},
    color::{Color, LinearSrgb},
    font::Font,
    geometry::Point,
    math::Vector2,
    path::Path,
    text::{Text, TextStyle},
    transform::Affine2,
};
use windows::Win32::UI::WindowsAndMessaging::{
    WM_APP, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_MOUSEMOVE, WM_MOVE, WM_NCHITTEST, WM_PAINT,
    WM_POINTERDOWN, WM_POINTERUP, WM_POINTERUPDATE, WM_SETCURSOR, WM_SIZE, WM_WINDOWPOSCHANGED,
};

use super::{MessageHistory, MessageStage};

const HEADER_HEIGHT: f32 = 112.0;
const FOOTER_HEIGHT: f32 = 52.0;
const PARTICLE_LIFETIME: Duration = Duration::from_secs(7);

pub fn render(
    history: &MessageHistory,
    font: &Font,
    width: f32,
    height: f32,
    elapsed: Duration,
    demo_thread_message: u32,
) -> Canvas {
    let margin = 24.0;
    let gap = 16.0;
    let content_width = (width - margin * 2.0).max(1.0);
    let lane_width = ((content_width - gap * 2.0) / 3.0).max(1.0);
    let lane_top = HEADER_HEIGHT;
    let lane_bottom = (height - FOOTER_HEIGHT).max(lane_top + 1.0);
    let lane_height = lane_bottom - lane_top;
    let now = Instant::now();

    let mut canvas = Canvas::draw(
        lane_background(MessageStage::Queued),
        Coverage::fill(rectangle(margin, lane_top, lane_width, lane_height)),
    );
    for stage in [
        MessageStage::Queued,
        MessageStage::Thread,
        MessageStage::Window,
    ] {
        let index = stage.index();
        let x = margin + index as f32 * (lane_width + gap);
        if stage != MessageStage::Queued {
            canvas = canvas.composite(
                Canvas::draw(
                    lane_background(stage),
                    Coverage::fill(rectangle(x, lane_top, lane_width, lane_height)),
                ),
                Composition::new(Composite::SourceOver, LinearSrgb),
            );
        }
        canvas = canvas.composite(
            label(
                font,
                &format!("{}  {:>7}", stage_name(stage), history.count(stage)),
                21.0,
                stage_color(stage),
                x + 14.0,
                118.0,
            ),
            Composition::new(Composite::SourceOver, LinearSrgb),
        );
    }

    for entry in history.entries() {
        let age = now.saturating_duration_since(entry.observation.observed_at);
        if age >= PARTICLE_LIFETIME {
            continue;
        }
        let stage = entry.observation.stage;
        let lane = stage.index() as f32;
        let progress = age.as_secs_f32() / PARTICLE_LIFETIME.as_secs_f32();
        let drift = (elapsed.as_secs_f32() * 1.7 + entry.sequence as f32 * 0.71).sin();
        let x = margin + lane * (lane_width + gap) + lane_width * (0.5 + drift * 0.34);
        let y = lane_bottom - progress * (lane_height - 32.0) - 16.0;
        let alpha = ((1.0 - progress) * 220.0 + 24.0) as u8;
        let base = stage_rgb(stage);
        let radius = 4.0 + (entry.observation.id % 7) as f32;
        canvas = canvas.composite(
            Canvas::draw(
                Color::new(Srgb, [base.0, base.1, base.2], Opacity::<u8>::from(alpha)),
                Coverage::fill(rectangle(
                    x - radius,
                    y - radius,
                    radius * 2.0,
                    radius * 2.0,
                )),
            ),
            Composition::new(Composite::Plus, LinearSrgb),
        );
    }

    canvas = canvas
        .composite(
            label(
                font,
                "WIN32 MESSAGE AQUARIUM",
                30.0,
                Color::new(Srgb, [231_u8, 240, 255], Opacity::<u8>::ONE),
                margin,
                8.0,
            ),
            Composition::new(Composite::SourceOver, LinearSrgb),
        )
        .composite(
            label(
                font,
                "Move, resize, scroll, or click. A click posts a thread message.",
                18.0,
                Color::new(Srgb, [138_u8, 157, 188], Opacity::<u8>::ONE),
                margin,
                height - 42.0,
            ),
            Composition::new(Composite::SourceOver, LinearSrgb),
        );

    if let Some(latest) = history.entries().next_back() {
        let observation = latest.observation;
        let details = format!(
            "LATEST  {}  {}  w={:#x}  l={:#x}{}{}{}",
            stage_name(observation.stage),
            message_name(observation.id, demo_thread_message),
            observation.wparam,
            observation.lparam,
            observation
                .posted_at
                .map(|time| format!("  t={time}"))
                .unwrap_or_default(),
            observation
                .cursor
                .map(|(x, y)| format!("  cursor=({x},{y})"))
                .unwrap_or_default(),
            observation
                .destination
                .map(|window| format!("  hwnd={window:#x}"))
                .unwrap_or_default(),
        );
        canvas = canvas.composite(
            label(
                font,
                &details,
                15.0,
                Color::new(Srgb, [180_u8, 197, 224], Opacity::<u8>::ONE),
                margin,
                55.0,
            ),
            Composition::new(Composite::SourceOver, LinearSrgb),
        );
    }

    canvas
}

fn rectangle(x: f32, y: f32, width: f32, height: f32) -> Path {
    Path::polygon([
        Point::new(x, y),
        Point::new(x + width, y),
        Point::new(x + width, y + height),
        Point::new(x, y + height),
    ])
}

fn label(
    font: &Font,
    content: &str,
    size: f32,
    color: impl Into<CanvasColor>,
    x: f32,
    y: f32,
) -> Canvas {
    Text::horizontal(TextStyle::new(std::slice::from_ref(font), size))
        .push(content)
        .layout()
        .expect("the aquarium label is valid")
        .to_canvas(color)
        .expect("the aquarium label glyphs can be realized")
        .expect("the aquarium label contains drawable glyphs")
        .transformed(Affine2::from_translation(Vector2::new(x, y)))
}

fn stage_name(stage: MessageStage) -> &'static str {
    match stage {
        MessageStage::Queued => "QUEUED",
        MessageStage::Thread => "THREAD",
        MessageStage::Window => "WINDOW",
    }
}

fn stage_rgb(stage: MessageStage) -> (u8, u8, u8) {
    match stage {
        MessageStage::Queued => (55, 206, 255),
        MessageStage::Thread => (255, 105, 213),
        MessageStage::Window => (255, 205, 72),
    }
}

fn stage_color(stage: MessageStage) -> CanvasColor {
    let (red, green, blue) = stage_rgb(stage);
    Color::new(Srgb, [red, green, blue], Opacity::<u8>::ONE).into()
}

fn lane_background(stage: MessageStage) -> CanvasColor {
    match stage {
        MessageStage::Queued => Color::new(Srgb, [23_u8, 92, 120], Opacity::<u8>::from(64)).into(),
        MessageStage::Thread => Color::new(Srgb, [111_u8, 31, 98], Opacity::<u8>::from(64)).into(),
        MessageStage::Window => Color::new(Srgb, [112_u8, 82, 18], Opacity::<u8>::from(64)).into(),
    }
}

fn message_name(id: u32, demo_thread_message: u32) -> String {
    if id == demo_thread_message {
        return "THEORIA_DEMO_THREAD".to_owned();
    }
    let name = match id {
        WM_CREATE => "WM_CREATE",
        WM_DESTROY => "WM_DESTROY",
        WM_MOVE => "WM_MOVE",
        WM_SIZE => "WM_SIZE",
        WM_PAINT => "WM_PAINT",
        WM_CLOSE => "WM_CLOSE",
        WM_SETCURSOR => "WM_SETCURSOR",
        WM_NCHITTEST => "WM_NCHITTEST",
        WM_MOUSEMOVE => "WM_MOUSEMOVE",
        WM_WINDOWPOSCHANGED => "WM_WINDOWPOSCHANGED",
        WM_POINTERUPDATE => "WM_POINTERUPDATE",
        WM_POINTERDOWN => "WM_POINTERDOWN",
        WM_POINTERUP => "WM_POINTERUP",
        _ if id >= WM_APP => return format!("WM_APP+{:#x}", id - WM_APP),
        _ => return format!("MSG {id:#06x}"),
    };
    name.to_owned()
}
