use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use crate::color::{ColorProfile, IccRenderingIntent, Rgb, TypedColorSpace, connect_profiles};

use super::composition_shader::{self, Binding};

/// The straight-RGB operation evaluated where source and backdrop overlap.
/// Alpha and Porter-Duff composition are separate from this operation.
///
/// Implement this for a specific typed space to restrict where an operation may
/// be used. The resulting shader is recorded in the Canvas and compiled by
/// each renderer when first needed.
pub trait BlendOperation<P: TypedColorSpace<Model = Rgb>> {
    fn shader(self) -> BlendShader;

    /// Whether this operation returns the source unchanged for every input.
    fn is_identity(&self) -> bool {
        false
    }
}

/// The WGSL body of a blend operation and its optional helper definitions.
/// The `backdrop` and `source` arguments and the generated function name are supplied
/// by the shader assembler, not by this definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct WgslBlend {
    helpers: Arc<str>,
    body: Arc<str>,
}

impl WgslBlend {
    pub fn new(body: impl Into<Arc<str>>) -> Self {
        Self {
            helpers: Arc::from(""),
            body: body.into(),
        }
    }

    /// Adds module-level helper functions used by the body.
    pub fn with_helpers(mut self, helpers: impl Into<Arc<str>>) -> Self {
        self.helpers = helpers.into();
        self
    }
}

impl From<&str> for WgslBlend {
    fn from(body: &str) -> Self {
        Self::new(body)
    }
}

/// A validated, immutable WGSL blend program definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlendShader(WgslBlend);

impl BlendShader {
    /// Defines the body of a pure RGB operation with `backdrop` and `source`
    /// arguments. Helpers may be supplied separately. Neither the body nor
    /// helpers may declare entry points or global resources.
    pub fn wgsl(definition: impl Into<WgslBlend>) -> Result<Self, BlendShaderError> {
        let definition = definition.into();
        let helpers = naga::front::wgsl::parse_str(&definition.helpers)
            .map_err(|error| BlendShaderError(error.emit_to_string(&definition.helpers)))?;
        if !helpers.global_variables.is_empty() || !helpers.entry_points.is_empty() {
            return Err(BlendShaderError(
                "blend helpers must not declare entry points or global resources".into(),
            ));
        }
        let source = composition_shader::operation(&definition.helpers, &definition.body);
        let validation = format!(
            "{source}\n@fragment fn theoria_validate() -> @location(0) vec4<f32> {{\n    return vec4<f32>(theoria_blend(vec3<f32>(0.0), vec3<f32>(0.0)), 1.0);\n}}"
        );
        let module = naga::front::wgsl::parse_str(&validation)
            .map_err(|error| BlendShaderError(error.emit_to_string(&validation)))?;
        if !module.global_variables.is_empty()
            || module.entry_points.len() != 1
            || module.functions.len() != helpers.functions.len() + 1
        {
            return Err(BlendShaderError(
                "blend body must not declare module-level functions, entry points, or global resources".into(),
            ));
        }
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| BlendShaderError(error.to_string()))?;
        let shader = Self(definition);
        let complete = shader.wgsl_module(Binding::WebGpu);
        let module = naga::front::wgsl::parse_str(&complete)
            .map_err(|error| BlendShaderError(error.emit_to_string(&complete)))?;
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| BlendShaderError(error.to_string()))?;
        Ok(shader)
    }

    pub(crate) fn identity() -> &'static Self {
        static IDENTITY: OnceLock<BlendShader> = OnceLock::new();
        IDENTITY.get_or_init(|| Self(WgslBlend::new(composition_shader::IDENTITY_BODY)))
    }

    pub(crate) fn wgsl_module(&self, binding: Binding) -> String {
        composition_shader::assemble(binding, &self.0.helpers, &self.0.body)
    }

    pub(crate) fn wgsl_module_for_profiles(
        &self,
        binding: Binding,
        working: &ColorProfile,
        blend: &ColorProfile,
        composite: &ColorProfile,
    ) -> Result<String, Box<dyn Error>> {
        let conversions = [
            (working, composite, "working_to_composite"),
            (composite, working, "composite_to_working"),
            (working, blend, "working_to_blend"),
            (blend, composite, "blend_to_composite"),
        ];
        let mut wgsl = String::new();
        for (id, (from, to, name)) in conversions.into_iter().enumerate() {
            if from.space().channels() != 3 || to.space().channels() != 3 {
                return Err(std::io::Error::other(
                    "Canvas composition requires three-channel color profiles",
                )
                .into());
            }
            if from == to {
                wgsl.push_str(&format!(
                    "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{ return value; }}\n"
                ));
                continue;
            }
            let transform = connect_profiles(
                from,
                to,
                IccRenderingIntent::RelativeColorimetric,
                IccRenderingIntent::RelativeColorimetric,
            )?;
            wgsl.push_str(&transform.transfer().wgsl(id as u32)?);
            wgsl.push_str(&format!(
                "\nfn {name}(value: vec3<f32>) -> vec3<f32> {{\n    let result = theoria_transfer_{id}(array<f32, 3>(value.x, value.y, value.z));\n    return vec3<f32>(result[0], result[1], result[2]);\n}}\n"
            ));
        }
        Ok(composition_shader::assemble_with_conversions(
            binding,
            &self.0.helpers,
            &self.0.body,
            &wgsl,
        ))
    }
}

/// An invalid WGSL blend definition.
#[derive(Debug)]
pub struct BlendShaderError(String);

impl fmt::Display for BlendShaderError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl Error for BlendShaderError {}

/// W3C Compositing and Blending Level 1 RGB blend functions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BlendMode {
    #[default]
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl BlendMode {
    pub const ALL: [Self; 16] = [
        Self::Normal,
        Self::Multiply,
        Self::Screen,
        Self::Overlay,
        Self::Darken,
        Self::Lighten,
        Self::ColorDodge,
        Self::ColorBurn,
        Self::HardLight,
        Self::SoftLight,
        Self::Difference,
        Self::Exclusion,
        Self::Hue,
        Self::Saturation,
        Self::Color,
        Self::Luminosity,
    ];

    fn shader_definition(self) -> BlendShader {
        static SHADERS: [OnceLock<BlendShader>; 16] = [const { OnceLock::new() }; 16];
        SHADERS[self as usize]
            .get_or_init(|| {
                let expression = match self {
                    Self::Normal => "cs",
                    Self::Multiply => "cb * cs",
                    Self::Screen => "1.0 - (1.0 - cb) * (1.0 - cs)",
                    Self::Overlay => "vec3<f32>(hard_light_component(cs.r, cb.r), hard_light_component(cs.g, cb.g), hard_light_component(cs.b, cb.b))",
                    Self::Darken => "min(cb, cs)",
                    Self::Lighten => "max(cb, cs)",
                    Self::ColorDodge => "vec3<f32>(color_dodge_component(cb.r, cs.r), color_dodge_component(cb.g, cs.g), color_dodge_component(cb.b, cs.b))",
                    Self::ColorBurn => "vec3<f32>(color_burn_component(cb.r, cs.r), color_burn_component(cb.g, cs.g), color_burn_component(cb.b, cs.b))",
                    Self::HardLight => "vec3<f32>(hard_light_component(cb.r, cs.r), hard_light_component(cb.g, cs.g), hard_light_component(cb.b, cs.b))",
                    Self::SoftLight => "vec3<f32>(soft_light_component(cb.r, cs.r), soft_light_component(cb.g, cs.g), soft_light_component(cb.b, cs.b))",
                    Self::Difference => "abs(cb - cs)",
                    Self::Exclusion => "cb + cs - 2.0 * cb * cs",
                    Self::Hue => "set_lum(set_sat(cs, sat(cb)), lum(cb))",
                    Self::Saturation => "set_lum(set_sat(cb, sat(cs)), lum(cb))",
                    Self::Color => "set_lum(cs, lum(cb))",
                    Self::Luminosity => "set_lum(cb, lum(cs))",
                };
                let body = format!(
                    "let cb = backdrop; let cs = source; return clamp({expression}, vec3<f32>(0.0), vec3<f32>(1.0));"
                );
                let definition = WgslBlend::new(body)
                    .with_helpers(include_str!("../../shaders/canvas_renderer/css_blend.wgsl"));
                BlendShader::wgsl(definition)
                    .expect("the static CSS blend shader must satisfy its WGSL contract")
            })
            .clone()
    }
}

impl<P: TypedColorSpace<Model = Rgb>> BlendOperation<P> for BlendMode {
    fn shader(self) -> BlendShader {
        self.shader_definition()
    }

    fn is_identity(&self) -> bool {
        *self == Self::Normal
    }
}

impl<P: TypedColorSpace<Model = Rgb>> BlendOperation<P> for BlendShader {
    fn shader(self) -> BlendShader {
        self
    }
}

/// A blend operation and the profile in which its three components are evaluated.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    shader: BlendShader,
    space: ColorProfile,
}

impl Blend {
    pub fn new<P, O>(operation: O, space: P) -> Self
    where
        P: TypedColorSpace<Model = Rgb> + Into<ColorProfile>,
        O: BlendOperation<P>,
    {
        Self {
            shader: operation.shader(),
            space: space.into(),
        }
    }

    pub fn shader(&self) -> &BlendShader {
        &self.shader
    }

    /// Evaluates a custom blend shader in a profile-backed three-channel space.
    pub fn from_shader(shader: BlendShader, space: impl Into<ColorProfile>) -> Self {
        Self {
            shader,
            space: space.into(),
        }
    }

    pub fn space(&self) -> &ColorProfile {
        &self.space
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{Composite, Composition},
        color::{DisplayP3, IccProfile, LinearSrgb, Srgb},
    };

    #[test]
    fn wgsl_blend_validates_its_pure_rgb_contract() {
        let shader = BlendShader::wgsl("return backdrop * source;").unwrap();
        let composition =
            Composition::new(Composite::SourceOver, Srgb).with_blend(shader.clone(), Srgb);
        assert_eq!(composition.blend().unwrap().shader(), &shader);
        assert!(BlendShader::wgsl("return missing(source);").is_err());
        assert!(BlendShader::wgsl(
            "return source; } fn leaked() {} fn trailing(backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> { return source;"
        ).is_err());
        assert!(
            BlendShader::wgsl(
                WgslBlend::new("return source;").with_helpers("var<private> hidden: f32;")
            )
            .is_err()
        );
        assert!(
            BlendShader::wgsl(
                WgslBlend::new("return source;")
                    .with_helpers("fn straight_rgb(a: vec4<f32>) -> vec3<f32> { return a.rgb; }")
            )
            .is_err()
        );
        assert!(
            BlendShader::wgsl(
                WgslBlend::new("return vec3<f32>(hard_light_component(backdrop.r, source.r));")
                    .with_helpers(
                        "fn hard_light_component(a: f32, b: f32) -> f32 { return a * b; }"
                    )
            )
            .is_ok()
        );
    }

    #[test]
    fn operations_can_be_restricted_to_a_profile() {
        struct SrgbOnly;
        impl BlendOperation<Srgb> for SrgbOnly {
            fn shader(self) -> BlendShader {
                BlendMode::Screen.shader_definition()
            }
        }

        let blend = Blend::new(SrgbOnly, Srgb);
        assert_eq!(blend.space(), &ColorProfile::from(Srgb));
        assert_eq!(blend.shader(), &BlendMode::Screen.shader_definition());
    }

    #[test]
    fn css_modes_and_custom_operations_use_the_same_shader_definition() {
        let custom = BlendShader::wgsl("return backdrop * source;").unwrap();
        let custom_composition =
            Composition::new(Composite::SourceOver, Srgb).with_blend(custom, Srgb);
        assert!(custom_composition.blend().is_some());

        let mut definitions = std::collections::HashSet::new();
        for mode in BlendMode::ALL {
            let shader = mode.shader_definition();
            assert!(definitions.insert(shader.clone()));
            let composition = Composition::new(Composite::SourceOver, Srgb).with_blend(mode, Srgb);
            if mode == BlendMode::Normal {
                assert!(composition.blend().is_none());
            } else {
                assert_eq!(composition.blend().unwrap().shader(), &shader);
            }
        }
        assert_eq!(definitions.len(), BlendMode::ALL.len());
    }

    #[test]
    fn evaluation_space_does_not_change_the_blend_program() {
        let srgb = Blend::new(BlendMode::Multiply, Srgb);
        let p3 = Blend::new(BlendMode::Multiply, DisplayP3);
        assert_ne!(srgb.space(), p3.space());
        assert_eq!(srgb.shader(), p3.shader());
    }

    #[test]
    fn composition_shader_lowers_profile_transfers_without_space_numbers() {
        let icc = IccProfile::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/crates/theoria-color/src/color/fixtures/sRGB-v4.icc"
        )))
        .unwrap();
        let shader = BlendShader::wgsl("return backdrop * source;").unwrap();
        let composition = Composition::new(Composite::SourceOver, icc.profile()).with_blend_shader(
            shader.clone(),
            crate::test_support::distinct_color_profile(Srgb),
        );
        assert_eq!(composition.composite_space(), &icc.profile());
        assert!(composition.blend().is_some());
        let source = shader
            .wgsl_module_for_profiles(
                Binding::WebGpu,
                &ColorProfile::from(LinearSrgb),
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
