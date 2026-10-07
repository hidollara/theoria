use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use crate::color::{ColorSpace, Rgb, TypedColorSpace};

/// The straight-RGB operation evaluated where source and backdrop overlap.
/// Alpha and Porter-Duff composition are separate from this operation.
///
/// Implement this for a specific typed space to restrict where an operation may
/// be used. The resulting shader is recorded in the Canvas and compiled by
/// each renderer when first needed.
pub trait BlendOperation<S: TypedColorSpace<Model = Rgb>> {
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
        let source = operation(&definition.helpers, &definition.body);
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
        Ok(Self(definition))
    }

    pub(crate) fn identity() -> &'static Self {
        static IDENTITY: OnceLock<BlendShader> = OnceLock::new();
        IDENTITY.get_or_init(|| Self(WgslBlend::new("return source;")))
    }

    /// Module-level helper definitions for the blend operation.
    pub fn helpers(&self) -> &str {
        &self.0.helpers
    }

    /// WGSL body using the straight-RGB `backdrop` and `source` arguments.
    pub fn body(&self) -> &str {
        &self.0.body
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
                    .with_helpers(include_str!("../../shaders/css_blend.wgsl"));
                BlendShader::wgsl(definition)
                    .expect("the static CSS blend shader must satisfy its WGSL contract")
            })
            .clone()
    }
}

impl<S: TypedColorSpace<Model = Rgb>> BlendOperation<S> for BlendMode {
    fn shader(self) -> BlendShader {
        self.shader_definition()
    }

    fn is_identity(&self) -> bool {
        *self == Self::Normal
    }
}

impl<S: TypedColorSpace<Model = Rgb>> BlendOperation<S> for BlendShader {
    fn shader(self) -> BlendShader {
        self
    }
}

/// A blend operation and the space in which its three components are evaluated.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    shader: BlendShader,
    space: ColorSpace,
}

impl Blend {
    pub fn new<S, O>(operation: O, space: S) -> Self
    where
        S: TypedColorSpace<Model = Rgb> + Into<ColorSpace>,
        O: BlendOperation<S>,
    {
        Self {
            shader: operation.shader(),
            space: space.into(),
        }
    }

    pub fn shader(&self) -> &BlendShader {
        &self.shader
    }

    /// Evaluates a custom blend shader in a three-channel color space.
    pub fn from_shader(shader: BlendShader, space: impl Into<ColorSpace>) -> Self {
        Self {
            shader,
            space: space.into(),
        }
    }

    pub fn space(&self) -> &ColorSpace {
        &self.space
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{Composite, Composition},
        color::{DisplayP3, Srgb},
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
            .is_ok()
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
    fn operations_can_be_restricted_to_a_space() {
        struct SrgbOnly;
        impl BlendOperation<Srgb> for SrgbOnly {
            fn shader(self) -> BlendShader {
                BlendMode::Screen.shader_definition()
            }
        }

        let blend = Blend::new(SrgbOnly, Srgb);
        assert_eq!(blend.space(), &ColorSpace::from(Srgb));
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
}

fn operation(helpers: &str, body: &str) -> String {
    format!(
        "{helpers}\nfn theoria_blend(backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> {{\n{body}\n}}\n"
    )
}
