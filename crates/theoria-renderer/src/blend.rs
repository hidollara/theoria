use std::error::Error;

use theoria_canvas::canvas::BlendShader;
use theoria_color::ColorSpace;

use crate::{composition_shader, transfer_wgsl::TransferWgslExt};

pub trait BlendShaderExt {
    /// Validates the pure RGB operation and assembles a validated composition
    /// module. Definition errors are reported here before backend compilation.
    fn wgsl_module_for_spaces(
        &self,
        binding: &str,
        working: &ColorSpace,
        blend: &ColorSpace,
        composite: &ColorSpace,
    ) -> Result<String, Box<dyn Error>>;
}

impl BlendShaderExt for BlendShader {
    fn wgsl_module_for_spaces(
        &self,
        binding: &str,
        working: &ColorSpace,
        blend: &ColorSpace,
        composite: &ColorSpace,
    ) -> Result<String, Box<dyn Error>> {
        validate_blend(self)?;
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
        let source = composition_shader::assemble_with_conversions(
            binding,
            self.helpers(),
            self.body(),
            &wgsl,
        );
        validate_module(&parse_module(&source)?)?;
        Ok(source)
    }
}

// Validate the operation in isolation so it cannot depend on renderer bindings
// or functions that happen to exist in the assembled composition module.
fn validate_blend(shader: &BlendShader) -> Result<(), Box<dyn Error>> {
    let helpers = parse_module(shader.helpers())?;
    if !helpers.global_variables.is_empty() || !helpers.entry_points.is_empty() {
        return Err(std::io::Error::other(
            "blend helpers must not declare entry points or global resources",
        )
        .into());
    }
    let source = composition_shader::operation(shader.helpers(), shader.body());
    let validation = format!(
        "{source}\n@fragment fn theoria_validate() -> @location(0) vec4<f32> {{\n    return vec4<f32>(theoria_blend(vec3<f32>(0.0), vec3<f32>(0.0)), 1.0);\n}}"
    );
    let module = parse_module(&validation)?;
    if !module.global_variables.is_empty()
        || module.entry_points.len() != 1
        || module.functions.len() != helpers.functions.len() + 1
    {
        return Err(std::io::Error::other(
            "blend body must not declare module-level functions, entry points, or global resources",
        )
        .into());
    }
    validate_module(&module)
}

fn parse_module(source: &str) -> Result<naga::Module, Box<dyn Error>> {
    naga::front::wgsl::parse_str(source)
        .map_err(|error| std::io::Error::other(error.emit_to_string(source)).into())
}

fn validate_module(module: &naga::Module) -> Result<(), Box<dyn Error>> {
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(module)
    .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use theoria_canvas::canvas::{Blend, BlendMode, Composite, Composition, WgslBlend};
    use theoria_color::{LinearSrgb, Srgb};

    fn lower(shader: &BlendShader, binding: &str) -> Result<String, Box<dyn Error>> {
        let space = ColorSpace::from(Srgb);
        shader.wgsl_module_for_spaces(binding, &space, &space, &space)
    }

    const WEBGPU_BINDING: &str =
        "@group(2) @binding(0) var<uniform> operation: CompositionOperation;";
    const VULKAN_BINDING: &str = "var<immediate> operation: CompositionOperation;";

    #[test]
    fn rendering_rejects_invalid_blend_definitions() {
        let invalid = [
            WgslBlend::new("return missing(source);"),
            WgslBlend::new("return 1.0;"),
            WgslBlend::new("return vec4<f32>(source, 1.0);"),
            WgslBlend::new("return source"),
            WgslBlend::new(
                "return source; } fn leaked() {} fn trailing(backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> { return source;",
            ),
            WgslBlend::new("return source;").with_helpers("var<private> hidden: f32;"),
            WgslBlend::new("return source;").with_helpers(
                "@fragment fn unrelated() -> @location(0) vec4<f32> { return vec4<f32>(0.0); }",
            ),
            // Even a legal renderer binding is unavailable to a pure blend.
            WgslBlend::new("return vec3<f32>(f32(operation.composite));"),
        ];
        for definition in invalid {
            let shader = BlendShader::wgsl(definition);
            for binding in [WEBGPU_BINDING, VULKAN_BINDING] {
                assert!(lower(&shader, binding).is_err(), "{shader:?}");
            }
        }
    }

    #[test]
    fn rendering_accepts_helpers_and_all_css_modes() {
        let custom = [
            WgslBlend::new("return backdrop * source;"),
            WgslBlend::new("return user_rgb(vec4<f32>(source, 1.0));")
                .with_helpers("fn user_rgb(a: vec4<f32>) -> vec3<f32> { return a.rgb; }"),
            WgslBlend::new("return vec3<f32>(hard_light_component(backdrop.r, source.r));")
                .with_helpers("fn hard_light_component(a: f32, b: f32) -> f32 { return a * b; }"),
        ];
        for shader in custom.into_iter().map(BlendShader::wgsl).chain(
            BlendMode::ALL
                .into_iter()
                .map(|mode| Blend::new(mode, Srgb).shader().clone()),
        ) {
            for binding in [WEBGPU_BINDING, VULKAN_BINDING] {
                lower(&shader, binding).unwrap();
            }
        }
    }

    #[test]
    fn rendering_rejects_collisions_with_the_composition_module() {
        let shader = BlendShader::wgsl(WgslBlend::new("return source;").with_helpers(
            "fn working_to_composite(value: vec3<f32>) -> vec3<f32> { return value; }",
        ));
        // Valid by itself, but not when assembled with renderer definitions.
        validate_blend(&shader).unwrap();
        for binding in [WEBGPU_BINDING, VULKAN_BINDING] {
            assert!(lower(&shader, binding).is_err());
        }
    }

    #[test]
    fn composition_shader_lowers_space_transfers_without_space_numbers() {
        let space = crate::test_support::distinct_color_space(Srgb);
        let shader = BlendShader::wgsl("return backdrop * source;");
        let composition = Composition::new(Composite::SourceOver, space.clone()).with_blend_shader(
            shader.clone(),
            crate::test_support::distinct_color_space(Srgb),
        );
        assert_eq!(composition.composite_space(), &space);
        assert!(composition.blend().is_some());
        let source = shader
            .wgsl_module_for_spaces(
                "@group(2) @binding(0) var<uniform> operation: CompositionOperation;",
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
