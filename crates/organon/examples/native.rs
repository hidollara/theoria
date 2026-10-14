//! cargo run -p organon --example native
use organon::{Implementation, output};
use theoria_reactive::Reactive;

mod contracts {
    organon::import!("examples/native.organon.xml");
}
use contracts::{Input, Scale};

struct Multiplier(f32);

impl Implementation<Scale> for Multiplier {
    fn build(&self, input: Input<Scale>) -> Scale {
        let value = input.value().clone();
        let factor = self.0;
        Scale {
            scaled: output(move |cx| *value.read(cx) * factor),
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let reactive = Reactive::new();
    let implementation = Multiplier(2.0);
    let fixed = Scale::builder()
        .value(3.0)
        .build(&reactive, &implementation)?;
    println!("fixed: {:?}", fixed.scaled_fixed());

    let state = reactive.create_state(4.0);
    let dynamic = Scale::builder()
        .value(state.value())
        .build(&reactive, &implementation)?;
    let value = dynamic.scaled();
    let view = reactive.create_computed(move |cx| *cx.read(&value));
    println!("reactive: {}", *view.snapshot()?);
    state.set(5.0)?;
    println!("updated: {}", *view.snapshot()?);
    Ok(())
}
