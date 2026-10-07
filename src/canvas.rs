mod blend;
pub(crate) mod canvas_image;
mod color;
mod color_coordinates;
mod color_field;
mod composition;
mod coverage;
pub(crate) mod image;
mod model;
mod opacity;
mod view;

pub use blend::{Blend, BlendMode, BlendOperation, BlendShader, BlendShaderError, WgslBlend};
pub use canvas_image::{CanvasImage, CanvasImageError};
pub(crate) use color::ColorValue;
pub use color::{Color, ColorError};
pub use color_coordinates::{ColorCoordinates, CoordinateValue};
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
pub use opacity::{Opacity, OpacityError, OpacitySample};
pub use view::CanvasView;

pub(crate) use coverage::CoverageNode;
pub(crate) use model::{CanvasNode, Draw};

#[cfg(test)]
mod tests;
