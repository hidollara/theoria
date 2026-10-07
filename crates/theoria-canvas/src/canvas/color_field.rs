use crate::{
    color::{DisplayP3, LinearDisplayP3, LinearSrgb, Srgb},
    image::{RasterImage, Rgb8, Rgba8},
};

use super::{Canvas, Color, canvas_image::CanvasImage, image::Image};

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

macro_rules! rgba8_image {
    ($space:ty) => {
        impl From<RasterImage<Rgba8, $space>> for ColorField {
            fn from(image: RasterImage<Rgba8, $space>) -> Self {
                Self::Image(ImagePattern::new(image))
            }
        }
    };
}

macro_rules! rgb8_image {
    ($space:ty) => {
        impl From<RasterImage<Rgb8, $space>> for ColorField {
            fn from(image: RasterImage<Rgb8, $space>) -> Self {
                Self::Image(ImagePattern::new(image))
            }
        }
    };
}

rgb8_image!(Srgb);
rgb8_image!(LinearSrgb);
rgb8_image!(DisplayP3);
rgb8_image!(LinearDisplayP3);
rgba8_image!(Srgb);
rgba8_image!(LinearSrgb);
rgba8_image!(DisplayP3);
rgba8_image!(LinearDisplayP3);

impl From<CanvasImage> for ColorField {
    fn from(image: CanvasImage) -> Self {
        Self::Image(ImagePattern::new(image))
    }
}

impl From<Image> for ColorField {
    fn from(image: Image) -> Self {
        Self::Image(ImagePattern::new(image))
    }
}

impl<C> From<C> for ColorField
where
    C: Into<Color>,
{
    fn from(color: C) -> Self {
        Self::Solid(color.into())
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
