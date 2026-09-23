struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) rgb_is_premultiplied: f32,
    @location(2) @interpolate(flat) rgb_color_space: u32,
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
    metadata: vec4<u32>,
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
    output.rgb_color_space = triangle.metadata.x;
    return output;
}

fn decode_srgb_component(value: f32) -> f32 {
    if value <= 0.04045 {
        return value / 12.92;
    }
    return pow((value + 0.055) / 1.055, 2.4);
}

fn decode_srgb(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        decode_srgb_component(value.r),
        decode_srgb_component(value.g),
        decode_srgb_component(value.b),
    );
}

fn linear_p3_to_linear_srgb(value: vec3<f32>) -> vec3<f32> {
    let x = 0.48657095 * value.r + 0.26566769 * value.g + 0.19821729 * value.b;
    let y = 0.22897456 * value.r + 0.69173852 * value.g + 0.07928691 * value.b;
    let z = 0.04511338 * value.g + 1.04394437 * value.b;
    return vec3<f32>(
        3.24096994 * x - 1.53738318 * y - 0.49861076 * z,
        -0.96924364 * x + 1.8759675 * y + 0.04155506 * z,
        0.05563008 * x - 0.20397696 * y + 1.05697151 * z,
    );
}

fn to_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    var linear = value;
    if space == 0u || space == 2u {
        linear = decode_srgb(value);
    }
    if space == 2u || space == 3u {
        return linear_p3_to_linear_srgb(linear);
    }
    return linear;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let sampled = textureSample(image_texture, image_sampler, input.uvw.xy / input.uvw.z);
    var straight_rgb = sampled.rgb;
    if input.rgb_is_premultiplied != 0.0 {
        if sampled.a <= 0.0 {
            straight_rgb = vec3<f32>(0.0);
        } else {
            straight_rgb = sampled.rgb / sampled.a;
        }
    }
    return vec4<f32>(to_working(straight_rgb, input.rgb_color_space) * sampled.a, sampled.a);
}
