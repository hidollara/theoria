struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) rgb_is_premultiplied: f32,
}

// Sampled images may enter with straight or premultiplied RGB. This shader
// normalizes both cases to the renderer's premultiplied linear working form.

struct ImageTriangle {
    position_0: vec4<f32>,
    uv_0: vec4<f32>,
    position_1: vec4<f32>,
    uv_1: vec4<f32>,
    position_2: vec4<f32>,
    uv_2: vec4<f32>,
}

var<immediate> triangle: ImageTriangle;

@group(0) @binding(0)
var image_sampler: sampler;

@group(0) @binding(1)
var image_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(
        triangle.position_0,
        triangle.position_1,
        triangle.position_2,
    );
    let uvs = array(
        triangle.uv_0,
        triangle.uv_1,
        triangle.uv_2,
    );

    var output: VertexOutput;
    output.position = positions[vertex_index];
    output.uvw = uvs[vertex_index].xyz;
    output.rgb_is_premultiplied = uvs[vertex_index].w;
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_texture, image_sampler, input.uvw.xy / input.uvw.z);
    let premultiplied_rgb = mix(
        sampled.rgb * sampled.a,
        sampled.rgb,
        input.rgb_is_premultiplied,
    );
    return vec4<f32>(premultiplied_rgb, sampled.a);
}
