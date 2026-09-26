mod blend;
pub(crate) mod canvas_image;
mod color;
mod color_field;
mod composition;
pub(crate) mod composition_shader;
mod coverage;
pub(crate) mod image;
mod model;
mod view;

pub use blend::{Blend, BlendMode, BlendOperation, BlendShader, BlendShaderError, WgslBlend};
pub use canvas_image::{CanvasImage, CanvasImageError};
pub use color::{CanvasColor, CanvasIccColorError};
pub(crate) use color::{CanvasProfile, ColorValue};
pub use color_field::{
    AcrossPathGradient, AcrossPathGradientError, AlongPathGradient, ColorField, ColorRamp,
    ColorRampError, ConicGradient, ConicGradientError, GradientExtend, GradientPoint, GradientStop,
    GradientStopError, ImageExtend, ImagePattern, ImageSampling, LinearGradient,
    LinearGradientError, MeshGradient, MeshGradientError, RadialGradient, RadialGradientError,
};
pub use composition::{Composite, Composition};
pub use coverage::Coverage;
pub use image::{IccImageError, Image};
pub use model::Canvas;
pub use view::CanvasView;

pub(crate) use coverage::CoverageNode;
pub(crate) use model::{CanvasNode, Draw};

#[cfg(test)]
mod tests;
