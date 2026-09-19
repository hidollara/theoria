struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) uvw_and_association: vec4<f32>,
    @location(2) transparent_extension: vec2<u32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) rgb_is_premultiplied: f32,
    @location(2) @interpolate(flat) transparent_extension: vec2<u32>,
}

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = input.position;
    output.uvw = input.uvw_and_association.xyz;
    output.rgb_is_premultiplied = input.uvw_and_association.w;
    output.transparent_extension = input.transparent_extension;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uvw.xy / input.uvw.z;
    if (input.transparent_extension.x != 0u && (uv.x < 0.0 || uv.x > 1.0)) ||
       (input.transparent_extension.y != 0u && (uv.y < 0.0 || uv.y > 1.0)) {
        return vec4<f32>(0.0);
    }
    let sampled = textureSample(image_texture, image_sampler, uv);
    let premultiplied_rgb = mix(
        sampled.rgb * sampled.a,
        sampled.rgb,
        input.rgb_is_premultiplied,
    );
    return vec4<f32>(premultiplied_rgb, sampled.a);
}
