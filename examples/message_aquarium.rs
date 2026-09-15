mod aquarium;

use std::time::Instant;

use ::windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};
use theoria::{
    canvas::CanvasView,
    event::EventSink,
    font::Font,
    geometry::Point,
    input::pointer::{PointerButton, PointerButtonChange, PointerEvent},
    surface::SurfaceContent,
    windows::{self, Window},
};

use aquarium::{MessageHistory, MessageObservation, render};

const WIDTH: u32 = 1_120;
const HEIGHT: u32 = 720;
const DEMO_THREAD_MESSAGE: u32 = WM_APP + 0x51;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let queued_messages = event_loop.queued_messages().clone();
    let thread_messages = event_loop.thread_messages().clone();
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;

    let window = Window::new(
        &event_loop_handle,
        "Theoria — Win32 Message Aquarium",
        WIDTH,
        HEIGHT,
        move |context, window| {
            let history = context.create_state(MessageHistory::new(160));
            let history_value = history.value();
            let record = context.receive(move |observation: MessageObservation, context| {
                context.modify(&history, |history| history.record(observation));
            });

            let queued = record
                .clone()
                .contramap(|message: windows::Win32QueuedMessage<'_>| {
                    MessageObservation::queued(&message, Instant::now())
                });
            assert!(context.connect_inline(queued_messages, queued));

            let thread = record
                .clone()
                .contramap(|message: windows::Win32ThreadMessage<'_>| {
                    MessageObservation::thread(&message, Instant::now())
                });
            assert!(context.connect_inline(thread_messages, thread));

            let native = record.contramap(|message: windows::Win32WindowMessage<'_>| {
                MessageObservation::window(&message, Instant::now())
            });
            assert!(context.connect_inline(window.native_messages().clone(), native));

            let thread_id = unsafe { GetCurrentThreadId() };
            let post_demo_message = context.receive(move |event: PointerEvent, _| {
                if event.button_change == Some(PointerButtonChange::Pressed(PointerButton::Primary))
                {
                    let x = event.position.x.round() as isize;
                    let y = event.position.y.round() as isize;
                    unsafe {
                        let _ = PostThreadMessageW(
                            thread_id,
                            DEMO_THREAD_MESSAGE,
                            WPARAM(x as usize),
                            LPARAM(y),
                        );
                    }
                }
            });
            assert!(context.connect(
                window.pointer().clone(),
                theoria::event::OwningSink::new(post_demo_message),
            ));

            let frame_clock = window.frame_clock();
            let frame = frame_clock.frame();
            let next_frame = frame_clock.clone();
            let continuous_frame = frame.clone();
            let animate = context.create_effect(move |context| {
                let _ = context.read(&continuous_frame).elapsed();
                let _ = next_frame.request_frame();
            });
            context.sync(&animate);

            let size = window.surface().size();
            context.create_computed(move |context| {
                let history = context.read(&history_value);
                let elapsed = context.read(&frame).elapsed();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let canvas = render(&history, &font, width, height, elapsed, DEMO_THREAD_MESSAGE);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive Window extent has a non-degenerate view");
                SurfaceContent::new(canvas, view, theoria::color::Color::rgb8(5, 8, 15))
            })
        },
    )?;
    window.show();
    event_loop.run()
}
