struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

// @theoria-rgb-conversions

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let rgba = textureSample(image_texture, image_sampler, input.uv);
    if rgba.a <= 0.0 {
        return vec4<f32>(0.0);
    }
    let straight = working_to_linear_srgb(rgba.rgb / rgba.a);
    return vec4<f32>(straight * rgba.a, rgba.a);
}
