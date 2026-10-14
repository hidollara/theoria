use std::{error::Error, fs::File, io::BufWriter, path::Path};

use theoria_renderer::SurfaceSize;
use theoria_text::font::Font;
use theoria_vulkan::ImageSurface;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let mut output = None;
    let mut fonts = Vec::new();
    let mut source = None;
    let mut samples = std::collections::BTreeMap::new();
    let mut output_name = String::from("drawing");
    while let Some(option) = args.next() {
        if option == "--font" {
            let path = args.next().ok_or("--font requires a font path")?;
            fonts.push(Font::from_bytes(std::fs::read(path)?)?);
        } else if option == "--png" && output.is_none() {
            output = Some(args.next().ok_or("--png requires an output path")?);
        } else if option == "--source" && source.is_none() {
            source = Some(args.next().ok_or("--source requires an Element XML path")?);
        } else if option == "--input" {
            let value = args
                .next()
                .ok_or("--input requires name=value")?
                .into_string()
                .map_err(|_| "sample must be UTF-8")?;
            let (name, value) = value.split_once('=').ok_or("--input requires name=value")?;
            if samples.insert(name.to_owned(), value.to_owned()).is_some() {
                return Err(format!("duplicate sample {name:?}").into());
            }
        } else if option == "--output" {
            output_name = args
                .next()
                .ok_or("--output requires an output name")?
                .into_string()
                .map_err(|_| "output name must be UTF-8")?;
        } else {
            return Err("usage: organon [--source element.organon.xml --input name=value ... --output drawing] [--font font.ttf]... [--png output.png]".into());
        }
    }
    #[cfg(windows)]
    if fonts.is_empty() {
        fonts.push(Font::from_bytes(std::fs::read(
            r"C:\Windows\Fonts\segoeui.ttf",
        )?)?);
    }
    if fonts.is_empty() && source.is_none() {
        return Err("supply at least one --font font.ttf for the text preview".into());
    }
    let scene = if let Some(source) = source {
        let preview =
            organon_app::preview::LibraryPreview::load(source, organon_app::preview::registry())?;
        let mut inputs = std::collections::BTreeMap::new();
        let element = preview
            .definition
            .program
            .elements
            .iter()
            .find(|e| e.name == preview.element)
            .unwrap();
        for input in &element.inputs {
            let ty = preview
                .registry
                .types
                .get(input.kind.spelling())
                .ok_or_else(|| format!("no preview type for {}", input.kind.spelling()))?;
            let value = if (ty.rust_type)() == std::any::TypeId::of::<Vec<Font>>() {
                organon_app::evaluation::Value::new(fonts.clone())
            } else {
                let text = samples
                    .remove(&input.name)
                    .ok_or_else(|| format!("missing --input {}=value", input.name))?;
                let scope =
                    organon_theoria::attribute::Scope::new(preview.registry.attributes.clone());
                let attribute = (ty.interpret)(&text, &scope, false)?;
                let reactive = theoria_reactive::Reactive::new();
                let source = attribute.source;
                let source =
                    organon_theoria::Calculation::new(move |cx| source.read(cx).into_cloned())
                        .bind(&reactive, true)?;
                organon_app::evaluation::Value::from_native(
                    source.as_fixed().ok_or("sample must be fixed")?.clone(),
                )
            };
            inputs.insert(input.name.clone(), value);
        }
        if let Some(name) = samples.keys().next() {
            return Err(format!("unknown or resource sample {name:?}").into());
        }
        Scene::Element {
            preview: Box::new(preview),
            inputs,
            output: output_name,
        }
    } else {
        if !samples.is_empty() || output_name != "drawing" {
            return Err("--input and --output require --source".into());
        }
        Scene::Demo(fonts)
    };
    // Report invalid sample data before entering the native event loop.
    scene.content(480.0, 256.0)?;
    if let Some(path) = output {
        return render_png(Path::new(&path), &scene);
    }
    show_window(scene)
}

enum Scene {
    Demo(Vec<Font>),
    Element {
        preview: Box<organon_app::preview::LibraryPreview>,
        inputs: std::collections::BTreeMap<String, organon_app::evaluation::Value>,
        output: String,
    },
}

impl Scene {
    fn content(
        &self,
        width: f32,
        height: f32,
    ) -> Result<theoria_renderer::SurfaceContent, Box<dyn Error>> {
        match self {
            Self::Demo(fonts) => Ok(organon_app::preview::content(width, height, fonts)?),
            Self::Element {
                preview,
                inputs,
                output,
            } => preview.content(inputs.clone(), output, width, height),
        }
    }
}

fn render_png(path: &Path, scene: &Scene) -> Result<(), Box<dyn Error>> {
    let mut surface = ImageSurface::new(SurfaceSize {
        width: 480,
        height: 256,
    })?;
    let image = surface.render(&scene.content(480.0, 256.0)?)?;
    let mut encoder = png::Encoder::new(
        BufWriter::new(File::create(path)?),
        image.width(),
        image.height(),
    );
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_source_srgb(png::SrgbRenderingIntent::RelativeColorimetric);
    encoder.write_header()?.write_image_data(image.pixels())?;
    Ok(())
}

#[cfg(windows)]
fn show_window(scene: Scene) -> Result<(), Box<dyn Error>> {
    use theoria_windows::Window;

    let mut event_loop = theoria_windows::event_loop();
    let window = Window::new(
        &event_loop.handle(),
        "Organon",
        480,
        256,
        move |cx, window| {
            let size = window.surface().size();
            cx.create_computed(move |cx| {
                let size = *cx.read(&size);
                scene
                    .content(size.width.max(1) as f32, size.height.max(1) as f32)
                    .expect("window dimensions produce a valid drawing")
            })
        },
    )?;
    window.show();
    event_loop.run()?;
    Ok(())
}

#[cfg(not(windows))]
fn show_window(_scene: Scene) -> Result<(), Box<dyn Error>> {
    Err("window preview requires Windows; use --png output.png for offscreen rendering".into())
}
