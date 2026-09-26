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
    UI::WindowsAndMessaging::{PostQuitMessage, PostThreadMessageW, WM_APP},
};
use theoria::color::{Opacity, Srgb};
use theoria::{
    canvas::{Canvas, CanvasImage, CanvasView, Coverage},
    event::EventSink,
    font::Font,
    geometry::Point,
    input::pointer::{PointerButton, PointerButtonChange, PointerEvent},
    recording::AnimatedPngRecorder,
    surface::{SurfaceContent, SurfaceSize},
    vulkan::ImageSurface,
    windows::{self, Window},
};

use aquarium::{MessageHistory, MessageObservation, render};

const WIDTH: u32 = 1_120;
const HEIGHT: u32 = 720;
const DEMO_THREAD_MESSAGE: u32 = WM_APP + 0x51;
const RECORDING_FRAMES: u32 = 120;
const RECORDING_FRAME_DURATION: Duration = Duration::from_nanos(16_666_667);

enum CaptureRequest {
    Snapshot(PathBuf),
    Recording(PathBuf),
}

enum CaptureOutput {
    Snapshot {
        surface: ImageSurface,
        path: PathBuf,
    },
    Recording {
        surface: ImageSurface,
        recorder: AnimatedPngRecorder<BufWriter<File>>,
        path: PathBuf,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let capture_request = capture_request(std::env::args_os().skip(1))?;
    let mut event_loop = windows::event_loop();
    let event_loop_handle = event_loop.handle();
    let queued_messages = event_loop.queued_messages().clone();
    let thread_messages = event_loop.thread_messages().clone();
    let thread_id = unsafe { GetCurrentThreadId() };
    if capture_request.is_some() {
        post_capture_activity(thread_id);
    }
    let font = Font::from_bytes(std::fs::read(r"C:\Windows\Fonts\segoeui.ttf")?)?;
    let capture_size = SurfaceSize {
        width: WIDTH,
        height: HEIGHT,
    };
    let capture = match capture_request {
        Some(CaptureRequest::Snapshot(path)) => Some(CaptureOutput::Snapshot {
            surface: ImageSurface::new(capture_size)?,
            path,
        }),
        Some(CaptureRequest::Recording(path)) => {
            let output = BufWriter::new(File::create(&path)?);
            Some(CaptureOutput::Recording {
                surface: ImageSurface::new(capture_size)?,
                recorder: AnimatedPngRecorder::new(output, capture_size, RECORDING_FRAMES)?,
                path,
            })
        }
        None => None,
    };
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
            let render_font = font.clone();
            let capture_history = history_value.clone();
            let capture_size = size.clone();
            let capture_font = font.clone();
            let content = context.create_computed(move |context| {
                let history = context.read(&history_value);
                let elapsed = context.read(&render_frame).elapsed();
                let size = *context.read(&size);
                aquarium_content(&history, &render_font, size, elapsed, false)
            });

            if let Some(capture) = capture {
                let captured_frame = frame;
                let mut capture = Some(capture);
                let mut last_recorded = None;
                let capture = context.create_effect(move |context| {
                    let Some(output) = capture.as_mut() else {
                        return;
                    };
                    let frame = *context.read(&captured_frame);
                    match output {
                        CaptureOutput::Snapshot { surface, path } => {
                            if frame.elapsed() < Duration::from_secs(2) {
                                return;
                            }
                            let history = context.read(&capture_history);
                            let size = *context.read(&capture_size);
                            let content = aquarium_content(
                                &history,
                                &capture_font,
                                size,
                                frame.elapsed(),
                                true,
                            );
                            let image = surface
                                .render(&content)
                                .expect("the Aquarium frame can be rendered offscreen");
                            write_png(path, &image).expect("the Aquarium snapshot can be written");
                            eprintln!("wrote {}", path.display());
                            capture = None;
                            unsafe { PostQuitMessage(0) };
                        }
                        CaptureOutput::Recording {
                            surface, recorder, ..
                        } => {
                            if last_recorded == Some(frame.elapsed()) {
                                return;
                            }
                            let timestamp = RECORDING_FRAME_DURATION * recorder.recorded_frames();
                            let history = context.read(&capture_history);
                            let size = *context.read(&capture_size);
                            let content =
                                aquarium_content(&history, &capture_font, size, timestamp, true);
                            let image = surface
                                .render(&content)
                                .expect("the Aquarium frame can be rendered offscreen");
                            recorder
                                .record(image, RECORDING_FRAME_DURATION)
                                .expect("the Aquarium frame can be recorded");
                            last_recorded = Some(frame.elapsed());
                            if recorder.is_complete() {
                                let CaptureOutput::Recording { recorder, path, .. } =
                                    capture.take().unwrap()
                                else {
                                    unreachable!()
                                };
                                recorder
                                    .finish()
                                    .expect("the Aquarium recording can be completed");
                                eprintln!("wrote {}", path.display());
                                unsafe { PostQuitMessage(0) };
                            }
                        }
                    }
                });
                context.sync(&capture);
            }

            content
        },
    )?;
    window.show();
    event_loop.run()
}

fn aquarium_content(
    history: &MessageHistory,
    font: &Font,
    size: SurfaceSize,
    elapsed: Duration,
    rasterize: bool,
) -> SurfaceContent {
    let width = size.width.max(1) as f32;
    let height = size.height.max(1) as f32;
    let canvas = render(history, font, width, height, elapsed, DEMO_THREAD_MESSAGE);
    let view = CanvasView::axis_aligned(Point::new(0.0, 0.0), Point::new(width, height))
        .expect("a positive Window extent has a non-degenerate view");
    let canvas = if rasterize {
        Canvas::draw(
            CanvasImage::new(canvas, view, size.width.max(1), size.height.max(1))
                .expect("a positive Window extent is a valid CanvasImage size"),
            Coverage::all(),
        )
    } else {
        canvas
    };
    SurfaceContent::new(
        canvas,
        view,
        theoria::color::Color::new(Srgb, [5_u8, 8, 15], Opacity::<u8>::ONE),
    )
}

fn capture_request(
    mut arguments: impl Iterator<Item = OsString>,
) -> Result<Option<CaptureRequest>, Box<dyn std::error::Error>> {
    let Some(argument) = arguments.next() else {
        return Ok(None);
    };
    let recording = argument == "--record";
    if argument != "--snapshot" && !recording {
        return Err(format!("unknown argument: {}", argument.to_string_lossy()).into());
    }
    let path = arguments
        .next()
        .ok_or_else(|| format!("{} requires an output PNG path", argument.to_string_lossy()))?;
    if let Some(argument) = arguments.next() {
        return Err(format!("unexpected argument: {}", argument.to_string_lossy()).into());
    }
    Ok(Some(if recording {
        CaptureRequest::Recording(path.into())
    } else {
        CaptureRequest::Snapshot(path.into())
    }))
}

fn post_capture_activity(thread_id: u32) {
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

fn write_png(
    path: &Path,
    image: &theoria::image::Srgba8Image,
) -> Result<(), Box<dyn std::error::Error>> {
    let output = BufWriter::new(File::create(path)?);
    let mut encoder = png::Encoder::new(output, image.width(), image.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::Perceptual);
    encoder.write_header()?.write_image_data(image.pixels())?;
    Ok(())
}
