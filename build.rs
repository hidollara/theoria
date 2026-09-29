use std::{env, fs, path::PathBuf};

#[path = "src/canvas/composition_shader.rs"]
mod composition_shader;
#[path = "src/rgb_shader.rs"]
mod rgb_shader;
#[path = "src/transfer_wgsl.rs"]
#[allow(dead_code)] // Build-time validation only uses the inline lowering path.
mod transfer_wgsl;

fn main() {
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    for source_file in [
        "src/canvas/composition_shader.rs",
        "src/rgb_shader.rs",
        "src/transfer_wgsl.rs",
    ] {
        println!("cargo:rerun-if-changed={source_file}");
    }
    for (source_file, output_prefix) in [
        ("shaders/vulkan/triangle.wgsl", "triangle"),
        ("shaders/vulkan/image.wgsl", "image"),
        ("shaders/canvas_renderer/composite.wgsl", "composite"),
        ("shaders/canvas_renderer/mask.wgsl", "mask"),
    ] {
        println!("cargo:rerun-if-changed={source_file}");
        compile_shader(source_file, output_prefix, &output_dir);
    }

    for source_file in [
        "shaders/canvas_renderer/composition_context.wgsl",
        "shaders/canvas_renderer/composition_fragment.wgsl",
        "shaders/canvas_renderer/css_blend.wgsl",
    ] {
        println!("cargo:rerun-if-changed={source_file}");
    }
    compile_shader_source(
        &composition_shader::assemble(
            composition_shader::Binding::Vulkan,
            "",
            composition_shader::IDENTITY_BODY,
        ),
        "composition",
        &output_dir,
        &[("vs_main", naga::ShaderStage::Vertex, "vert")],
    );

    for source_file in ["shaders/webgpu/triangle.wgsl", "shaders/webgpu/image.wgsl"] {
        println!("cargo:rerun-if-changed={source_file}");
        let source = fs::read_to_string(source_file).expect("read WebGPU WGSL shader");
        if source_file.ends_with("/image.wgsl") {
            validate_shader(&rgb_shader::assemble(&source));
        } else {
            validate_shader(&source);
        }
    }

    let output_shader = "shaders/canvas_renderer/output_image.wgsl";
    println!("cargo:rerun-if-changed={output_shader}");
    validate_shader(
        &rgb_shader::assemble_output_for(
            &fs::read_to_string(output_shader).expect("read output image shader"),
            &theoria_color::LinearSrgb.into(),
            &theoria_color::Srgb.into(),
        )
        .expect("assemble output image shader"),
    );

    let webgpu_blend = composition_shader::assemble(
        composition_shader::Binding::WebGpu,
        "",
        composition_shader::IDENTITY_BODY,
    );
    validate_shader(&webgpu_blend);
}

fn compile_shader(source_file: &str, output_prefix: &str, output_dir: &std::path::Path) {
    let source = fs::read_to_string(source_file).expect("read WGSL shader");
    let source = if source_file.ends_with("/image.wgsl") {
        rgb_shader::assemble(&source)
    } else {
        source
    };
    compile_shader_source(
        &source,
        output_prefix,
        output_dir,
        &[
            ("vs_main", naga::ShaderStage::Vertex, "vert"),
            ("fs_main", naga::ShaderStage::Fragment, "frag"),
        ],
    );
}

fn compile_shader_source(
    source: &str,
    output_prefix: &str,
    output_dir: &std::path::Path,
    stages: &[(&str, naga::ShaderStage, &str)],
) {
    let (module, info) = validated_module(source);
    let options = naga::back::spv::Options::default();

    for &(entry_point, stage, suffix) in stages {
        let pipeline_options = naga::back::spv::PipelineOptions {
            entry_point: entry_point.into(),
            shader_stage: stage,
        };
        let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options))
            .expect("compile shader to SPIR-V");

        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        fs::write(
            output_dir.join(format!("{output_prefix}.{suffix}.spv")),
            bytes,
        )
        .expect("write compiled shader");
    }
}

fn validate_shader(source: &str) {
    validated_module(source);
}

fn validated_module(source: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(source).expect("parse WGSL shader");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("validate WGSL shader");
    (module, info)
}
