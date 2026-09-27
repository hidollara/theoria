//! Supplies the same RGB conversion definitions to every Canvas WGSL shader.
//! Also compiled by build.rs when validating and compiling Vulkan shaders.

use theoria_color::{ColorTransform, RgbColorSpace, Srgb, Transfer};

const MARKER: &str = "// @theoria-rgb-conversions";

// The build script includes this module and uses the default specialization.
#[allow(dead_code)]
pub(crate) fn assemble(source: &str) -> String {
    assemble_for(source, 1)
}

pub(crate) fn assemble_for(source: &str, working_space: u32) -> String {
    assert!(working_space < 4, "unknown RGB working space");
    assert_eq!(
        source.matches(MARKER).count(),
        1,
        "WGSL source must have exactly one RGB conversion insertion point"
    );
    source.replacen(MARKER, &conversion_functions(working_space), 1)
}

fn conversion_functions(working_space: u32) -> String {
    let mut result = format!(
        r#"
const THEORIA_WORKING_SPACE: u32 = {working_space}u;

fn decode_srgb_component(value: f32) -> f32 {{
    let magnitude = abs(value);
    if magnitude <= {decode_threshold} {{
        return value / {linear_scale};
    }}
    return sign(value) * pow((magnitude + {power_offset}) / {power_scale}, {gamma});
}}

fn encode_srgb_component(value: f32) -> f32 {{
    let magnitude = abs(value);
    if magnitude <= {encode_threshold} {{
        return {linear_scale} * value;
    }}
    return sign(value) * ({power_scale} * pow(magnitude, 1.0 / {gamma}) - {power_offset});
}}

fn decode_srgb(value: vec3<f32>) -> vec3<f32> {{
    return vec3<f32>(
        decode_srgb_component(value.r),
        decode_srgb_component(value.g),
        decode_srgb_component(value.b),
    );
}}

fn encode_srgb(value: vec3<f32>) -> vec3<f32> {{
    return vec3<f32>(
        encode_srgb_component(value.r),
        encode_srgb_component(value.g),
        encode_srgb_component(value.b),
    );
}}
"#,
        decode_threshold = literal(Srgb::DECODE_THRESHOLD as f32),
        encode_threshold = literal(Srgb::ENCODE_THRESHOLD as f32),
        linear_scale = literal(Srgb::LINEAR_SCALE as f32),
        power_offset = literal(Srgb::POWER_OFFSET as f32),
        power_scale = literal(Srgb::POWER_SCALE as f32),
        gamma = literal(Srgb::GAMMA as f32),
    );
    for (id, (encoded, to_working)) in [(true, true), (false, true), (true, false), (false, false)]
        .into_iter()
        .enumerate()
    {
        result.push_str(
            &p3_transfer(encoded, to_working)
                .wgsl(id as u32)
                .expect("built-in RGB profile is representable in WGSL"),
        );
    }
    result.push_str(
        r#"
fn rgb_to_array(value: vec3<f32>) -> array<f32, 3> {
    return array<f32, 3>(value.r, value.g, value.b);
}

fn array_to_rgb(value: array<f32, 3>) -> vec3<f32> {
    return vec3<f32>(value[0], value[1], value[2]);
}

fn from_linear_srgb(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == 0u {
        return encode_srgb(value);
    }
    if space == 1u {
        return value;
    }
    if space == 2u {
        return array_to_rgb(theoria_transfer_2(rgb_to_array(value)));
    }
    return array_to_rgb(theoria_transfer_3(rgb_to_array(value)));
}

fn to_linear_srgb(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == 0u {
        return decode_srgb(value);
    }
    if space == 1u {
        return value;
    }
    if space == 2u {
        return array_to_rgb(theoria_transfer_0(rgb_to_array(value)));
    }
    return array_to_rgb(theoria_transfer_1(rgb_to_array(value)));
}

fn working_to_linear_srgb(value: vec3<f32>) -> vec3<f32> {
    return to_linear_srgb(value, THEORIA_WORKING_SPACE);
}

fn linear_srgb_to_working(value: vec3<f32>) -> vec3<f32> {
    return from_linear_srgb(value, THEORIA_WORKING_SPACE);
}

fn from_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == THEORIA_WORKING_SPACE {
        return value;
    }
    return from_linear_srgb(working_to_linear_srgb(value), space);
}

fn to_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == THEORIA_WORKING_SPACE {
        return value;
    }
    return linear_srgb_to_working(to_linear_srgb(value, space));
}
"#,
    );
    result
}

fn p3_transfer(encoded: bool, to_working: bool) -> Transfer {
    let p3 = if encoded {
        RgbColorSpace::DisplayP3
    } else {
        RgbColorSpace::LinearDisplayP3
    };
    let working = RgbColorSpace::LinearSrgb;
    let (from, to) = if to_working {
        (p3, working)
    } else {
        (working, p3)
    };
    ColorTransform::between_rgb(from, to)
        .expect("built-in RGB profile transfer is valid")
        .into_transfer()
}

fn literal(value: f32) -> String {
    format!("{value:?}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_the_shared_conversions_once() {
        let shader = assemble(MARKER);
        assert!(!shader.contains(MARKER));
        assert_eq!(shader.matches("fn to_working(").count(), 1);
        assert_eq!(shader.matches("fn theoria_transfer_0(").count(), 1);
        assert_eq!(shader.matches("fn theoria_transfer_3(").count(), 1);
    }

    #[test]
    fn every_working_space_validates_for_sampling_and_presentation() {
        for working_space in 0..4 {
            for template in [
                include_str!("../shaders/webgpu/image.wgsl"),
                include_str!("../shaders/canvas_renderer/present.wgsl"),
            ] {
                let source = assemble_for(template, working_space);
                let module = naga::front::wgsl::parse_str(&source).unwrap();
                naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                )
                .validate(&module)
                .unwrap();
            }
        }
    }
}
