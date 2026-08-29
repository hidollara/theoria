struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

@group(0) @binding(0) var destination_sampler: sampler;
@group(0) @binding(1) var destination_texture: texture_2d<f32>;
@group(1) @binding(0) var candidate_sampler: sampler;
@group(1) @binding(1) var candidate_texture: texture_2d<f32>;
@group(2) @binding(0) var coverage_sampler: sampler;
@group(2) @binding(1) var coverage_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let destination = textureSample(destination_texture, destination_sampler, input.uv);
    let candidate = textureSample(candidate_texture, candidate_sampler, input.uv);
    let coverage = clamp(textureSample(coverage_texture, coverage_sampler, input.uv).a, 0.0, 1.0);
    return mix(destination, candidate, coverage);
}
