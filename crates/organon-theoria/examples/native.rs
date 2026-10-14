//! cargo run -p organon-theoria --example native
use organon_theoria::{ReadContext, Rect};
use theoria_reactive::Reactive;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reactive = Reactive::new();
    let width = reactive.create_state(40.0_f32);
    let rectangle = Rect::builder()
        .x(0.0)
        .y(0.0)
        .width(width.value())
        .height(20.0)
        .build(&reactive)?;
    let path = rectangle.path();
    let view = reactive.create_computed(move |cx| path.read(&ReadContext::new(cx)).into_cloned());
    println!("initial: {:?}", view.snapshot()?);
    width.set(80.0)?;
    println!("updated: {:?}", view.snapshot()?);
    Ok(())
}
