struct FragmentInput {
    @builtin(position) position: vec4<f32>,
    @location(0) uvw: vec3<f32>,
    @location(1) @interpolate(flat) unused_association: f32,
    @location(2) @interpolate(flat) flags: u32,
}

@group(0) @binding(0) var image_sampler: sampler;
@group(0) @binding(1) var image_texture: texture_2d_array<f32>;

// @theoria-transfer

fn sample_device(uv: vec2<f32>) -> array<f32, THEORIA_SAMPLE_CHANNELS> {
// @theoria-sample-values
}

fn convert_device(sample: array<f32, THEORIA_SAMPLE_CHANNELS>) -> vec4<f32> {
    let rgb = THEORIA_CONVERTED_VALUES;
    let alpha = THEORIA_ALPHA;
    return vec4<f32>(rgb[0], rgb[1], rgb[2], alpha);
}

fn sample_color(uv: vec2<f32>, flags: u32) -> vec4<f32> {
    if ((flags & 2u) != 0u && (uv.x < 0.0 || uv.x > 1.0)) ||
       ((flags & 4u) != 0u && (uv.y < 0.0 || uv.y > 1.0)) {
        return vec4<f32>(0.0);
    }
    return convert_device(sample_device(uv));
}

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let uv = input.uvw.xy / input.uvw.z;
    var straight: vec4<f32>;
    if (input.flags & 1u) == 0u {
        straight = sample_color(uv, input.flags);
    } else {
        let size = vec2<f32>(textureDimensions(image_texture));
        let pixel = uv * size - vec2<f32>(0.5);
        let base = floor(pixel);
        let weight = fract(pixel);
        let c00 = sample_color((base + vec2<f32>(0.5, 0.5)) / size, input.flags);
        let c10 = sample_color((base + vec2<f32>(1.5, 0.5)) / size, input.flags);
        let c01 = sample_color((base + vec2<f32>(0.5, 1.5)) / size, input.flags);
        let c11 = sample_color((base + vec2<f32>(1.5, 1.5)) / size, input.flags);
        straight = mix(mix(c00, c10, weight.x), mix(c01, c11, weight.x), weight.y);
    }
    return vec4<f32>(straight.rgb * straight.a, straight.a);
}
