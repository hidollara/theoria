use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use wgpu::util::DeviceExt;

use crate::canvas::{Composite, Composition, composition_shader::Binding};
use crate::canvas_renderer::icc_image_shader;
use crate::color::{ColorSpace, IccProfile, IccRenderingIntent};

use super::encoding::{CompositionUniform, ImageVertex, OutputUniform, TriangleVertex};

pub(super) const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(super) struct Pipelines {
    working_space: ColorSpace,
    pub(super) texture_layout: wgpu::BindGroupLayout,
    pub(super) blend_uniform_layout: wgpu::BindGroupLayout,
    pub(super) output_uniform_layout: wgpu::BindGroupLayout,
    pub(super) triangle: wgpu::RenderPipeline,
    pub(super) image: wgpu::RenderPipeline,
    pub(super) composites: HashMap<Composite, wgpu::RenderPipeline>,
    pub(super) mask: wgpu::RenderPipeline,
    compositions: HashMap<Composition, wgpu::RenderPipeline>,
    outputs: HashMap<(wgpu::TextureFormat, ColorSpace), wgpu::RenderPipeline>,
    icc_images: HashMap<(ColorSpace, bool), IccImagePipeline>,
}

pub(super) struct IccImagePipeline {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) tables: wgpu::Buffer,
}

impl Pipelines {
    pub(super) fn new(
        device: &wgpu::Device,
        working_space: &ColorSpace,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
                        min_binding_size: wgpu::BufferSize::new(std::mem::size_of::<
                            CompositionUniform,
                        >() as u64),
                    },
                    count: None,
                }],
            });
        let output_uniform_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("theoria WebGPU output image"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(
                            std::mem::size_of::<OutputUniform>() as u64,
                        ),
                    },
                    count: None,
                }],
            });

        let triangle_shader = shader(
            device,
            "theoria WebGPU triangle shader",
            include_str!("../../../shaders/webgpu/triangle.wgsl"),
        );
        let image_source = crate::rgb_shader::assemble_for(
            include_str!("../../../shaders/webgpu/image.wgsl"),
            working_space,
        )?;
        let image_shader = shader(device, "theoria WebGPU image shader", &image_source);
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
        Ok(Self {
            working_space: working_space.clone(),
            texture_layout,
            blend_uniform_layout,
            output_uniform_layout,
            triangle,
            image,
            composites,
            mask,
            compositions: HashMap::new(),
            outputs: HashMap::new(),
            icc_images: HashMap::new(),
        })
    }

    pub(super) fn ensure_icc_image(
        &mut self,
        device: &wgpu::Device,
        profile: &IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
        working_space: &ColorSpace,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = (profile.color_space(intent), has_alpha);
        if self.icc_images.contains_key(&key) {
            return Ok(());
        }
        let lowered = icc_image_shader::build(
            profile,
            intent,
            has_alpha,
            working_space,
            include_str!("../../../shaders/webgpu/icc_image.wgsl"),
        )?;
        let shader = shader(device, "theoria WebGPU ICC image shader", lowered.source);
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("theoria WebGPU ICC image"),
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
                        view_dimension: wgpu::TextureViewDimension::D2Array,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Storage { read_only: true },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let tables = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("theoria WebGPU ICC image transfer tables"),
            contents: bytemuck::cast_slice(&lowered.tables),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let pipeline = create_pipeline(
            device,
            "theoria WebGPU ICC image pipeline",
            WORKING_FORMAT,
            &shader,
            &[Some(ImageVertex::layout())],
            &[Some(&layout)],
            composite_blend_state(Composite::SourceOver),
        );
        self.icc_images.insert(
            key,
            IccImagePipeline {
                pipeline,
                layout,
                tables,
            },
        );
        Ok(())
    }

    pub(super) fn icc_image(&self, key: &(ColorSpace, bool)) -> &IccImagePipeline {
        &self.icc_images[key]
    }

    pub(super) fn retain_icc_images(&mut self, active: &HashSet<(ColorSpace, bool)>) {
        self.icc_images.retain(|key, _| active.contains(key));
    }

    pub(super) fn ensure_compositions(
        &mut self,
        device: &wgpu::Device,
        compositions: &[Composition],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let working = self.working_space.clone();
        for composition in compositions {
            if self.compositions.contains_key(composition) {
                continue;
            }
            let blend = composition.blend().map_or(&working, |blend| blend.space());
            let source = composition.blend_shader().wgsl_module_for_spaces(
                Binding::WebGpu,
                &working,
                blend,
                composition.composite_space(),
            )?;
            let module = shader(device, "theoria WebGPU blend shader", Cow::Owned(source));
            let pipeline = create_pipeline(
                device,
                "theoria WebGPU blend pipeline",
                WORKING_FORMAT,
                &module,
                &[],
                &[
                    Some(&self.texture_layout),
                    Some(&self.texture_layout),
                    Some(&self.blend_uniform_layout),
                ],
                composite_blend_state(Composite::Copy),
            );
            self.compositions.insert(composition.clone(), pipeline);
        }
        Ok(())
    }

    pub(super) fn blend(&self, composition: &Composition) -> &wgpu::RenderPipeline {
        &self.compositions[composition]
    }

    pub(super) fn output(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        space: &ColorSpace,
    ) -> Result<&wgpu::RenderPipeline, Box<dyn std::error::Error>> {
        let key = (format, space.clone());
        if !self.outputs.contains_key(&key) {
            let source = crate::rgb_shader::assemble_output_for(
                include_str!("../../../shaders/canvas_renderer/output_image.wgsl"),
                &self.working_space,
                space,
            )?;
            // Vulkan uses an immediate; WebGPU binds the same parameters as a uniform.
            let source = source.replace(
                "var<immediate> output: OutputParameters;",
                "@group(1) @binding(0) var<uniform> output: OutputParameters;",
            );
            let module = shader(device, "theoria WebGPU output shader", source);
            let pipeline = create_pipeline(
                device,
                "theoria WebGPU output pipeline",
                format,
                &module,
                &[],
                &[
                    Some(&self.texture_layout),
                    Some(&self.output_uniform_layout),
                ],
                composite_blend_state(Composite::Copy),
            );
            self.outputs.insert(key.clone(), pipeline);
        }
        Ok(&self.outputs[&key])
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
    fn icc_image_shaders_validate_for_rgb_gray_and_cmyk_samples() {
        for (profile_bytes, intent) in [
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/crates/theoria-color/src/color/fixtures/DisplayP3-v4.icc"
                ))
                .as_slice(),
                IccRenderingIntent::RelativeColorimetric,
            ),
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/crates/theoria-color/src/color/fixtures/sGrey-v4.icc"
                ))
                .as_slice(),
                IccRenderingIntent::RelativeColorimetric,
            ),
            (
                include_bytes!(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/crates/theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
                ))
                .as_slice(),
                IccRenderingIntent::Perceptual,
            ),
        ] {
            let profile = IccProfile::from_bytes(profile_bytes).unwrap();
            for has_alpha in [false, true] {
                let lowered = icc_image_shader::build(
                    &profile,
                    intent,
                    has_alpha,
                    crate::test_support::distinct_color_space(crate::color::LinearSrgb),
                    include_str!("../../../shaders/webgpu/icc_image.wgsl"),
                )
                .unwrap();
                let module = naga::front::wgsl::parse_str(&lowered.source).unwrap();
                naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                )
                .validate(&module)
                .unwrap();
            }
        }
    }

    #[test]
    fn every_porter_duff_operator_maps_to_explicit_factors() {
        for composite in Composite::ALL {
            let state = composite_blend_state(composite);
            assert_eq!(state.color, state.alpha);
        }
    }
}
