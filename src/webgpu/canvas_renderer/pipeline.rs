use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};

use wgpu::util::DeviceExt;

use crate::canvas::{BlendShader, Composite, composition_shader::Binding};
use crate::canvas_renderer::output_color::OutputColorTransform;
use crate::color::{ColorSpaceKey, ColorTransform, IccProfile, IccRenderingIntent, RgbColorSpace};

use super::encoding::{ImageVertex, TriangleVertex};

pub(super) const WORKING_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

pub(super) struct Pipelines {
    pub(super) texture_layout: wgpu::BindGroupLayout,
    pub(super) blend_uniform_layout: wgpu::BindGroupLayout,
    pub(super) triangle: wgpu::RenderPipeline,
    pub(super) image: wgpu::RenderPipeline,
    pub(super) composites: HashMap<Composite, wgpu::RenderPipeline>,
    pub(super) mask: wgpu::RenderPipeline,
    blend_shaders: HashMap<BlendShader, wgpu::RenderPipeline>,
    presents: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
    icc_presents:
        HashMap<(wgpu::TextureFormat, ColorSpaceKey, IccRenderingIntent), wgpu::RenderPipeline>,
    icc_images: HashMap<(ColorSpaceKey, IccRenderingIntent, bool), IccImagePipeline>,
}

pub(super) struct IccImagePipeline {
    pub(super) pipeline: wgpu::RenderPipeline,
    pub(super) layout: wgpu::BindGroupLayout,
    pub(super) tables: wgpu::Buffer,
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
        let image_source =
            crate::rgb_shader::assemble(include_str!("../../../shaders/webgpu/image.wgsl"));
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
        Self {
            texture_layout,
            blend_uniform_layout,
            triangle,
            image,
            composites,
            mask,
            blend_shaders: HashMap::new(),
            presents: HashMap::new(),
            icc_presents: HashMap::new(),
            icc_images: HashMap::new(),
        }
    }

    pub(super) fn ensure_icc_image(
        &mut self,
        device: &wgpu::Device,
        profile: &IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = (profile.device_space(), intent, has_alpha);
        if self.icc_images.contains_key(&key) {
            return Ok(());
        }
        let (shader_source, table_values) = icc_image_shader(profile, intent, has_alpha)?;
        let shader = shader(device, "theoria WebGPU ICC image shader", shader_source);
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
            contents: bytemuck::cast_slice(&table_values),
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

    pub(super) fn icc_image(
        &self,
        key: &(ColorSpaceKey, IccRenderingIntent, bool),
    ) -> &IccImagePipeline {
        &self.icc_images[key]
    }

    pub(super) fn retain_icc_images(
        &mut self,
        active: &HashSet<(ColorSpaceKey, IccRenderingIntent, bool)>,
    ) {
        self.icc_images.retain(|key, _| active.contains(key));
    }

    pub(super) fn ensure_blend_shaders(&mut self, device: &wgpu::Device, shaders: &[BlendShader]) {
        for definition in shaders {
            self.blend_shaders
                .entry(definition.clone())
                .or_insert_with(|| {
                    let source = definition.wgsl_module(Binding::WebGpu);
                    let module = shader(device, "theoria WebGPU blend shader", Cow::Owned(source));
                    create_pipeline(
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
                    )
                });
        }
    }

    pub(super) fn blend(&self, shader: &BlendShader) -> &wgpu::RenderPipeline {
        &self.blend_shaders[shader]
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

    pub(super) fn icc_present(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        profile: &IccProfile,
        intent: IccRenderingIntent,
        transform: &OutputColorTransform,
    ) -> Result<&wgpu::RenderPipeline, Box<dyn std::error::Error>> {
        let key = (format, profile.device_space(), intent);
        if !self.icc_presents.contains_key(&key) {
            let source = transform.wgsl()?;
            let module = shader(device, "theoria WebGPU ICC present shader", source);
            let pipeline = create_pipeline(
                device,
                "theoria WebGPU ICC present pipeline",
                format,
                &module,
                &[],
                &[Some(&self.texture_layout)],
                composite_blend_state(Composite::SourceOver),
            );
            self.icc_presents.insert(key.clone(), pipeline);
        }
        Ok(&self.icc_presents[&key])
    }
}

fn icc_image_shader(
    profile: &IccProfile,
    intent: IccRenderingIntent,
    has_alpha: bool,
) -> Result<(String, Vec<f32>), Box<dyn std::error::Error>> {
    let to_pcs = profile.to_pcs(intent)?;
    let to_working = ColorTransform::pcs_to_rgb(profile.pcs(), RgbColorSpace::LinearSrgb)?;
    let transform = to_pcs.then(&to_working)?;
    let transfer = transform.transfer().wgsl_with_storage(0, 0, 2)?;
    let channels = profile.device_channels();
    let total = channels + usize::from(has_alpha);
    let layers = total.div_ceil(4);
    let components = ["r", "g", "b", "a"];
    let mut samples = String::new();
    for layer in 0..layers {
        samples.push_str(&format!(
            "    let layer_{layer} = textureSampleLevel(image_texture, image_sampler, uv, {layer}, 0.0);\n"
        ));
    }
    let values = (0..total)
        .map(|channel| format!("layer_{}.{}", channel / 4, components[channel % 4]))
        .collect::<Vec<_>>()
        .join(", ");
    samples.push_str(&format!("    return array<f32, {total}>({values});\n"));
    let input = (0..channels)
        .map(|channel| format!("sample[{channel}]"))
        .collect::<Vec<_>>()
        .join(", ");
    let alpha = if has_alpha {
        format!("sample[{channels}]")
    } else {
        "1.0".to_owned()
    };
    let source = include_str!("../../../shaders/webgpu/icc_image.wgsl")
        .replace("// @theoria-transfer", transfer.source())
        .replace("// @theoria-sample-values", &samples)
        .replace("THEORIA_SAMPLE_CHANNELS", &total.to_string())
        .replace("THEORIA_DEVICE_CHANNELS", &channels.to_string())
        .replace("THEORIA_DEVICE_VALUES", &input)
        .replace("THEORIA_ALPHA", &alpha);
    let tables = if transfer.table_values().is_empty() {
        vec![0.0f32]
    } else {
        transfer.table_values().to_vec()
    };
    Ok((source, tables))
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
                include_bytes!("../../color/fixtures/DisplayP3-v4.icc").as_slice(),
                IccRenderingIntent::RelativeColorimetric,
            ),
            (
                include_bytes!("../../color/fixtures/sGrey-v4.icc").as_slice(),
                IccRenderingIntent::RelativeColorimetric,
            ),
            (
                include_bytes!("../../color/fixtures/CGATS001Compat-v2-micro.icc").as_slice(),
                IccRenderingIntent::Perceptual,
            ),
        ] {
            let profile = IccProfile::from_bytes(profile_bytes).unwrap();
            for has_alpha in [false, true] {
                let (source, _) = icc_image_shader(&profile, intent, has_alpha).unwrap();
                let module = naga::front::wgsl::parse_str(&source).unwrap();
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
