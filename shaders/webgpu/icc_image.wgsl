struct VertexInput {
    @location(0) position: vec4<f32>,
    @location(1) uvw_and_association: vec4<f32>,
    @location(2) metadata: vec4<u32>,
}

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) transparent_extension: vec2<u32>,
    @location(2) @interpolate(flat) linear_sampling: u32,
}

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d_array<f32>;

// @theoria-transfer

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    output.position = input.position;
    output.uvw = input.uvw_and_association.xyz;
    output.transparent_extension = input.metadata.xy;
    output.linear_sampling = input.metadata.w;
    return output;
}

fn sample_device(uv: vec2<f32>) -> array<f32, THEORIA_SAMPLE_CHANNELS> {
// @theoria-sample-values
}

fn convert_device(sample: array<f32, THEORIA_SAMPLE_CHANNELS>) -> vec4<f32> {
    let rgb = theoria_transfer_0(array<f32, THEORIA_DEVICE_CHANNELS>(THEORIA_DEVICE_VALUES));
    let alpha = THEORIA_ALPHA;
    return vec4<f32>(rgb[0], rgb[1], rgb[2], alpha);
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let uv = input.uvw.xy / input.uvw.z;
    if (input.transparent_extension.x != 0u && (uv.x < 0.0 || uv.x > 1.0)) ||
       (input.transparent_extension.y != 0u && (uv.y < 0.0 || uv.y > 1.0)) {
        return vec4<f32>(0.0);
    }
    var straight: vec4<f32>;
    if input.linear_sampling == 0u {
        straight = convert_device(sample_device(uv));
    } else {
        let size = vec2<f32>(textureDimensions(image_texture));
        let pixel = uv * size - vec2<f32>(0.5);
        let base = floor(pixel);
        let weight = fract(pixel);
        let c00 = convert_device(sample_device((base + vec2<f32>(0.5, 0.5)) / size));
        let c10 = convert_device(sample_device((base + vec2<f32>(1.5, 0.5)) / size));
        let c01 = convert_device(sample_device((base + vec2<f32>(0.5, 1.5)) / size));
        let c11 = convert_device(sample_device((base + vec2<f32>(1.5, 1.5)) / size));
        straight = mix(mix(c00, c10, weight.x), mix(c01, c11, weight.x), weight.y);
    }
    return vec4<f32>(straight.rgb * straight.a, straight.a);
}
