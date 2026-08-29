use crate::{color::Color, image::Image};

use super::Canvas;

mod gradient;
mod image_pattern;

pub use gradient::{
    AcrossPathGradient, AcrossPathGradientError, AlongPathGradient, ColorRamp, ColorRampError,
    ConicGradient, ConicGradientError, GradientExtend, GradientPoint, GradientStop,
    GradientStopError, LinearGradient, LinearGradientError, MeshGradient, MeshGradientError,
    RadialGradient, RadialGradientError,
};
pub use image_pattern::{ImageExtend, ImagePattern, ImageSampling};

/// A backend-independent RGBA field over the unbounded Canvas plane.
///
/// Alpha belongs to the field itself (for example an image's alpha or a
/// translucent gradient); it is not the amount by which a draw is applied.
#[derive(Clone, Debug)]
pub enum ColorField {
    Solid(Color),
    LinearGradient(LinearGradient),
    RadialGradient(RadialGradient),
    ConicGradient(ConicGradient),
    AlongPathGradient(AlongPathGradient),
    AcrossPathGradient(AcrossPathGradient),
    MeshGradient(MeshGradient),
    Image(ImagePattern),
    /// Evaluates a child Canvas as one spatial RGBA field.
    Canvas(Canvas),
}

impl From<Image> for ColorField {
    fn from(image: Image) -> Self {
        Self::Image(ImagePattern::new(image))
    }
}

impl From<Color> for ColorField {
    fn from(color: Color) -> Self {
        Self::Solid(color)
    }
}

impl From<LinearGradient> for ColorField {
    fn from(gradient: LinearGradient) -> Self {
        Self::LinearGradient(gradient)
    }
}

impl From<RadialGradient> for ColorField {
    fn from(gradient: RadialGradient) -> Self {
        Self::RadialGradient(gradient)
    }
}

impl From<ConicGradient> for ColorField {
    fn from(gradient: ConicGradient) -> Self {
        Self::ConicGradient(gradient)
    }
}

impl From<AlongPathGradient> for ColorField {
    fn from(gradient: AlongPathGradient) -> Self {
        Self::AlongPathGradient(gradient)
    }
}

impl From<AcrossPathGradient> for ColorField {
    fn from(gradient: AcrossPathGradient) -> Self {
        Self::AcrossPathGradient(gradient)
    }
}

impl From<MeshGradient> for ColorField {
    fn from(gradient: MeshGradient) -> Self {
        Self::MeshGradient(gradient)
    }
}

impl From<ImagePattern> for ColorField {
    fn from(pattern: ImagePattern) -> Self {
        Self::Image(pattern)
    }
}

impl From<Canvas> for ColorField {
    fn from(canvas: Canvas) -> Self {
        Self::Canvas(canvas)
    }
}
