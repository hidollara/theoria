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

__THEORIA_BLEND_OPERATION_BINDING__

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

fn hard_light_component(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb * (2.0 * cs);
    }
    return cb + (2.0 * cs - 1.0) - cb * (2.0 * cs - 1.0);
}

fn color_dodge_component(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 { return 0.0; }
    if cs >= 1.0 { return 1.0; }
    return min(1.0, cb / (1.0 - cs));
}

fn color_burn_component(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 { return 1.0; }
    if cs <= 0.0 { return 0.0; }
    return 1.0 - min(1.0, (1.0 - cb) / cs);
}

fn soft_light_component(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        return cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb);
    }
    var d = sqrt(cb);
    if cb <= 0.25 {
        d = ((16.0 * cb - 12.0) * cb + 4.0) * cb;
    }
    return cb + (2.0 * cs - 1.0) * (d - cb);
}

fn lum(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.3, 0.59, 0.11));
}

fn sat(c: vec3<f32>) -> f32 {
    return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
}

fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var result = c;
    if n < 0.0 {
        result = vec3<f32>(l) + (result - vec3<f32>(l)) * l / (l - n);
    }
    if x > 1.0 {
        result = vec3<f32>(l) + (result - vec3<f32>(l)) * (1.0 - l) / (x - l);
    }
    return result;
}

fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> {
    return clip_color(c + vec3<f32>(l - lum(c)));
}

fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let cmin = min(c.r, min(c.g, c.b));
    let cmax = max(c.r, max(c.g, c.b));
    if cmax <= cmin {
        return vec3<f32>(0.0);
    }
    return (c - vec3<f32>(cmin)) * (s / (cmax - cmin));
}

fn blend_rgb(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    var result = cs;
    if operation.blend_mode == 1u {
        result = cb * cs;
    } else if operation.blend_mode == 2u {
        result = 1.0 - (1.0 - cb) * (1.0 - cs);
    } else if operation.blend_mode == 3u {
        result = vec3<f32>(
            hard_light_component(cs.r, cb.r),
            hard_light_component(cs.g, cb.g),
            hard_light_component(cs.b, cb.b),
        );
    } else if operation.blend_mode == 4u {
        result = min(cb, cs);
    } else if operation.blend_mode == 5u {
        result = max(cb, cs);
    } else if operation.blend_mode == 6u {
        result = vec3<f32>(
            color_dodge_component(cb.r, cs.r),
            color_dodge_component(cb.g, cs.g),
            color_dodge_component(cb.b, cs.b),
        );
    } else if operation.blend_mode == 7u {
        result = vec3<f32>(
            color_burn_component(cb.r, cs.r),
            color_burn_component(cb.g, cs.g),
            color_burn_component(cb.b, cs.b),
        );
    } else if operation.blend_mode == 8u {
        result = vec3<f32>(
            hard_light_component(cb.r, cs.r),
            hard_light_component(cb.g, cs.g),
            hard_light_component(cb.b, cs.b),
        );
    } else if operation.blend_mode == 9u {
        result = vec3<f32>(
            soft_light_component(cb.r, cs.r),
            soft_light_component(cb.g, cs.g),
            soft_light_component(cb.b, cs.b),
        );
    } else if operation.blend_mode == 10u {
        result = abs(cb - cs);
    } else if operation.blend_mode == 11u {
        result = cb + cs - 2.0 * cb * cs;
    } else if operation.blend_mode == 12u {
        result = set_lum(set_sat(cs, sat(cb)), lum(cb));
    } else if operation.blend_mode == 13u {
        result = set_lum(set_sat(cb, sat(cs)), lum(cb));
    } else if operation.blend_mode == 14u {
        result = set_lum(cs, lum(cb));
    } else if operation.blend_mode == 15u {
        result = set_lum(cb, lum(cs));
    }
    return clamp(result, vec3<f32>(0.0), vec3<f32>(1.0));
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let d = textureSample(destination_texture, destination_sampler, input.uv);
    let s = textureSample(source_texture, source_sampler, input.uv);
    let destination_working = straight_rgb(d);
    let source_working = straight_rgb(s);

    var blended_working = source_working;
    if operation.blend_mode != 0u {
        let destination_blend = from_working(destination_working, operation.blend_space);
        let source_blend = from_working(source_working, operation.blend_space);
        let blended = (1.0 - d.a) * source_blend
            + d.a * blend_rgb(destination_blend, source_blend);
        blended_working = to_working(blended, operation.blend_space);
    }

    let source_composite = from_working(blended_working, operation.composite_space) * s.a;
    let destination_composite = from_working(destination_working, operation.composite_space) * d.a;

    var fa = 1.0;
    var fb = 1.0 - s.a;
    if operation.composite == 0u { fa = 0.0; fb = 0.0; }
    else if operation.composite == 1u { fa = 1.0; fb = 0.0; }
    else if operation.composite == 2u { fa = 0.0; fb = 1.0; }
    else if operation.composite == 4u { fa = 1.0 - d.a; fb = 1.0; }
    else if operation.composite == 5u { fa = d.a; fb = 0.0; }
    else if operation.composite == 6u { fa = 0.0; fb = s.a; }
    else if operation.composite == 7u { fa = 1.0 - d.a; fb = 0.0; }
    else if operation.composite == 8u { fa = 0.0; fb = 1.0 - s.a; }
    else if operation.composite == 9u { fa = d.a; fb = 1.0 - s.a; }
    else if operation.composite == 10u { fa = 1.0 - d.a; fb = s.a; }
    else if operation.composite == 11u { fa = 1.0 - d.a; fb = 1.0 - s.a; }

    let output_alpha = fa * s.a + fb * d.a;
    let output_composite = fa * source_composite + fb * destination_composite;
    if output_alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let output_working = to_working(output_composite / output_alpha, operation.composite_space);
    return vec4<f32>(output_working * output_alpha, output_alpha);
}
