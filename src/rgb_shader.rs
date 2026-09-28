//! Specializes raster sampling and presentation shaders for a renderer space.
//! Also compiled by build.rs for the default Vulkan shader validation.

use theoria_color::{ColorSpace, DisplayP3, LinearDisplayP3, LinearSrgb, Srgb, connect_spaces};

const MARKER: &str = "// @theoria-rgb-conversions";

#[allow(dead_code)]
pub(crate) fn assemble(source: &str) -> String {
    assemble_for(source, &LinearSrgb.into()).expect("default RGB transfer is representable")
}

pub(crate) fn assemble_for(
    source: &str,
    working_space: &ColorSpace,
) -> Result<String, Box<dyn std::error::Error>> {
    if working_space.channels() != 3 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Canvas working space must have three color channels",
        )
        .into());
    }
    assert_eq!(
        source.matches(MARKER).count(),
        1,
        "WGSL source must have exactly one RGB conversion insertion point"
    );
    let spaces: [ColorSpace; 4] = [
        Srgb.into(),
        LinearSrgb.into(),
        DisplayP3.into(),
        LinearDisplayP3.into(),
    ];
    let mut definitions = String::new();
    for (index, space) in spaces.into_iter().enumerate() {
        let transfer = connect_spaces(space, working_space)?;
        definitions.push_str(&transfer.wgsl(index as u32)?);
    }
    let present = connect_spaces(working_space, LinearSrgb)?;
    definitions.push_str(&present.wgsl(4)?);
    definitions.push_str(
        r#"
fn rgb_to_array(value: vec3<f32>) -> array<f32, 3> {
    return array<f32, 3>(value.r, value.g, value.b);
}

fn array_to_rgb(value: array<f32, 3>) -> vec3<f32> {
    return vec3<f32>(value[0], value[1], value[2]);
}

fn to_working(value: vec3<f32>, space: u32) -> vec3<f32> {
    if space == 4u { return value; }
    let values = rgb_to_array(value);
    if space == 0u { return array_to_rgb(theoria_transfer_0(values)); }
    if space == 1u { return array_to_rgb(theoria_transfer_1(values)); }
    if space == 2u { return array_to_rgb(theoria_transfer_2(values)); }
    return array_to_rgb(theoria_transfer_3(values));
}

fn working_to_linear_srgb(value: vec3<f32>) -> vec3<f32> {
    return array_to_rgb(theoria_transfer_4(rgb_to_array(value)));
}
"#,
    );
    Ok(source.replacen(MARKER, &definitions, 1))
}

pub(crate) fn assemble_output_for(
    source: &str,
    working_space: &ColorSpace,
    output_space: &ColorSpace,
) -> Result<String, Box<dyn std::error::Error>> {
    if working_space.channels() != 3 || output_space.channels() != 3 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Canvas output requires three-channel spaces",
        )
        .into());
    }
    assert_eq!(source.matches(MARKER).count(), 1);
    let composition_space = ColorSpace::from(LinearSrgb);
    let to_composition = connect_spaces(working_space, &composition_space)?;
    let to_output = connect_spaces(&composition_space, output_space)?;
    let definitions = format!(
        "{}{}\n\
         fn rgb_to_array(value: vec3<f32>) -> array<f32, 3> {{\n\
             return array<f32, 3>(value.r, value.g, value.b);\n\
         }}\n\
         fn array_to_rgb(value: array<f32, 3>) -> vec3<f32> {{\n\
             return vec3<f32>(value[0], value[1], value[2]);\n\
         }}\n\
         fn working_to_composition(value: vec3<f32>) -> vec3<f32> {{\n\
             return array_to_rgb(theoria_transfer_0(rgb_to_array(value)));\n\
         }}\n\
         fn composition_to_output(value: vec3<f32>) -> vec3<f32> {{\n\
             return array_to_rgb(theoria_transfer_1(rgb_to_array(value)));\n\
         }}\n",
        to_composition.wgsl(0)?,
        to_output.wgsl(1)?,
    );
    Ok(source.replacen(MARKER, &definitions, 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_the_space_transfers_once() {
        let shader = assemble(MARKER);
        assert!(!shader.contains(MARKER));
        assert_eq!(shader.matches("fn to_working(").count(), 1);
        assert_eq!(shader.matches("fn theoria_transfer_4(").count(), 1);
    }

    #[test]
    fn every_predefined_working_space_validates() {
        let spaces: [ColorSpace; 4] = [
            Srgb.into(),
            LinearSrgb.into(),
            DisplayP3.into(),
            LinearDisplayP3.into(),
        ];
        for space in spaces {
            let source =
                assemble_for(include_str!("../shaders/webgpu/image.wgsl"), &space).unwrap();
            let module = naga::front::wgsl::parse_str(&source).unwrap();
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap();
        }
    }

    #[test]
    fn distinct_space_handle_can_be_used_as_working_space() {
        let space = crate::test_support::distinct_color_space(Srgb);
        let source = assemble_for(include_str!("../shaders/webgpu/image.wgsl"), &space).unwrap();
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    fn output_shader_accepts_distinct_working_and_destination_spaces() {
        let source = assemble_output_for(
            include_str!("../shaders/canvas_renderer/output_image.wgsl"),
            &DisplayP3.into(),
            &Srgb.into(),
        )
        .unwrap();
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
