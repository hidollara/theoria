struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) uvw_and_association: vec4<f32>,
    @location(2) metadata: vec4<u32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) rgb_is_premultiplied: f32,
    @location(2) @interpolate(flat) transparent_extension: vec2<u32>,
    @location(3) @interpolate(flat) rgb_color_space: u32,
}

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d<f32>;

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = input.position;
    output.uvw = input.uvw_and_association.xyz;
    output.rgb_is_premultiplied = input.uvw_and_association.w;
    output.transparent_extension = input.metadata.xy;
    output.rgb_color_space = input.metadata.z;
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
    let uv = input.uvw.xy / input.uvw.z;
    if (input.transparent_extension.x != 0u && (uv.x < 0.0 || uv.x > 1.0)) ||
       (input.transparent_extension.y != 0u && (uv.y < 0.0 || uv.y > 1.0)) {
        return vec4<f32>(0.0);
    }
    let sampled = textureSample(image_texture, image_sampler, uv);
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
