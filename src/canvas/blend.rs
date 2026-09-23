use std::{
    error::Error,
    fmt,
    sync::{Arc, OnceLock},
};

use crate::color::{ColorProfile, Rgb, RgbColorSpace};

use super::composition_shader::{self, Binding};

/// The straight-RGB operation evaluated where source and backdrop overlap.
/// Alpha and Porter-Duff composition are separate from this operation.
///
/// Implement this for a specific profile to restrict where an operation may
/// be used. The resulting shader is recorded in the Canvas and compiled by
/// each renderer when first needed.
pub trait BlendOperation<P: ColorProfile<Model = Rgb>> {
    fn shader(self) -> BlendShader;

    /// Whether this operation returns the source unchanged for every input.
    fn is_identity(&self) -> bool {
        false
    }
}

/// A backend-independent, immutable blend program definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlendShader(Arc<str>);

impl BlendShader {
    /// Defines a pure WGSL `fn theoria_blend(backdrop: vec3<f32>, source:
    /// vec3<f32>) -> vec3<f32>`. The code may contain helper functions, but
    /// cannot declare entry points or global resources.
    pub fn wgsl(source: impl Into<Arc<str>>) -> Result<Self, BlendShaderError> {
        let source = source.into();
        let validation = format!(
            "{source}\n@fragment fn theoria_validate() -> @location(0) vec4<f32> {{\n    return vec4<f32>(theoria_blend(vec3<f32>(0.0), vec3<f32>(0.0)), 1.0);\n}}"
        );
        let module = naga::front::wgsl::parse_str(&validation)
            .map_err(|error| BlendShaderError(error.emit_to_string(&validation)))?;
        if !module.global_variables.is_empty() || module.entry_points.len() != 1 {
            return Err(BlendShaderError(
                "blend WGSL must not declare entry points or global resources".into(),
            ));
        }
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|error| BlendShaderError(error.to_string()))?;
        let shader = Self(source);
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
        IDENTITY.get_or_init(|| Self(Arc::from(composition_shader::IDENTITY)))
    }

    pub(crate) fn wgsl_module(&self, binding: Binding) -> String {
        composition_shader::assemble(binding, &self.0)
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
                let source = format!(
                    "{}\nfn theoria_blend(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {{ return clamp({expression}, vec3<f32>(0.0), vec3<f32>(1.0)); }}",
                    include_str!("../../shaders/canvas_renderer/css_blend.wgsl"),
                );
                BlendShader::wgsl(source)
                    .expect("the static CSS blend shader must satisfy its WGSL contract")
            })
            .clone()
    }
}

impl<P: ColorProfile<Model = Rgb>> BlendOperation<P> for BlendMode {
    fn shader(self) -> BlendShader {
        self.shader_definition()
    }

    fn is_identity(&self) -> bool {
        *self == Self::Normal
    }
}

impl<P: ColorProfile<Model = Rgb>> BlendOperation<P> for BlendShader {
    fn shader(self) -> BlendShader {
        self
    }
}

/// A blend operation and the RGB space in which it is evaluated.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Blend {
    shader: BlendShader,
    space: RgbColorSpace,
}

impl Blend {
    pub fn new<P, O>(operation: O, space: P) -> Self
    where
        P: ColorProfile<Model = Rgb> + Into<RgbColorSpace>,
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

    pub const fn space(&self) -> RgbColorSpace {
        self.space
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        canvas::{Composite, Composition},
        color::Srgb,
    };

    #[test]
    fn wgsl_blend_validates_its_pure_rgb_contract() {
        let shader = BlendShader::wgsl(
            "fn theoria_blend(backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> { return backdrop * source; }",
        )
        .unwrap();
        let composition =
            Composition::new(Composite::SourceOver, Srgb).with_blend(shader.clone(), Srgb);
        assert_eq!(composition.blend().unwrap().shader(), &shader);
        assert!(BlendShader::wgsl("fn other() -> f32 { return 1.0; }").is_err());
        assert!(BlendShader::wgsl("var<private> hidden: f32; fn theoria_blend(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> { return a + b; }").is_err());
        assert!(BlendShader::wgsl(
            "fn straight_rgb(a: vec4<f32>) -> vec3<f32> { return a.rgb; } fn theoria_blend(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> { return a; }"
        ).is_err());
        assert!(BlendShader::wgsl(
            "fn hard_light_component(a: f32, b: f32) -> f32 { return a * b; } fn theoria_blend(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> { return vec3<f32>(hard_light_component(a.r, b.r)); }"
        ).is_ok());
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
        assert_eq!(blend.space(), RgbColorSpace::Srgb);
        assert_eq!(blend.shader(), &BlendMode::Screen.shader_definition());
    }

    #[test]
    fn css_modes_and_custom_operations_use_the_same_shader_definition() {
        let custom = BlendShader::wgsl(
            "fn theoria_blend(cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> { return cb * cs; }",
        )
        .unwrap();
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
}
