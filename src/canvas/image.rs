use std::{fmt, sync::Arc};

use crate::{
    color::{DisplayP3, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb},
    image::{AlphaAssociation, RasterImage, RasterImageInstanceKey, RasterPlane, Rgba8},
    transform::Projective2,
};

use super::canvas_image::CanvasImage;

/// A finite image that can be sampled by a Canvas.
///
/// `Image` describes a logical image, not a particular CPU buffer or backend
/// resource. A renderer may materialize, cache, discard, and rebuild its
/// physical representation without changing the image's identity.
#[derive(Clone, Debug)]
pub struct Image {
    kind: ImageKind,
}

#[derive(Clone, Debug)]
pub(crate) enum ImageKind {
    Raster(Arc<dyn RasterImageSource>),
    Canvas(CanvasImage),
}

pub(crate) enum RasterImageView<'a> {
    Rgba8 {
        plane: &'a RasterPlane<u8>,
        color_space: RgbColorSpace,
        alpha: AlphaAssociation,
    },
}

pub(crate) trait RasterImageSource: fmt::Debug + Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn instance_key(&self) -> RasterImageInstanceKey;
    fn view(&self) -> RasterImageView<'_>;
}

#[derive(Debug)]
struct Rgba8Source<P>
where
    P: crate::color::ColorProfile<Model = crate::color::Rgb>,
{
    image: RasterImage<Rgba8, P>,
    color_space: RgbColorSpace,
}

impl<P> RasterImageSource for Rgba8Source<P>
where
    P: crate::color::ColorProfile<Model = crate::color::Rgb> + fmt::Debug + Send + Sync,
{
    fn width(&self) -> u32 {
        self.image.width()
    }

    fn height(&self) -> u32 {
        self.image.height()
    }

    fn instance_key(&self) -> RasterImageInstanceKey {
        self.image.instance_key()
    }

    fn view(&self) -> RasterImageView<'_> {
        RasterImageView::Rgba8 {
            plane: &self.image.planes()[0],
            color_space: self.color_space,
            alpha: self.image.alpha_association(),
        }
    }
}

impl Image {
    pub fn width(&self) -> u32 {
        match &self.kind {
            ImageKind::Raster(image) => image.width(),
            ImageKind::Canvas(image) => image.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match &self.kind {
            ImageKind::Raster(image) => image.height(),
            ImageKind::Canvas(image) => image.height(),
        }
    }

    pub(crate) const fn kind(&self) -> &ImageKind {
        &self.kind
    }

    pub(super) fn natural_canvas_to_image(&self) -> Projective2 {
        match &self.kind {
            ImageKind::Raster(_) => Projective2::IDENTITY,
            ImageKind::Canvas(image) => {
                let normalized_to_pixels = Projective2::from_rows([
                    [image.width() as f32, 0.0, 0.0],
                    [0.0, image.height() as f32, 0.0],
                    [0.0, 0.0, 1.0],
                ]);
                image.view().canvas_to_view.then(normalized_to_pixels)
            }
        }
    }
}

macro_rules! rgba8_image {
    ($profile:ty, $space:expr) => {
        impl From<RasterImage<Rgba8, $profile>> for Image {
            fn from(image: RasterImage<Rgba8, $profile>) -> Self {
                Self {
                    kind: ImageKind::Raster(Arc::new(Rgba8Source {
                        image,
                        color_space: $space,
                    })),
                }
            }
        }
    };
}

rgba8_image!(Srgb, RgbColorSpace::Srgb);
rgba8_image!(LinearSrgb, RgbColorSpace::LinearSrgb);
rgba8_image!(DisplayP3, RgbColorSpace::DisplayP3);
rgba8_image!(LinearDisplayP3, RgbColorSpace::LinearDisplayP3);

impl From<CanvasImage> for Image {
    fn from(image: CanvasImage) -> Self {
        Self {
            kind: ImageKind::Canvas(image),
        }
    }
}
