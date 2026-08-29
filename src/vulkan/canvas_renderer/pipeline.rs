use std::{collections::HashMap, error::Error, ffi::CStr, io::Cursor, mem::size_of};

use ash::vk;

use crate::canvas::Composite;

use super::encoding::{BlendCompositePushConstants, ImagePushConstants, TrianglePushConstants};

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
const BLEND_VERTEX_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blend.vert.spv"));
const BLEND_FRAGMENT_SHADER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/blend.frag.spv"));
const COVERAGE_VERTEX_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/coverage.vert.spv"));
const COVERAGE_FRAGMENT_SHADER: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/coverage.frag.spv"));

pub(super) struct Pipeline {
    pub(super) layout: vk::PipelineLayout,
    pub(super) handle: vk::Pipeline,
}

pub(super) struct Pipelines {
    pub(super) format: vk::Format,
    pub(super) sample_count: vk::SampleCountFlags,
    pub(super) triangle: Pipeline,
    pub(super) image: Pipeline,
    pub(super) composites: HashMap<Composite, Pipeline>,
    pub(super) mask: Pipeline,
    pub(super) present: Pipeline,
    pub(super) blend: Pipeline,
    pub(super) coverage: Pipeline,
}

impl Pipelines {
    pub(super) fn composite(&self, composite: Composite) -> &Pipeline {
        &self.composites[&composite]
    }
}

pub(super) unsafe fn create_pipelines(
    device: &ash::Device,
    color_format: vk::Format,
    sample_count: vk::SampleCountFlags,
    image_descriptor_set_layout: vk::DescriptorSetLayout,
) -> Result<Pipelines> {
    let triangle = unsafe {
        create_pipeline(
            device,
            color_format,
            TRIANGLE_VERTEX_SHADER,
            TRIANGLE_FRAGMENT_SHADER,
            size_of::<TrianglePushConstants>() as u32,
            vk::ShaderStageFlags::VERTEX,
            &[],
            sample_count,
            Composite::SourceOver,
        )?
    };
    let image = match unsafe {
        create_pipeline(
            device,
            color_format,
            IMAGE_VERTEX_SHADER,
            IMAGE_FRAGMENT_SHADER,
            size_of::<ImagePushConstants>() as u32,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            sample_count,
            Composite::SourceOver,
        )
    } {
        Ok(image) => image,
        Err(error) => {
            unsafe {
                device.destroy_pipeline(triangle.handle, None);
                device.destroy_pipeline_layout(triangle.layout, None);
            }
            return Err(error);
        }
    };
    let source_over = match unsafe {
        create_pipeline(
            device,
            color_format,
            COMPOSITE_VERTEX_SHADER,
            COMPOSITE_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            vk::SampleCountFlags::TYPE_1,
            Composite::SourceOver,
        )
    } {
        Ok(composite) => composite,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image] {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let mut composites = HashMap::from([(Composite::SourceOver, source_over)]);
    for composite in Composite::ALL {
        if composite == Composite::SourceOver {
            continue;
        }
        let composite_pipeline = match unsafe {
            create_pipeline(
                device,
                color_format,
                COMPOSITE_VERTEX_SHADER,
                COMPOSITE_FRAGMENT_SHADER,
                0,
                vk::ShaderStageFlags::VERTEX,
                &[image_descriptor_set_layout],
                sample_count,
                composite,
            )
        } {
            Ok(composite) => composite,
            Err(error) => {
                unsafe {
                    for pipeline in [triangle, image]
                        .into_iter()
                        .chain(composites.into_values())
                    {
                        device.destroy_pipeline(pipeline.handle, None);
                        device.destroy_pipeline_layout(pipeline.layout, None);
                    }
                }
                return Err(error);
            }
        };
        composites.insert(composite, composite_pipeline);
    }
    let mask = match unsafe {
        create_pipeline(
            device,
            color_format,
            MASK_VERTEX_SHADER,
            MASK_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            sample_count,
            Composite::DestinationIn,
        )
    } {
        Ok(mask) => mask,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let present = match unsafe {
        create_pipeline(
            device,
            color_format,
            COMPOSITE_VERTEX_SHADER,
            COMPOSITE_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[image_descriptor_set_layout],
            vk::SampleCountFlags::TYPE_1,
            Composite::SourceOver,
        )
    } {
        Ok(present) => present,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image, mask]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let blend = match unsafe {
        create_pipeline(
            device,
            color_format,
            BLEND_VERTEX_SHADER,
            BLEND_FRAGMENT_SHADER,
            size_of::<BlendCompositePushConstants>() as u32,
            vk::ShaderStageFlags::FRAGMENT,
            &[image_descriptor_set_layout, image_descriptor_set_layout],
            sample_count,
            Composite::Copy,
        )
    } {
        Ok(blend) => blend,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image, mask, present]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    let coverage = match unsafe {
        create_pipeline(
            device,
            color_format,
            COVERAGE_VERTEX_SHADER,
            COVERAGE_FRAGMENT_SHADER,
            0,
            vk::ShaderStageFlags::VERTEX,
            &[
                image_descriptor_set_layout,
                image_descriptor_set_layout,
                image_descriptor_set_layout,
            ],
            sample_count,
            Composite::Copy,
        )
    } {
        Ok(coverage) => coverage,
        Err(error) => {
            unsafe {
                for pipeline in [triangle, image, mask, present, blend]
                    .into_iter()
                    .chain(composites.into_values())
                {
                    device.destroy_pipeline(pipeline.handle, None);
                    device.destroy_pipeline_layout(pipeline.layout, None);
                }
            }
            return Err(error);
        }
    };
    Ok(Pipelines {
        format: color_format,
        sample_count,
        triangle,
        image,
        composites,
        mask,
        present,
        blend,
        coverage,
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
unsafe fn create_pipeline(
    device: &ash::Device,
    color_format: vk::Format,
    vertex_shader: &[u8],
    fragment_shader: &[u8],
    push_constant_size: u32,
    push_constant_stages: vk::ShaderStageFlags,
    descriptor_set_layouts: &[vk::DescriptorSetLayout],
    sample_count: vk::SampleCountFlags,
    blend: Composite,
) -> Result<Pipeline> {
    let vertex_code = ash::util::read_spv(&mut Cursor::new(vertex_shader))?;
    let fragment_code = ash::util::read_spv(&mut Cursor::new(fragment_shader))?;
    let vertex_module = unsafe {
        device.create_shader_module(
            &vk::ShaderModuleCreateInfo::default().code(&vertex_code),
            None,
        )?
    };
    let fragment_module = unsafe {
        device.create_shader_module(
            &vk::ShaderModuleCreateInfo::default().code(&fragment_code),
            None,
        )?
    };

    let result = (|| {
        let vertex_entry: &CStr = c"vs_main";
        let fragment_entry: &CStr = c"fs_main";
        let stages = [
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::VERTEX)
                .module(vertex_module)
                .name(vertex_entry),
            vk::PipelineShaderStageCreateInfo::default()
                .stage(vk::ShaderStageFlags::FRAGMENT)
                .module(fragment_module)
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
        let pipeline = unsafe {
            device
                .create_graphics_pipelines(vk::PipelineCache::null(), &[info], None)
                .map_err(|(_, error)| error)?[0]
        };
        Ok::<_, Box<dyn Error>>(Pipeline {
            layout,
            handle: pipeline,
        })
    })();
    unsafe {
        device.destroy_shader_module(fragment_module, None);
        device.destroy_shader_module(vertex_module, None);
    }
    result
}
