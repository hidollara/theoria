//! Supplies the same RGB conversion definitions to every Canvas WGSL shader.
//! Also compiled by build.rs when validating and compiling Vulkan shaders.

use crate::color::{RgbTransfer, RgbTransform, Transfer, profile_data as data};

const MARKER: &str = "// @theoria-rgb-conversions";

pub(crate) fn assemble(source: &str) -> String {
    assert_eq!(
        source.matches(MARKER).count(),
        1,
        "WGSL source must have exactly one RGB conversion insertion point"
    );
    source.replacen(MARKER, &conversion_functions(), 1)
}

fn conversion_functions() -> String {
    let mut result = format!(
        r#"
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
        decode_threshold = literal(data::SRGB_DECODE_THRESHOLD as f32),
        encode_threshold = literal(data::SRGB_ENCODE_THRESHOLD as f32),
        linear_scale = literal(data::SRGB_LINEAR_SCALE as f32),
        power_offset = literal(data::SRGB_POWER_OFFSET as f32),
        power_scale = literal(data::SRGB_POWER_SCALE as f32),
        gamma = literal(data::SRGB_GAMMA as f32),
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

fn from_working(value: vec3<f32>, space: u32) -> vec3<f32> {
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

fn to_working(value: vec3<f32>, space: u32) -> vec3<f32> {
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
"#,
    );
    result
}

fn p3_transfer(encoded: bool, to_working: bool) -> Transfer {
    let p3 = RgbTransform::new(
        if encoded {
            RgbTransfer::Srgb
        } else {
            RgbTransfer::Linear
        },
        data::LINEAR_P3_TO_XYZ_D65,
        data::XYZ_D65_TO_LINEAR_P3,
    );
    let working = RgbTransform::new(
        RgbTransfer::Linear,
        data::LINEAR_SRGB_TO_XYZ_D65,
        data::XYZ_D65_TO_LINEAR_SRGB,
    );
    if to_working {
        p3.to_xyz_d65_transfer()
            .and_then(|transfer| transfer.then(&working.xyz_d65_to_rgb_transfer()?))
    } else {
        working
            .to_xyz_d65_transfer()
            .and_then(|transfer| transfer.then(&p3.xyz_d65_to_rgb_transfer()?))
    }
    .expect("built-in RGB profile transfer is valid")
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
}
