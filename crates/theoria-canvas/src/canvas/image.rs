use std::{fmt, sync::Arc};

use crate::{
    color::{
        ColorSpace, DisplayP3, IccProfile, IccRenderingIntent, LinearDisplayP3, LinearSrgb, Srgb,
    },
    image::{
        AlphaAssociation, IccRasterImage, RasterImage, RasterImageInstanceKey, RasterPlane, Rgb8,
        Rgba8,
    },
    transform::Projective2,
};

use super::canvas_image::CanvasImage;

pub use crate::image::IccImageError;

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
pub enum ImageKind {
    Raster(Arc<dyn RasterImageSource>),
    Canvas(CanvasImage),
}

pub enum RasterImageView<'a> {
    Rgb8 {
        plane: &'a RasterPlane<u8>,
        color_space: ColorSpace,
    },
    Rgba8 {
        plane: &'a RasterPlane<u8>,
        color_space: ColorSpace,
        alpha: AlphaAssociation,
    },
    Icc8 {
        pixels: &'a [u8],
        profile: &'a IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
    },
}

impl RasterImageSource for IccRasterImage {
    fn width(&self) -> u32 {
        self.width()
    }

    fn height(&self) -> u32 {
        self.height()
    }

    fn instance_key(&self) -> RasterImageInstanceKey {
        self.instance_key()
    }

    fn view(&self) -> RasterImageView<'_> {
        RasterImageView::Icc8 {
            pixels: self.pixels(),
            profile: self.profile(),
            intent: self.intent(),
            has_alpha: self.has_alpha(),
        }
    }
}

#[derive(Debug)]
struct Rgb8Source<S>
where
    S: crate::color::TypedColorSpace<Model = crate::color::Rgb>,
{
    image: RasterImage<Rgb8, S>,
    color_space: ColorSpace,
}

impl<S> RasterImageSource for Rgb8Source<S>
where
    S: crate::color::TypedColorSpace<Model = crate::color::Rgb> + fmt::Debug + Send + Sync,
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
        RasterImageView::Rgb8 {
            plane: &self.image.planes()[0],
            color_space: self.color_space.clone(),
        }
    }
}

pub trait RasterImageSource: fmt::Debug + Send + Sync {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn instance_key(&self) -> RasterImageInstanceKey;
    fn view(&self) -> RasterImageView<'_>;
}

#[derive(Debug)]
struct Rgba8Source<S>
where
    S: crate::color::TypedColorSpace<Model = crate::color::Rgb>,
{
    image: RasterImage<Rgba8, S>,
    color_space: ColorSpace,
}

impl<S> RasterImageSource for Rgba8Source<S>
where
    S: crate::color::TypedColorSpace<Model = crate::color::Rgb> + fmt::Debug + Send + Sync,
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
            color_space: self.color_space.clone(),
            alpha: self.image.alpha_association(),
        }
    }
}

impl Image {
    /// Retains interleaved normalized device samples and their ICC profile.
    /// The renderer interprets them when sampling the image. Alpha, when
    /// present, is the last byte of each pixel and is straight (not premultiplied).
    pub fn from_icc_u8(
        width: u32,
        height: u32,
        pixels: impl Into<Arc<[u8]>>,
        profile: IccProfile,
        intent: IccRenderingIntent,
        has_alpha: bool,
    ) -> Result<Self, IccImageError> {
        IccRasterImage::new(width, height, pixels, profile, intent, has_alpha).map(Into::into)
    }

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

    pub const fn kind(&self) -> &ImageKind {
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
                ])
                .expect("valid projective coefficients");
                image
                    .view()
                    .canvas_to_view
                    .then(normalized_to_pixels)
                    .expect("finite composed transform")
            }
        }
    }
}

macro_rules! rgba8_image {
    ($space:ty) => {
        impl From<RasterImage<Rgba8, $space>> for Image {
            fn from(image: RasterImage<Rgba8, $space>) -> Self {
                let color_space = image.space().into();
                Self {
                    kind: ImageKind::Raster(Arc::new(Rgba8Source { image, color_space })),
                }
            }
        }
    };
}

macro_rules! rgb8_image {
    ($space:ty) => {
        impl From<RasterImage<Rgb8, $space>> for Image {
            fn from(image: RasterImage<Rgb8, $space>) -> Self {
                let color_space = image.space().into();
                Self {
                    kind: ImageKind::Raster(Arc::new(Rgb8Source { image, color_space })),
                }
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

impl From<CanvasImage> for Image {
    fn from(image: CanvasImage) -> Self {
        Self {
            kind: ImageKind::Canvas(image),
        }
    }
}

impl From<IccRasterImage> for Image {
    fn from(image: IccRasterImage) -> Self {
        Self {
            kind: ImageKind::Raster(Arc::new(image)),
        }
    }
}
