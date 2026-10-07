use std::error::Error;

use super::composition_shader::{self, Binding};
use crate::{canvas::BlendShader, color::ColorSpace, transfer_wgsl::TransferWgslExt};

pub(crate) trait BlendShaderExt {
    #[cfg(test)]
    fn wgsl_module(&self, binding: Binding) -> String;
    fn wgsl_module_for_spaces(
        &self,
        binding: Binding,
        working: &ColorSpace,
        blend: &ColorSpace,
        composite: &ColorSpace,
    ) -> Result<String, Box<dyn Error>>;
}

impl BlendShaderExt for BlendShader {
    #[cfg(test)]
    fn wgsl_module(&self, binding: Binding) -> String {
        composition_shader::assemble(binding, self.helpers(), self.body())
    }

    fn wgsl_module_for_spaces(
        &self,
        binding: Binding,
        working: &ColorSpace,
        blend: &ColorSpace,
        composite: &ColorSpace,
    ) -> Result<String, Box<dyn Error>> {
        let conversions = [
            (working, composite, "working_to_composite"),
            (composite, working, "composite_to_working"),
            (working, blend, "working_to_blend"),
            (blend, composite, "blend_to_composite"),
        ];
        let mut wgsl = String::new();
        for (id, (from, to, name)) in conversions.into_iter().enumerate() {
            if from.channels() != 3 || to.channels() != 3 {
                return Err(std::io::Error::other(
                    "Canvas composition requires three-channel color spaces",
                )
                .into());
            }
            if from == to {
                wgsl.push_str(&format!(
                    "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{ return value; }}\n"
                ));
                continue;
            }
            let transfer = from.transfer_to(to)?;
            if transfer.is_empty() {
                wgsl.push_str(&format!(
                    "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{ return value; }}\n"
                ));
                continue;
            }
            wgsl.push_str(&transfer.wgsl(id as u32)?);
            wgsl.push_str(&format!(
                "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{\n    let result = theoria_transfer_{id}(array<f32, 3>(value.x, value.y, value.z));\n    return vec3<f32>(result[0], result[1], result[2]);\n}}\n"
            ));
        }
        Ok(composition_shader::assemble_with_conversions(
            binding,
            self.helpers(),
            self.body(),
            &wgsl,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{Composite, Composition},
        color::{IccProfile, IccRenderingIntent, LinearSrgb, Srgb},
    };
    #[test]
    fn composition_shader_lowers_space_transfers_without_space_numbers() {
        let icc = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
        )))
        .unwrap();
        let shader = BlendShader::wgsl("return backdrop * source;").unwrap();
        let composition = Composition::new(
            Composite::SourceOver,
            icc.color_space(IccRenderingIntent::RelativeColorimetric),
        )
        .with_blend_shader(
            shader.clone(),
            crate::test_support::distinct_color_space(Srgb),
        );
        assert_eq!(
            composition.composite_space(),
            &icc.color_space(IccRenderingIntent::RelativeColorimetric)
        );
        assert!(composition.blend().is_some());
        let source = shader
            .wgsl_module_for_spaces(
                Binding::WebGpu,
                &ColorSpace::from(LinearSrgb),
                composition.blend().unwrap().space(),
                composition.composite_space(),
            )
            .unwrap();
        assert!(source.contains("theoria_transfer_0"));
        assert!(!source.contains("operation.composite_space"));
        let module = naga::front::wgsl::parse_str(&source)
            .map_err(|error| error.emit_to_string(&source))
            .unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .unwrap();
    }
}
