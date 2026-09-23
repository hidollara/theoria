//! Supplies the same RGB conversion definitions to every Canvas WGSL shader.
//! Also compiled by build.rs when validating and compiling Vulkan shaders.

use crate::color::profile_data as data;

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
    result.push_str(&matrix_function(
        "linear_srgb_to_xyz_d65",
        data::LINEAR_SRGB_TO_XYZ_D65,
    ));
    result.push_str(&matrix_function(
        "xyz_d65_to_linear_srgb",
        data::XYZ_D65_TO_LINEAR_SRGB,
    ));
    result.push_str(&matrix_function(
        "linear_p3_to_xyz_d65",
        data::LINEAR_P3_TO_XYZ_D65,
    ));
    result.push_str(&matrix_function(
        "xyz_d65_to_linear_p3",
        data::XYZ_D65_TO_LINEAR_P3,
    ));
    result.push_str(
        r#"
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
"#,
    );
    result
}

fn matrix_function(name: &str, matrix: [[f64; 3]; 3]) -> String {
    let row = |components: [f64; 3]| {
        format!(
            "{} * value.x + {} * value.y + {} * value.z",
            literal(components[0] as f32),
            literal(components[1] as f32),
            literal(components[2] as f32),
        )
    };
    format!(
        "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{\n    return vec3<f32>(\n        {},\n        {},\n        {},\n    );\n}}\n",
        row(matrix[0]),
        row(matrix[1]),
        row(matrix[2]),
    )
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
        assert_eq!(shader.matches("fn linear_p3_to_xyz_d65(").count(), 1);
    }
}
