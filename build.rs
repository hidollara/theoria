use std::{env, fs, path::PathBuf};

fn main() {
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    for (source_file, output_prefix) in [
        ("shaders/vulkan/triangle.wgsl", "triangle"),
        ("shaders/vulkan/image.wgsl", "image"),
        ("shaders/vulkan/composite.wgsl", "composite"),
        ("shaders/vulkan/mask.wgsl", "mask"),
        ("shaders/vulkan/blend.wgsl", "blend"),
    ] {
        println!("cargo:rerun-if-changed={source_file}");
        compile_shader(source_file, output_prefix, &output_dir);
    }
}

fn compile_shader(source_file: &str, output_prefix: &str, output_dir: &std::path::Path) {
    let source = fs::read_to_string(source_file).expect("read WGSL shader");
    let module = naga::front::wgsl::parse_str(&source).expect("parse WGSL shader");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .expect("validate WGSL shader");
    let options = naga::back::spv::Options::default();

    for (entry_point, stage, suffix) in [
        ("vs_main", naga::ShaderStage::Vertex, "vert"),
        ("fs_main", naga::ShaderStage::Fragment, "frag"),
    ] {
        let pipeline_options = naga::back::spv::PipelineOptions {
            entry_point: entry_point.into(),
            shader_stage: stage,
        };
        let words = naga::back::spv::write_vec(&module, &info, &options, Some(&pipeline_options))
            .expect("compile triangle shader to SPIR-V");

        let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        fs::write(
            output_dir.join(format!("{output_prefix}.{suffix}.spv")),
            bytes,
        )
        .expect("write compiled shader");
    }
}
