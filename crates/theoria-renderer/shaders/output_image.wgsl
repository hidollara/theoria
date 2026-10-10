struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

struct OutputParameters {
    clear_color: vec4<f32>,
    alpha_mode: u32,
}

var<immediate> output: OutputParameters;

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    var result: VertexOutput;
    result.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    result.uv = uvs[vertex_index];
    return result;
}

// @theoria-rgb-conversions

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let source = textureSample(image_texture, image_sampler, input.uv);
    let source_alpha = clamp(source.a, 0.0, 1.0);
    var source_premultiplied = vec3<f32>(0.0);
    if source_alpha > 0.0 {
        source_premultiplied = working_to_composition(source.rgb / source_alpha) * source_alpha;
    }
    let result_alpha = source_alpha + output.clear_color.a * (1.0 - source_alpha);
    let result_premultiplied = source_premultiplied + output.clear_color.rgb * (1.0 - source_alpha);
    // An opaque destination displays the remaining transparency over black.
    if output.alpha_mode == 2u {
        return vec4<f32>(composition_to_output(result_premultiplied), 1.0);
    }
    if result_alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let straight = composition_to_output(result_premultiplied / result_alpha);
    if output.alpha_mode == 1u {
        return vec4<f32>(straight, result_alpha);
    }
    return vec4<f32>(straight * result_alpha, result_alpha);
}
