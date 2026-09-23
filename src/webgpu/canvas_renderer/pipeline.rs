use std::{borrow::Cow, collections::HashMap};

use crate::canvas::{BlendMode, BlendShader, Composite};

use super::encoding::{ImageVertex, TriangleVertex};

pub(super) const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

pub(super) struct Pipelines {
    pub(super) texture_layout: wgpu::BindGroupLayout,
    pub(super) blend_uniform_layout: wgpu::BindGroupLayout,
    pub(super) triangle: wgpu::RenderPipeline,
    pub(super) image: wgpu::RenderPipeline,
    pub(super) composites: HashMap<Composite, wgpu::RenderPipeline>,
    pub(super) mask: wgpu::RenderPipeline,
    pub(super) blend: wgpu::RenderPipeline,
    custom_blends: HashMap<BlendShader, wgpu::RenderPipeline>,
    presents: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

impl Pipelines {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("theoria WebGPU sampled image"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let blend_uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("theoria WebGPU blend operation"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
                    },
                    count: None,
                }],
            });

        let triangle_shader = shader(
            device,
            "theoria WebGPU triangle shader",
            include_str!("../../../shaders/webgpu/triangle.wgsl"),
        );
        let image_shader = shader(
            device,
            "theoria WebGPU image shader",
            include_str!("../../../shaders/webgpu/image.wgsl"),
        );
        let composite_shader = shader(
            device,
            "theoria WebGPU composite shader",
            include_str!("../../../shaders/canvas_renderer/composite.wgsl"),
        );
        let mask_shader = shader(
            device,
            "theoria WebGPU mask shader",
            include_str!("../../../shaders/canvas_renderer/mask.wgsl"),
        );
        let blend_source = BlendShader::builtin(BlendMode::Normal)
            .wgsl_module("@group(2) @binding(0) var<uniform> operation: CompositionOperation;");
        let blend_shader = shader(
            device,
            "theoria WebGPU blend shader",
            Cow::Owned(blend_source),
        );

        let triangle = create_pipeline(
            device,
            "theoria WebGPU triangle pipeline",
            WORKING_FORMAT,
            &triangle_shader,
            &[Some(TriangleVertex::layout())],
            &[],
            composite_blend_state(Composite::SourceOver),
        );
        let image = create_pipeline(
            device,
            "theoria WebGPU image pipeline",
            WORKING_FORMAT,
            &image_shader,
            &[Some(ImageVertex::layout())],
            &[Some(&texture_layout)],
            composite_blend_state(Composite::SourceOver),
        );
        let composites = Composite::ALL
            .into_iter()
            .map(|composite| {
                let pipeline = create_pipeline(
                    device,
                    "theoria WebGPU composite pipeline",
                    WORKING_FORMAT,
                    &composite_shader,
                    &[],
                    &[Some(&texture_layout)],
                    composite_blend_state(composite),
                );
                (composite, pipeline)
            })
            .collect();
        let mask = create_pipeline(
            device,
            "theoria WebGPU mask pipeline",
            WORKING_FORMAT,
            &mask_shader,
            &[],
            &[Some(&texture_layout)],
            composite_blend_state(Composite::DestinationIn),
        );
        let blend = create_pipeline(
            device,
            "theoria WebGPU blend pipeline",
            WORKING_FORMAT,
            &blend_shader,
            &[],
            &[
                Some(&texture_layout),
                Some(&texture_layout),
                Some(&blend_uniform_layout),
            ],
            composite_blend_state(Composite::Copy),
        );

        Self {
            texture_layout,
            blend_uniform_layout,
            triangle,
            image,
            composites,
            mask,
            blend,
            custom_blends: HashMap::new(),
            presents: HashMap::new(),
        }
    }

    pub(super) fn ensure_custom_blends(&mut self, device: &wgpu::Device, shaders: &[BlendShader]) {
        for definition in shaders {
            self.custom_blends
                .entry(definition.clone())
                .or_insert_with(|| {
                    let source = definition.wgsl_module(
                        "@group(2) @binding(0) var<uniform> operation: CompositionOperation;",
                    );
                    let module = shader(
                        device,
                        "theoria WebGPU custom blend shader",
                        Cow::Owned(source),
                    );
                    create_pipeline(
                        device,
                        "theoria WebGPU custom blend pipeline",
                        WORKING_FORMAT,
                        &module,
                        &[],
                        &[
                            Some(&self.texture_layout),
                            Some(&self.texture_layout),
                            Some(&self.blend_uniform_layout),
                        ],
                        composite_blend_state(Composite::Copy),
                    )
                });
        }
    }

    pub(super) fn blend(&self, shader: &BlendShader) -> &wgpu::RenderPipeline {
        if shader.wgsl_source().is_some() {
            &self.custom_blends[shader]
        } else {
            &self.blend
        }
    }

    pub(super) fn present(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
    ) -> &wgpu::RenderPipeline {
        self.presents.entry(format).or_insert_with(|| {
            let shader = shader(
                device,
                "theoria WebGPU present shader",
                include_str!("../../../shaders/canvas_renderer/composite.wgsl"),
            );
            create_pipeline(
                device,
                "theoria WebGPU present pipeline",
                format,
                &shader,
                &[],
                &[Some(&self.texture_layout)],
                composite_blend_state(Composite::SourceOver),
            )
        })
    }
}

fn shader<'a>(
    device: &wgpu::Device,
    label: &'static str,
    source: impl Into<Cow<'a, str>>,
) -> wgpu::ShaderModule {
    device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    })
}

fn create_pipeline(
    device: &wgpu::Device,
    label: &'static str,
    format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
    bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
    blend: wgpu::BlendState,
) -> wgpu::RenderPipeline {
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some(label),
        bind_group_layouts,
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(label),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers,
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(blend),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

fn composite_blend_state(composite: Composite) -> wgpu::BlendState {
    let (src_factor, dst_factor) = match composite {
        Composite::Plus => (wgpu::BlendFactor::One, wgpu::BlendFactor::One),
        Composite::Clear => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::Zero),
        Composite::Copy => (wgpu::BlendFactor::One, wgpu::BlendFactor::Zero),
        Composite::Destination => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::One),
        Composite::SourceOver => (wgpu::BlendFactor::One, wgpu::BlendFactor::OneMinusSrcAlpha),
        Composite::DestinationOver => (wgpu::BlendFactor::OneMinusDstAlpha, wgpu::BlendFactor::One),
        Composite::SourceIn => (wgpu::BlendFactor::DstAlpha, wgpu::BlendFactor::Zero),
        Composite::DestinationIn => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::SrcAlpha),
        Composite::SourceOut => (wgpu::BlendFactor::OneMinusDstAlpha, wgpu::BlendFactor::Zero),
        Composite::DestinationOut => (wgpu::BlendFactor::Zero, wgpu::BlendFactor::OneMinusSrcAlpha),
        Composite::SourceAtop => (
            wgpu::BlendFactor::DstAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        ),
        Composite::DestinationAtop => (
            wgpu::BlendFactor::OneMinusDstAlpha,
            wgpu::BlendFactor::SrcAlpha,
        ),
        Composite::Xor => (
            wgpu::BlendFactor::OneMinusDstAlpha,
            wgpu::BlendFactor::OneMinusSrcAlpha,
        ),
    };
    let component = wgpu::BlendComponent {
        src_factor,
        dst_factor,
        operation: wgpu::BlendOperation::Add,
    };
    wgpu::BlendState {
        color: component,
        alpha: component,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_porter_duff_operator_maps_to_explicit_factors() {
        for composite in Composite::ALL {
            let state = composite_blend_state(composite);
            assert_eq!(state.color, state.alpha);
        }
    }
}
