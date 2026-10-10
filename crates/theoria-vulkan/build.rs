use std::{env, fs, path::PathBuf};

use theoria_renderer::{composition_shader, rgb_shader, shaders};

fn main() {
    let output_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is set"));
    for (source_file, output_prefix) in [
        ("shaders/triangle.wgsl", "triangle"),
        ("shaders/image.wgsl", "image"),
    ] {
        println!("cargo:rerun-if-changed={source_file}");
        compile_shader(source_file, output_prefix, &output_dir);
    }

    for (source, output_prefix) in [(shaders::COMPOSITE, "composite"), (shaders::MASK, "mask")] {
        compile_shader_source(
            source,
            output_prefix,
            &output_dir,
            &[
                ("vs_main", naga::ShaderStage::Vertex, "vert"),
                ("fs_main", naga::ShaderStage::Fragment, "frag"),
            ],
        );
    }
    compile_shader_source(
        &composition_shader::assemble(
            "var<immediate> operation: CompositionOperation;",
            "",
            composition_shader::IDENTITY_BODY,
        ),
        "composition",
        &output_dir,
        &[("vs_main", naga::ShaderStage::Vertex, "vert")],
    );

    validate_shader(
        &rgb_shader::assemble_output_for(
            shaders::OUTPUT_IMAGE,
            &theoria_color::LinearSrgb.into(),
            &theoria_color::Srgb.into(),
        )
        .expect("assemble output image shader"),
    );
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
