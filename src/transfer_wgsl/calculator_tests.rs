use std::{
    future::Future,
    sync::{Arc, mpsc},
    task::{Context, Poll, Wake, Waker},
    thread,
};

use theoria_color::{
    CalculatorBinary as B, CalculatorOperation as O, CalculatorUnary as U, ColorSpace, HslSrgb,
    HwbSrgb, LabD50, LchD50, Srgb, Transfer,
};
use wgpu::util::DeviceExt;

use super::TransferWgslExt;

#[test]
fn calculator_gpu_results_match_cpu_for_color_conversions_and_lazy_branches() {
    block_on(async {
        let instance = wgpu::Instance::default();
        let Ok(adapter) = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
        else {
            eprintln!("calculator GPU test skipped: no adapter available");
            return;
        };
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .unwrap();
        let srgb = ColorSpace::from(Srgb);
        let mut cases = Vec::new();
        for space in [ColorSpace::from(HslSrgb), ColorSpace::from(HwbSrgb)] {
            for rgb in [
                [1.0, 0.0, 0.0],
                [0.2, 0.7, 0.4],
                [0.5; 3],
                [0.0; 3],
                [1.0; 3],
                [-0.2, 0.4, 1.1],
            ] {
                let to = srgb.transfer_to(&space).unwrap();
                let coordinates = to.evaluate(&rgb).unwrap();
                cases.push((to, rgb.to_vec()));
                cases.push((space.transfer_to(&srgb).unwrap(), coordinates));
            }
        }
        let lab = ColorSpace::from(LabD50);
        let lch = ColorSpace::from(LchD50);
        for coordinates in [[50.0, 0.0, 0.0], [50.0, 0.0, -40.0], [50.0, 30.0, -40.0]] {
            let to = lab.transfer_to(&lch).unwrap();
            let polar = to.evaluate(&coordinates).unwrap();
            cases.push((to, coordinates.to_vec()));
            cases.push((lch.transfer_to(&lab).unwrap(), polar));
        }
        let conditional = Transfer::calculator(
            2,
            1,
            [
                O::Input(0),
                O::Constant(0.0),
                O::Binary(B::Greater),
                O::IfElse {
                    when_true: vec![O::Input(1), O::Unary(U::SquareRoot)].into(),
                    when_false: vec![O::Constant(7.0)].into(),
                },
            ],
        )
        .unwrap();
        cases.push((conditional.clone(), vec![1.0, 9.0]));
        cases.push((conditional, vec![-1.0, -9.0]));
        for op in [U::CubeRoot, U::Round, U::Truncate] {
            cases.push((
                Transfer::calculator(1, 1, [O::Input(0), O::Unary(op)]).unwrap(),
                vec![-2.5],
            ));
        }
        cases.push((
            Transfer::calculator(2, 1, [O::Input(0), O::Input(1), O::Binary(B::Remainder)])
                .unwrap(),
            vec![-5.5, 2.0],
        ));

        let mut inputs = Vec::<f32>::new();
        let mut expected = Vec::new();
        let mut source = String::new();
        let mut body = String::new();
        for (index, (transfer, values)) in cases.iter().enumerate() {
            source.push_str(&transfer.wgsl(index as u32).unwrap());
            let args = (0..values.len())
                .map(|i| format!("inputs[{}]", inputs.len() + i))
                .collect::<Vec<_>>()
                .join(", ");
            body.push_str(&format!(
                "let result_{index} = theoria_transfer_{index}(array<f32, {}>({args}));\n",
                values.len()
            ));
            let result = transfer.evaluate(values).unwrap();
            for channel in 0..result.len() {
                body.push_str(&format!(
                    "outputs[{}] = result_{index}[{channel}];\n",
                    expected.len() + channel
                ));
            }
            inputs.extend(values.iter().map(|v| *v as f32));
            expected.extend(result);
        }
        source.push_str(&format!("@group(0) @binding(0) var<storage, read> inputs: array<f32>;\n@group(0) @binding(1) var<storage, read_write> outputs: array<f32>;\n@compute @workgroup_size(1) fn main() {{\n{body}}}"));
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("calculator parity"),
            source: wgpu::ShaderSource::Wgsl(source.into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("calculator parity"),
            layout: None,
            module: &shader,
            entry_point: Some("main"),
            compilation_options: Default::default(),
            cache: None,
        });
        let input = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: None,
            contents: bytemuck::cast_slice(&inputs),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let bytes = (expected.len() * 4) as u64;
        let output = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: bytes,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: None,
                timestamp_writes: None,
            });
            pass.set_pipeline(&pipeline);
            pass.set_bind_group(0, &bindings, &[]);
            pass.dispatch_workgroups(1, 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, bytes);
        queue.submit([encoder.finish()]);
        let (sender, receiver) = mpsc::channel();
        readback.map_async(wgpu::MapMode::Read, .., move |result| {
            sender.send(result).unwrap();
        });
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        receiver.recv().unwrap().unwrap();
        let mapped = readback.get_mapped_range(..).unwrap();
        for (index, (&actual, expected)) in bytemuck::cast_slice::<_, f32>(&mapped)
            .iter()
            .zip(expected)
            .enumerate()
        {
            assert!(
                (f64::from(actual) - expected).abs() < 0.0002 * expected.abs().max(1.0),
                "channel {index}: GPU {actual}, CPU {expected}"
            );
        }
    });
}

struct ThreadWaker(thread::Thread);
impl Wake for ThreadWaker {
    fn wake(self: Arc<Self>) {
        self.0.unpark();
    }
    fn wake_by_ref(self: &Arc<Self>) {
        self.0.unpark();
    }
}
fn block_on<F: Future>(future: F) -> F::Output {
    let waker = Waker::from(Arc::new(ThreadWaker(thread::current())));
    let mut context = Context::from_waker(&waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => thread::park(),
        }
    }
}
