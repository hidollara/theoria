use crate::{image::Srgba8Image, transform::Projective2};

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
    Srgba8(Srgba8Image),
    Canvas(CanvasImage),
}

impl Image {
    pub fn width(&self) -> u32 {
        match &self.kind {
            ImageKind::Srgba8(image) => image.width(),
            ImageKind::Canvas(image) => image.width(),
        }
    }

    pub fn height(&self) -> u32 {
        match &self.kind {
            ImageKind::Srgba8(image) => image.height(),
            ImageKind::Canvas(image) => image.height(),
        }
    }

    pub(crate) const fn kind(&self) -> &ImageKind {
        &self.kind
    }

    pub(super) fn natural_canvas_to_image(&self) -> Projective2 {
        match &self.kind {
            ImageKind::Srgba8(_) => Projective2::IDENTITY,
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

impl From<Srgba8Image> for Image {
    fn from(image: Srgba8Image) -> Self {
        Self {
            kind: ImageKind::Srgba8(image),
        }
    }
}

impl From<CanvasImage> for Image {
    fn from(image: CanvasImage) -> Self {
        Self {
            kind: ImageKind::Canvas(image),
        }
    }
}
