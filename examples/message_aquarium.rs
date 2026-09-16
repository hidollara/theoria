mod aquarium;

use std::{
    ffi::OsString,
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

use ::windows::Win32::{
    Foundation::{LPARAM, WPARAM},
    System::Threading::GetCurrentThreadId,
    UI::WindowsAndMessaging::{PostThreadMessageW, WM_APP},
};
use theoria::{
    canvas::{CanvasView, Layer},
    event::EventSink,
    font::Font,
    geometry::Point,
    input::pointer::{PointerButton, PointerButtonChange, PointerEvent},
    surface::{SurfaceContent, SurfaceSize},
    vulkan::ImageSurface,
    windows::{self, Window},
};

use aquarium::{MessageHistory, MessageObservation, render};

const WIDTH: u32 = 1_120;
const HEIGHT: u32 = 720;
const DEMO_THREAD_MESSAGE: u32 = WM_APP + 0x51;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot_path = snapshot_path(std::env::args_os().skip(1))?;
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let queued_messages = event_loop.queued_messages().clone();
    let thread_messages = event_loop.thread_messages().clone();
    let thread_id = unsafe { GetCurrentThreadId() };
    if snapshot_path.is_some() {
        post_snapshot_activity(thread_id);
    }
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let image_surface = snapshot_path
        .as_ref()
        .map(|_| {
            ImageSurface::new(SurfaceSize {
                width: WIDTH,
                height: HEIGHT,
            })
        })
        .transpose()?;
    let snapshotting = image_surface.is_some();

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
            let render_frame = frame.clone();
            let content = context.create_computed(move |context| {
                let history = context.read(&history_value);
                let elapsed = context.read(&render_frame).elapsed();
                let size = *context.read(&size);
                let width = size.width.max(1) as f32;
                let height = size.height.max(1) as f32;
                let canvas = render(&history, &font, width, height, elapsed, DEMO_THREAD_MESSAGE);
                let view =
                    CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
                        .expect("a positive Window extent has a non-degenerate view");
                let canvas = if snapshotting {
                    Layer::new(canvas, view, size.width.max(1), size.height.max(1))
                        .expect("a positive Window extent is a valid Layer size")
                        .into()
                } else {
                    canvas
                };
                SurfaceContent::new(canvas, view, theoria::color::Color::rgb8(5, 8, 15))
            });

            if let Some((mut image_surface, path)) = image_surface.zip(snapshot_path) {
                let captured_content = content.clone();
                let captured_frame = frame;
                let mut captured = false;
                let capture = context.create_effect(move |context| {
                    if captured {
                        return;
                    }
                    if context.read(&captured_frame).elapsed() < Duration::from_secs(2) {
                        return;
                    }
                    let image = image_surface
                        .render(&context.read(&captured_content))
                        .expect("the Aquarium frame can be rendered offscreen");
                    write_png(&path, &image).expect("the Aquarium snapshot can be written");
                    eprintln!("wrote {}", path.display());
                    captured = true;
                });
                context.sync(&capture);
            }

            content
        },
    )?;
    window.show();
    event_loop.run()
}

fn snapshot_path(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    let Some(argument) = arguments.next() else {
        return Ok(None);
    };
    if argument != "--snapshot" {
        return Err(format!("unknown argument: {}", argument.to_string_lossy()).into());
    }
    let path = arguments
        .next()
        .ok_or("--snapshot requires an output PNG path")?;
    if let Some(argument) = arguments.next() {
        return Err(format!("unexpected argument: {}", argument.to_string_lossy()).into());
    }
    Ok(Some(path.into()))
}

fn post_snapshot_activity(thread_id: u32) {
    thread::spawn(move || {
        for index in 0..12 {
            thread::sleep(Duration::from_millis(110));
            unsafe {
                let _ = PostThreadMessageW(
                    thread_id,
                    DEMO_THREAD_MESSAGE,
                    WPARAM(80 + index * 41),
                    LPARAM(120 + index as isize * 23),
                );
            }
        }
    });
}

fn write_png(path: &Path, image: &theoria::image::Image) -> Result<(), Box<dyn std::error::Error>> {
    let output = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(output, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.write_header()?.write_image_data(image.rgba8())?;
    Ok(())
}
