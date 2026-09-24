use std::{fmt, sync::Arc};

use crate::{
    color::{
        DisplayP3, IccProfile, IccRenderingIntent, LinearDisplayP3, LinearSrgb, RgbColorSpace, Srgb,
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
pub(crate) enum ImageKind {
    Raster(Arc<dyn RasterImageSource>),
    Canvas(CanvasImage),
}

pub(crate) enum RasterImageView<'a> {
    Rgb8 {
        plane: &'a RasterPlane<u8>,
        color_space: RgbColorSpace,
    },
    Rgba8 {
        plane: &'a RasterPlane<u8>,
        color_space: RgbColorSpace,
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
struct Rgb8Source<P>
where
    P: crate::color::ColorProfile<Model = crate::color::Rgb>,
{
    image: RasterImage<Rgb8, P>,
    color_space: RgbColorSpace,
}

impl<P> RasterImageSource for Rgb8Source<P>
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
        RasterImageView::Rgb8 {
            plane: &self.image.planes()[0],
            color_space: self.color_space,
        }
    }
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
    /// Retains interleaved normalized device samples and their ICC profile.
    /// The renderer converts them when materializing the image. Alpha, when
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

macro_rules! rgb8_image {
    ($profile:ty, $space:expr) => {
        impl From<RasterImage<Rgb8, $profile>> for Image {
            fn from(image: RasterImage<Rgb8, $profile>) -> Self {
                Self {
                    kind: ImageKind::Raster(Arc::new(Rgb8Source {
                        image,
                        color_space: $space,
                    })),
                }
            }
        }
    };
}

rgb8_image!(Srgb, RgbColorSpace::Srgb);
rgb8_image!(LinearSrgb, RgbColorSpace::LinearSrgb);
rgb8_image!(DisplayP3, RgbColorSpace::DisplayP3);
rgb8_image!(LinearDisplayP3, RgbColorSpace::LinearDisplayP3);
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

impl From<IccRasterImage> for Image {
    fn from(image: IccRasterImage) -> Self {
        Self {
            kind: ImageKind::Raster(Arc::new(image)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas_renderer::raster::convert_icc8_to_rgba8;

    #[test]
    fn icc_image_preserves_samples_until_renderer_conversion() {
        let profile =
            IccProfile::from_bytes(include_bytes!("../color/fixtures/sRGB-v4.icc")).unwrap();
        let source: Arc<[u8]> = vec![64, 128, 192, 77].into();
        let image = Image::from_icc_u8(
            1,
            1,
            source.clone(),
            profile,
            IccRenderingIntent::RelativeColorimetric,
            true,
        )
        .unwrap();
        let ImageKind::Raster(raster) = image.kind() else {
            panic!("ICC image was materialized at the Canvas boundary")
        };
        let RasterImageView::Icc8 {
            pixels,
            profile,
            intent,
            has_alpha,
        } = raster.view()
        else {
            panic!("ICC samples were replaced before renderer conversion")
        };
        assert_eq!(pixels, source.as_ref());
        let converted = convert_icc8_to_rgba8(1, pixels, profile, intent, has_alpha).unwrap();
        for (actual, expected) in converted.samples()[..3].iter().zip([64, 128, 192]) {
            assert!(actual.abs_diff(expected) <= 2);
        }
        assert_eq!(converted.samples()[3], 77);
    }

    #[test]
    fn icc_image_checks_sample_count_at_construction() {
        let profile =
            IccProfile::from_bytes(include_bytes!("../color/fixtures/sGrey-v4.icc")).unwrap();
        assert!(matches!(
            Image::from_icc_u8(
                2,
                1,
                vec![127],
                profile,
                IccRenderingIntent::RelativeColorimetric,
                false,
            ),
            Err(IccImageError::InvalidDataLength {
                expected: 2,
                actual: 1
            })
        ));
    }
}
