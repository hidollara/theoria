struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Both textures contain premultiplied linear-sRGB Canvas values. The shader
// temporarily converts straight RGB into the spaces selected independently
// for blending and Porter-Duff composition, then returns the result to the
// renderer working representation.

struct CompositionOperation {
    blend_mode: u32,
    composite: u32,
    blend_space: u32,
    composite_space: u32,
}

@group(0) @binding(0) var destination_sampler: sampler;
@group(0) @binding(1) var destination_texture: texture_2d<f32>;
@group(1) @binding(0) var source_sampler: sampler;
@group(1) @binding(1) var source_texture: texture_2d<f32>;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(vec2<f32>(-1.0, -1.0), vec2<f32>(3.0, -1.0), vec2<f32>(-1.0, 3.0));
    let uvs = array(vec2<f32>(0.0, 1.0), vec2<f32>(2.0, 1.0), vec2<f32>(0.0, -1.0));
    var output: VertexOutput;
    output.position = vec4<f32>(positions[vertex_index], 0.0, 1.0);
    output.uv = uvs[vertex_index];
    return output;
}

fn straight_rgb(value: vec4<f32>) -> vec3<f32> {
    if value.a <= 0.0 {
        return vec3<f32>(0.0);
    }
    return value.rgb / value.a;
}

fn decode_srgb_component(value: f32) -> f32 {
    let magnitude = abs(value);
    if magnitude <= 0.04045 {
        return value / 12.92;
    }
    return sign(value) * pow((magnitude + 0.055) / 1.055, 2.4);
}

fn encode_srgb_component(value: f32) -> f32 {
    let magnitude = abs(value);
    if magnitude <= 0.0031308 {
        return 12.92 * value;
    }
    return sign(value) * (1.055 * pow(magnitude, 1.0 / 2.4) - 0.055);
}

fn decode_srgb(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        decode_srgb_component(value.r),
        decode_srgb_component(value.g),
        decode_srgb_component(value.b),
    );
}

fn encode_srgb(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        encode_srgb_component(value.r),
        encode_srgb_component(value.g),
        encode_srgb_component(value.b),
    );
}

fn linear_srgb_to_xyz_d65(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.4123908 * value.r + 0.35758434 * value.g + 0.18048079 * value.b,
        0.212639 * value.r + 0.71516868 * value.g + 0.07219232 * value.b,
        0.01933082 * value.r + 0.11919478 * value.g + 0.95053215 * value.b,
    );
}

fn xyz_d65_to_linear_srgb(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        3.24096994 * value.x - 1.53738318 * value.y - 0.49861076 * value.z,
        -0.96924364 * value.x + 1.8759675 * value.y + 0.04155506 * value.z,
        0.05563008 * value.x - 0.20397696 * value.y + 1.05697151 * value.z,
    );
}

fn linear_p3_to_xyz_d65(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        0.48657095 * value.r + 0.26566769 * value.g + 0.19821729 * value.b,
        0.22897456 * value.r + 0.69173852 * value.g + 0.07928691 * value.b,
        0.0 * value.r + 0.04511338 * value.g + 1.04394437 * value.b,
    );
}

fn xyz_d65_to_linear_p3(value: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(
        2.49349691 * value.x - 0.93138362 * value.y - 0.40271078 * value.z,
        -0.82948897 * value.x + 1.76266406 * value.y + 0.02362469 * value.z,
        0.03584583 * value.x - 0.07617239 * value.y + 0.95688452 * value.z,
    );
}

fn from_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == 0u {
        return encode_srgb(value);
    }
    if space == 1u {
        return value;
    }
    let p3 = xyz_d65_to_linear_p3(linear_srgb_to_xyz_d65(value));
    if space == 2u {
        return encode_srgb(p3);
    }
    return p3;
}

fn to_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == 0u {
        return decode_srgb(value);
    }
    if space == 1u {
        return value;
    }
    var p3 = value;
    if space == 2u {
        p3 = decode_srgb(value);
    }
    return xyz_d65_to_linear_srgb(linear_p3_to_xyz_d65(p3));
}
