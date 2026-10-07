use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    error::Error,
    ffi::CStr,
    io::Cursor,
    mem::size_of,
};

use ash::vk;

use crate::canvas_renderer::{blend::BlendShaderExt, composition_shader::Binding};

use crate::{
    canvas::{Composite, Composition},
    color::{ColorSpace, LinearSrgb},
};

use super::encoding::{
    CompositionPushConstants, ImagePushConstants, OutputPushConstants, TrianglePushConstants,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

const TRIANGLE_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/triangle.vert.spv"));
const TRIANGLE_FRAGMENT_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/triangle.frag.spv"));
const IMAGE_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.vert.spv"));
const IMAGE_FRAGMENT_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/image.frag.spv"));
const COMPOSITE_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/composite.vert.spv"));
const COMPOSITE_FRAGMENT_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/composite.frag.spv"));
const MASK_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mask.vert.spv"));
const MASK_FRAGMENT_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mask.frag.spv"));
const COMPOSITION_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/composition.vert.spv"));

struct ShaderModule<'a> {
    device: &'a ash::Device,
    handle: vk::ShaderModule,
}

impl<'a> ShaderModule<'a> {
    unsafe fn new(device: &'a ash::Device, code: &[u32]) -> Result<Self> {
        let handle = unsafe {
            device.create_shader_module(&vk::ShaderModuleCreateInfo::default().code(code), None)?
        };
        Ok(Self { device, handle })
    }
}

impl Drop for ShaderModule<'_> {
    fn drop(&mut self) {
        unsafe {
            self.device.destroy_shader_module(self.handle, None);
        }
    }
}

pub(super) struct Pipeline {
    pub(super) layout: vk::PipelineLayout,
    pub(super) handle: vk::Pipeline,
}

struct PipelineGuard<'a> {
    device: &'a ash::Device,
    pipeline: Option<Pipeline>,
}

impl PipelineGuard<'_> {
    fn into_pipeline(mut self) -> Pipeline {
        self.pipeline.take().unwrap()
    }
}

impl Drop for PipelineGuard<'_> {
    fn drop(&mut self) {
        if let Some(pipeline) = self.pipeline.take() {
            unsafe {
                self.device.destroy_pipeline(pipeline.handle, None);
                self.device.destroy_pipeline_layout(pipeline.layout, None);
            }
        }
    }
}

pub(super) struct Pipelines {
    pub(super) working_space: ColorSpace,
    pub(super) working_format: vk::Format,
    pub(super) output_format: vk::Format,
    pub(super) sample_count: vk::SampleCountFlags,
    pub(super) triangle: Pipeline,
    pub(super) image: Pipeline,
    pub(super) composites: HashMap<Composite, Pipeline>,
    pub(super) mask: Pipeline,
    pub(super) outputs: HashMap<ColorSpace, Pipeline>,
    pub(super) icc_images: HashMap<(ColorSpace, bool), Pipeline>,
    pub(super) compositions: HashMap<Composition, Pipeline>,
}

impl Pipelines {
    pub(super) fn icc_image(&self, key: &(ColorSpace, bool)) -> &Pipeline {
        &self.icc_images[key]
    }

    pub(super) unsafe fn ensure_icc_image(
        &mut self,
        device: &ash::Device,
        descriptor_layout: vk::DescriptorSetLayout,
        key: (ColorSpace, bool),
        source: &str,
    ) -> Result<()> {
        if self.icc_images.contains_key(&key) {
            return Ok(());
        }
        let words = compile_fragment(source)?;
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let pipeline = unsafe {
            create_pipeline(
                device,
                self.working_format,
                IMAGE_VERTEX_SHADER,
                &bytes,
                size_of::<ImagePushConstants>() as u32,
                vk::ShaderStageFlags::VERTEX,
                &[descriptor_layout],
                self.sample_count,
                Composite::SourceOver,
            )?
        };
        self.icc_images.insert(key, pipeline.into_pipeline());
        Ok(())
    }

    pub(super) unsafe fn retain_icc_images(
        &mut self,
        device: &ash::Device,
        active: &HashSet<(ColorSpace, bool)>,
    ) {
        self.icc_images.retain(|key, pipeline| {
            if active.contains(key) {
                return true;
            }
            unsafe {
                device.destroy_pipeline(pipeline.handle, None);
                device.destroy_pipeline_layout(pipeline.layout, None);
            }
            false
        });
    }

    pub(super) fn output(&self, space: &ColorSpace) -> &Pipeline {
        &self.outputs[space]
    }

    pub(super) unsafe fn ensure_output(
        &mut self,
        device: &ash::Device,
        image_descriptor_set_layout: vk::DescriptorSetLayout,
        space: &ColorSpace,
    ) -> Result<()> {
        let key = space.clone();
        if self.outputs.contains_key(&key) {
            return Ok(());
        }
        let source = crate::rgb_shader::assemble_output_for(
            include_str!("../../../shaders/canvas_renderer/output_image.wgsl"),
            &self.working_space,
            space,
        )?;
        let words = compile_fragment(&source)?;
        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        let pipeline = unsafe {
            create_pipeline(
                device,
                self.output_format,
                COMPOSITION_VERTEX_SHADER,
                &bytes,
                size_of::<OutputPushConstants>() as u32,
                vk::ShaderStageFlags::FRAGMENT,
                &[image_descriptor_set_layout],
                vk::SampleCountFlags::TYPE_1,
                Composite::Copy,
            )?
        };
        self.outputs.insert(key, pipeline.into_pipeline());
        Ok(())
    }

    pub(super) fn composite(&self, composite: Composite) -> &Pipeline {
        &self.composites[&composite]
    }

    pub(super) fn blend(&self, composition: &Composition) -> &Pipeline {
        &self.compositions[composition]
    }

    pub(super) unsafe fn ensure_compositions(
        &mut self,
        device: &ash::Device,
        image_descriptor_set_layout: vk::DescriptorSetLayout,
        compositions: &[Composition],
    ) -> Result<()> {
        let working = self.working_space.clone();
        for composition in compositions {
            if self.compositions.contains_key(composition) {
                continue;
            }
            let blend = composition.blend().map_or(&working, |blend| blend.space());
            let source = composition.blend_shader().wgsl_module_for_spaces(
                Binding::Vulkan,
                &working,
                blend,
                composition.composite_space(),
            )?;
            let words = compile_fragment(&source)?;
            let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
            let pipeline = unsafe {
                create_pipeline(
                    device,
                    self.working_format,
                    COMPOSITION_VERTEX_SHADER,
                    &bytes,
                    size_of::<CompositionPushConstants>() as u32,
                    vk::ShaderStageFlags::FRAGMENT,
                    &[image_descriptor_set_layout, image_descriptor_set_layout],
                    self.sample_count,
                    Composite::Copy,
                )?
            };
            self.compositions
                .insert(composition.clone(), pipeline.into_pipeline());
        }
        Ok(())
    }
}

fn compile_fragment(source: &str) -> Result<Vec<u32>> {
    let module = naga::front::wgsl::parse_str(source)
        .map_err(|error| std::io::Error::other(error.emit_to_string(source)))?;
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    let options = naga::back::spv::PipelineOptions {
        entry_point: "fs_main".into(),
        shader_stage: naga::ShaderStage::Fragment,
    };
    naga::back::spv::write_vec(
        &module,
        &info,
        &naga::back::spv::Options::default(),
        Some(&options),
    )
    .map_err(|error| std::io::Error::other(error.to_string()).into())
}

pub(super) unsafe fn create_pipelines(
    device: &ash::Device,
    working_format: vk::Format,
    output_format: vk::Format,
    sample_count: vk::SampleCountFlags,
    image_descriptor_set_layout: vk::DescriptorSetLayout,
    working_space: &ColorSpace,
) -> Result<Pipelines> {
    let image_fragment: Cow<'_, [u8]> = if working_space == &LinearSrgb.into() {
        Cow::Borrowed(IMAGE_FRAGMENT_SHADER)
    } else {
        let source = crate::rgb_shader::assemble_for(
            include_str!("../../../shaders/vulkan/image.wgsl"),
            working_space,
        )?;
        Cow::Owned(
            compile_fragment(&source)?
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect(),
        )
    };
    let triangle = unsafe {
        create_pipeline(
            device,
            working_format,
            TRIANGLE_VERTEX_SHADER,
            TRIANGLE_FRAGMENT_SHADER,
            size_of::<TrianglePushConstants>() as u32,
            vk::ShaderStageFlags::VERTEX,
            &[],
            sample_count,
            Composite::SourceOver,
        )?
    };
    let image = unsafe {
        create_pipeline(
            device,
            working_format,
            IMAGE_VERTEX_SHADER,
            &image_fragment,
            size_of::<ImagePushConstants>() as u32,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            sample_count,
            Composite::SourceOver,
        )?
    };
    let source_over = unsafe {
        create_pipeline(
            device,
            working_format,
            COMPOSITE_VERTEX_SHADER,
            COMPOSITE_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            vk::SampleCountFlags::TYPE_1,
            Composite::SourceOver,
        )?
    };
    let mut composites = HashMap::from([(Composite::SourceOver, source_over)]);
    for composite in Composite::ALL {
        if composite == Composite::SourceOver {
            continue;
        }
        let composite_pipeline = unsafe {
            create_pipeline(
                device,
                working_format,
                COMPOSITE_VERTEX_SHADER,
                COMPOSITE_FRAGMENT_SHADER,
                0,
                vk::ShaderStageFlags::VERTEX,
                &[image_descriptor_set_layout],
                sample_count,
                composite,
            )?
        };
        composites.insert(composite, composite_pipeline);
    }
    let mask = unsafe {
        create_pipeline(
            device,
            working_format,
            MASK_VERTEX_SHADER,
            MASK_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            sample_count,
            Composite::DestinationIn,
        )?
    };
    Ok(Pipelines {
        working_space: working_space.clone(),
        working_format,
        output_format,
        sample_count,
        triangle: triangle.into_pipeline(),
        image: image.into_pipeline(),
        composites: composites
            .into_iter()
            .map(|(composite, pipeline)| (composite, pipeline.into_pipeline()))
            .collect(),
        mask: mask.into_pipeline(),
        outputs: HashMap::new(),
        icc_images: HashMap::new(),
        compositions: HashMap::new(),
    })
}

pub(super) fn composite_blend_factors(composite: Composite) -> (vk::BlendFactor, vk::BlendFactor) {
    match composite {
        Composite::Plus => (vk::BlendFactor::ONE, vk::BlendFactor::ONE),
        Composite::Clear => (vk::BlendFactor::ZERO, vk::BlendFactor::ZERO),
        Composite::Copy => (vk::BlendFactor::ONE, vk::BlendFactor::ZERO),
        Composite::Destination => (vk::BlendFactor::ZERO, vk::BlendFactor::ONE),
        Composite::SourceOver => (vk::BlendFactor::ONE, vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
        Composite::DestinationOver => (vk::BlendFactor::ONE_MINUS_DST_ALPHA, vk::BlendFactor::ONE),
        Composite::SourceIn => (vk::BlendFactor::DST_ALPHA, vk::BlendFactor::ZERO),
        Composite::DestinationIn => (vk::BlendFactor::ZERO, vk::BlendFactor::SRC_ALPHA),
        Composite::SourceOut => (vk::BlendFactor::ONE_MINUS_DST_ALPHA, vk::BlendFactor::ZERO),
        Composite::DestinationOut => (vk::BlendFactor::ZERO, vk::BlendFactor::ONE_MINUS_SRC_ALPHA),
        Composite::SourceAtop => (
            vk::BlendFactor::DST_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
        Composite::DestinationAtop => (
            vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            vk::BlendFactor::SRC_ALPHA,
        ),
        Composite::Xor => (
            vk::BlendFactor::ONE_MINUS_DST_ALPHA,
            vk::BlendFactor::ONE_MINUS_SRC_ALPHA,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
unsafe fn create_pipeline<'a>(
    device: &'a ash::Device,
    color_format: vk::Format,
    vertex_shader: &[u8],
    fragment_shader: &[u8],
    push_constant_size: u32,
    push_constant_stages: vk::ShaderStageFlags,
    descriptor_set_layouts: &[vk::DescriptorSetLayout],
    sample_count: vk::SampleCountFlags,
    blend: Composite,
) -> Result<PipelineGuard<'a>> {
    let vertex_code = ash::util::read_spv(&mut Cursor::new(vertex_shader))?;
    let fragment_code = ash::util::read_spv(&mut Cursor::new(fragment_shader))?;
    let vertex_module = unsafe { ShaderModule::new(device, &vertex_code)? };
    let fragment_module = unsafe { ShaderModule::new(device, &fragment_code)? };

    let vertex_entry: &CStr = c"vs_main";
    let fragment_entry: &CStr = c"fs_main";
    let stages = [
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::VERTEX)
            .module(vertex_module.handle)
            .name(vertex_entry),
        vk::PipelineShaderStageCreateInfo::default()
            .stage(vk::ShaderStageFlags::FRAGMENT)
            .module(fragment_module.handle)
            .name(fragment_entry),
    ];
    let (src_color, dst_color) = composite_blend_factors(blend);
    let blend_attachment = vk::PipelineColorBlendAttachmentState::default()
        .blend_enable(true)
        .src_color_blend_factor(src_color)
        .dst_color_blend_factor(dst_color)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(src_color)
        .dst_alpha_blend_factor(dst_color)
        .alpha_blend_op(vk::BlendOp::ADD)
        .color_write_mask(
            vk::ColorComponentFlags::R
                | vk::ColorComponentFlags::G
                | vk::ColorComponentFlags::B
                | vk::ColorComponentFlags::A,
        );
    let push_range = vk::PushConstantRange::default()
        .stage_flags(push_constant_stages)
        .size(push_constant_size);
    let push_ranges = if push_constant_size == 0 {
        &[][..]
    } else {
        std::slice::from_ref(&push_range)
    };
    let layout = unsafe {
        device.create_pipeline_layout(
            &vk::PipelineLayoutCreateInfo::default()
                .set_layouts(descriptor_set_layouts)
                .push_constant_ranges(push_ranges),
            None,
        )?
    };
    let dynamic_states = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
    let vertex_input = vk::PipelineVertexInputStateCreateInfo::default();
    let input_assembly = vk::PipelineInputAssemblyStateCreateInfo::default()
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST);
    let viewport = vk::PipelineViewportStateCreateInfo::default()
        .viewport_count(1)
        .scissor_count(1);
    let rasterization = vk::PipelineRasterizationStateCreateInfo::default()
        .polygon_mode(vk::PolygonMode::FILL)
        .cull_mode(vk::CullModeFlags::NONE)
        .front_face(vk::FrontFace::COUNTER_CLOCKWISE)
        .line_width(1.0);
    let multisample =
        vk::PipelineMultisampleStateCreateInfo::default().rasterization_samples(sample_count);
    let color_blend = vk::PipelineColorBlendStateCreateInfo::default()
        .attachments(std::slice::from_ref(&blend_attachment));
    let dynamic = vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_states);
    let color_formats = [color_format];
    let mut rendering_info =
        vk::PipelineRenderingCreateInfo::default().color_attachment_formats(&color_formats);
    let info = vk::GraphicsPipelineCreateInfo::default()
        .stages(&stages)
        .vertex_input_state(&vertex_input)
        .input_assembly_state(&input_assembly)
        .viewport_state(&viewport)
        .rasterization_state(&rasterization)
        .multisample_state(&multisample)
        .color_blend_state(&color_blend)
        .dynamic_state(&dynamic)
        .layout(layout)
        .render_pass(vk::RenderPass::null())
        .push_next(&mut rendering_info);
    let pipeline =
        match unsafe { device.create_graphics_pipelines(vk::PipelineCache::null(), &[info], None) }
        {
            Ok(pipelines) => pipelines[0],
            Err((pipelines, error)) => {
                unsafe {
                    for pipeline in pipelines {
                        device.destroy_pipeline(pipeline, None);
                    }
                    device.destroy_pipeline_layout(layout, None);
                }
                return Err(error.into());
            }
        };
    Ok(PipelineGuard {
        device,
        pipeline: Some(Pipeline {
            layout,
            handle: pipeline,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::IccRenderingIntent;
    use crate::{
        canvas::{Blend, BlendMode, BlendShader},
        color::{IccProfile, Srgb},
    };

    #[test]
    fn css_and_custom_blends_compile_for_vulkan() {
        for mode in BlendMode::ALL {
            let blend = Blend::new(mode, Srgb);
            let source = blend.shader().wgsl_module(Binding::Vulkan);
            assert!(!compile_fragment(&source).unwrap().is_empty(), "{mode:?}");
        }

        let shader = BlendShader::wgsl("return backdrop * source;").unwrap();
        let source = shader.wgsl_module(Binding::Vulkan);
        assert!(!compile_fragment(&source).unwrap().is_empty());

        let source = shader
            .wgsl_module_for_spaces(
                Binding::Vulkan,
                &ColorSpace::from(crate::color::LinearSrgb),
                &ColorSpace::from(Srgb),
                &ColorSpace::from(Srgb),
            )
            .unwrap();
        assert!(!compile_fragment(&source).unwrap().is_empty());
    }

    #[test]
    fn icc_image_fragment_compiles_for_cmyk_with_alpha() {
        let profile = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/CGATS001Compat-v2-micro.icc"
        )))
        .unwrap();
        let lowered = crate::canvas_renderer::icc_image_shader::build(
            &profile,
            IccRenderingIntent::Perceptual,
            true,
            crate::color::LinearSrgb,
            include_str!("../../../shaders/vulkan/icc_image.wgsl"),
        )
        .unwrap();
        assert!(!compile_fragment(&lowered.source).unwrap().is_empty());
    }
}
