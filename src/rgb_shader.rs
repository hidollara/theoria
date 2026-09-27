//! Specializes raster sampling and presentation shaders for a renderer profile.
//! Also compiled by build.rs for the default Vulkan shader validation.

use theoria_color::{
    ColorProfile, DisplayP3, IccRenderingIntent, LinearDisplayP3, LinearSrgb, Srgb,
    connect_profiles,
};

const MARKER: &str = "// @theoria-rgb-conversions";

#[allow(dead_code)]
pub(crate) fn assemble(source: &str) -> String {
    assemble_for(source, &LinearSrgb.into()).expect("default RGB transfer is representable")
}

pub(crate) fn assemble_for(
    source: &str,
    working_profile: &ColorProfile,
) -> Result<String, Box<dyn std::error::Error>> {
    if working_profile.space().channels() != 3 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "Canvas working profile must have three color channels",
        )
        .into());
    }
    assert_eq!(
        source.matches(MARKER).count(),
        1,
        "WGSL source must have exactly one RGB conversion insertion point"
    );
    let profiles: [ColorProfile; 4] = [
        Srgb.into(),
        LinearSrgb.into(),
        DisplayP3.into(),
        LinearDisplayP3.into(),
    ];
    let mut definitions = String::new();
    for (index, profile) in profiles.into_iter().enumerate() {
        let transfer = connect_profiles(
            profile,
            working_profile,
            IccRenderingIntent::RelativeColorimetric,
            IccRenderingIntent::RelativeColorimetric,
        )?;
        definitions.push_str(&transfer.transfer().wgsl(index as u32)?);
    }
    let present = connect_profiles(
        working_profile,
        LinearSrgb,
        IccRenderingIntent::RelativeColorimetric,
        IccRenderingIntent::RelativeColorimetric,
    )?;
    definitions.push_str(&present.transfer().wgsl(4)?);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_the_profile_transfers_once() {
        let shader = assemble(MARKER);
        assert!(!shader.contains(MARKER));
        assert_eq!(shader.matches("fn to_working(").count(), 1);
        assert_eq!(shader.matches("fn theoria_transfer_4(").count(), 1);
    }

    #[test]
    fn every_predefined_working_profile_validates() {
        let profiles: [ColorProfile; 4] = [
            Srgb.into(),
            LinearSrgb.into(),
            DisplayP3.into(),
            LinearDisplayP3.into(),
        ];
        for profile in profiles {
            for template in [
                include_str!("../shaders/webgpu/image.wgsl"),
                include_str!("../shaders/canvas_renderer/present.wgsl"),
            ] {
                let source = assemble_for(template, &profile).unwrap();
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

    #[test]
    fn distinct_profile_handle_can_be_used_as_working_profile() {
        let profile = crate::test_support::distinct_color_profile(Srgb);
        let source = assemble_for(include_str!("../shaders/webgpu/image.wgsl"), &profile).unwrap();
        let module = naga::front::wgsl::parse_str(&source).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
