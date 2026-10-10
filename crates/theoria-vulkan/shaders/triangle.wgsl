struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
}

// Vertex colors have already been lowered to premultiplied linear sRGB.

// WGSL immediate data maps to Vulkan push constants. Arrays are not allowed in
// the immediate address space, so the three vertices are laid out explicitly.
struct Triangle {
    position_0: vec4<f32>,
    color_0: vec4<f32>,
    position_1: vec4<f32>,
    color_1: vec4<f32>,
    position_2: vec4<f32>,
    color_2: vec4<f32>,
}

var<immediate> triangle: Triangle;

@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    let positions = array(
        triangle.position_0,
        triangle.position_1,
        triangle.position_2,
    );
    let colors = array(
        triangle.color_0,
        triangle.color_1,
        triangle.color_2,
    );

    var output: VertexOutput;
    output.position = positions[vertex_index];
    output.color = colors[vertex_index];
    return output;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    return input.color;
}
