@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    let d = textureSample(destination_texture, destination_sampler, input.uv);
    let s = textureSample(source_texture, source_sampler, input.uv);
    let destination_working = straight_rgb(d);
    let source_working = straight_rgb(s);

    let source_composite = working_to_composite(source_working);
    let destination_composite = working_to_composite(destination_working);
    var blended_composite = source_composite;
    if operation.has_blend != 0u {
        let destination_blend = working_to_blend(destination_working);
        let source_blend = working_to_blend(source_working);
        let blended = theoria_blend(destination_blend, source_blend);
        blended_composite = blend_to_composite(blended);
    }

    // Blend operates on straight RGB. Alpha weighting belongs to composition,
    // so source and blend results must be in the same composition space first.
    let effective_source = (1.0 - d.a) * source_composite + d.a * blended_composite;
    let source_premultiplied = effective_source * s.a;
    let destination_premultiplied = destination_composite * d.a;

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
    let output_composite = fa * source_premultiplied + fb * destination_premultiplied;
    if output_alpha <= 0.0 {
        return vec4<f32>(0.0);
    }
    let output_working = composite_to_working(output_composite / output_alpha);
    return vec4<f32>(output_working * output_alpha, output_alpha);
}
