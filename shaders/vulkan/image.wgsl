struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
}

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
    let uvws = array(
        triangle.uv_0.xyz,
        triangle.uv_1.xyz,
        triangle.uv_2.xyz,
    );

    var output: VertexOutput;
    output.position = positions[vertex_index];
    output.uvw = uvws[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let straight = textureSample(image_texture, image_sampler, input.uvw.xy / input.uvw.z);
    return vec4<f32>(straight.rgb * straight.a, straight.a);
}
