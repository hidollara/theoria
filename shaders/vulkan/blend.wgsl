struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Both textures contain premultiplied linear-sRGB Canvas values. Blend modes
// temporarily recover straight RGB; the result is associated again before the
// Porter-Duff factors combine it with the destination.

struct BlendComposite {
    blend_mode: u32,
    composite: u32,
}

var<immediate> operation: BlendComposite;

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
    let blended_source = s.a * ((1.0 - d.a) * straight_rgb(s) + d.a * blend_rgb(straight_rgb(d), straight_rgb(s)));

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

    if operation.composite == 12u {
        return vec4<f32>(blended_source + d.rgb, s.a + d.a);
    }
    return vec4<f32>(fa * blended_source + fb * d.rgb, fa * s.a + fb * d.a);
}
