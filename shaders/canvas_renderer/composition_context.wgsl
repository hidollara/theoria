struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Both textures contain premultiplied linear-sRGB Canvas values. The shader
// temporarily converts straight RGB into the spaces selected independently
// for blending and Porter-Duff composition, then returns the result to the
// renderer working representation.

struct CompositionOperation {
    has_blend: u32,
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

// @theoria-rgb-conversions
