use std::{error::Error, fmt, sync::Arc};

use crate::color::{ColorProfile, Rgb, RgbColorSpace};

/// The straight-RGB operation evaluated where source and backdrop overlap.
/// Alpha and Porter-Duff composition are separate from this operation.
///
/// Implement this for a specific profile to restrict where an operation may
/// be used. The resulting shader is recorded in the Canvas and compiled by
/// each renderer when first needed.
pub trait BlendOperation<P: ColorProfile<Model = Rgb>> {
    fn shader(self) -> BlendShader;
}

/// A backend-independent, immutable blend program definition.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct BlendShader(BlendShaderKind);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum BlendShaderKind {
    Builtin(BlendMode),
    Wgsl(Arc<str>),
}

impl BlendShader {
    pub fn builtin(mode: BlendMode) -> Self {
        Self(BlendShaderKind::Builtin(mode))
    }

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
        let shader = Self(BlendShaderKind::Wgsl(source));
        let complete = shader
            .wgsl_module("@group(2) @binding(0) var<uniform> operation: CompositionOperation;");
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

    pub(crate) fn builtin_mode(&self) -> Option<BlendMode> {
        match self.0 {
            BlendShaderKind::Builtin(mode) => Some(mode),
            BlendShaderKind::Wgsl(_) => None,
        }
    }

    pub(crate) fn wgsl_source(&self) -> Option<&str> {
        match &self.0 {
            BlendShaderKind::Builtin(_) => None,
            BlendShaderKind::Wgsl(source) => Some(source),
        }
    }

    pub(crate) fn wgsl_module(&self, binding: &str) -> String {
        let blend_function = self.wgsl_source().unwrap_or(
            "fn theoria_blend(_backdrop: vec3<f32>, source: vec3<f32>) -> vec3<f32> { return source; }",
        );
        include_str!("../../shaders/canvas_renderer/blend.wgsl")
            .replace("__THEORIA_BLEND_OPERATION_BINDING__", binding)
            .replace("__THEORIA_CUSTOM_BLEND__", blend_function)
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
}

impl<P: ColorProfile<Model = Rgb>> BlendOperation<P> for BlendMode {
    fn shader(self) -> BlendShader {
        BlendShader::builtin(self)
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
            "fn blend_rgb(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> { return a + b; } fn theoria_blend(a: vec3<f32>, b: vec3<f32>) -> vec3<f32> { return a; }"
        ).is_err());
    }

    #[test]
    fn operations_can_be_restricted_to_a_profile() {
        struct SrgbOnly;
        impl BlendOperation<Srgb> for SrgbOnly {
            fn shader(self) -> BlendShader {
                BlendShader::builtin(BlendMode::Screen)
            }
        }

        let blend = Blend::new(SrgbOnly, Srgb);
        assert_eq!(blend.space(), RgbColorSpace::Srgb);
        assert_eq!(blend.shader().builtin_mode(), Some(BlendMode::Screen));
    }
}
