pub(crate) mod canvas_image;
mod color_field;
mod composition;
mod coverage;
pub(crate) mod image;
mod model;
mod view;

pub use canvas_image::{CanvasImage, CanvasImageError};
pub use color_field::{
    AcrossPathGradient, AcrossPathGradientError, AlongPathGradient, ColorField, ColorRamp,
    ColorRampError, ConicGradient, ConicGradientError, GradientExtend, GradientPoint, GradientStop,
    GradientStopError, ImageExtend, ImagePattern, ImageSampling, LinearGradient,
    LinearGradientError, MeshGradient, MeshGradientError, RadialGradient, RadialGradientError,
};
pub use composition::{BlendMode, Composite};
pub use coverage::Coverage;
pub use image::Image;
pub use model::Canvas;
pub use view::CanvasView;

pub(crate) use coverage::CoverageNode;
pub(crate) use model::{CanvasNode, Draw};

#[cfg(test)]
mod tests;
