use crate::{canvas::image::Image, transform::Projective2};

/// Filtering used when an image-backed color field is sampled between texels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageSampling {
    Nearest,
    #[default]
    Linear,
}

/// Defines an image-backed color field outside its normalized image extent.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageExtend {
    /// Produces transparent black outside the image.
    #[default]
    Transparent,
    /// Repeats the nearest edge texel.
    Clamp,
    /// Tiles the image with period one.
    Repeat,
    /// Tiles the image while reflecting every other copy.
    Mirror,
}

/// A spatial RGBA color field backed by an immutable image.
///
/// Image coordinates use the image's top-left-origin pixel space. The mapping
/// converts Canvas coordinates into that space before normalized sampling.
#[derive(Clone, Debug)]
pub struct ImagePattern {
    image: Image,
    canvas_to_image: Projective2,
    sampling: ImageSampling,
    extend_x: ImageExtend,
    extend_y: ImageExtend,
}

impl ImagePattern {
    /// Creates a pattern using the source's natural placement in Canvas space.
    ///
    /// An sRGBA8 image places one pixel per Canvas unit from the origin. A
    /// CanvasImage returns to the Canvas region from which it was evaluated.
    pub fn new(image: impl Into<Image>) -> Self {
        let image = image.into();
        let canvas_to_image = image.natural_canvas_to_image();
        Self {
            image,
            canvas_to_image,
            sampling: ImageSampling::Linear,
            extend_x: ImageExtend::Transparent,
            extend_y: ImageExtend::Transparent,
        }
    }

    /// Places image pixel coordinates into Canvas space.
    pub fn placed(
        image: impl Into<Image>,
        image_to_canvas: impl Into<Projective2>,
    ) -> Option<Self> {
        Some(Self::new(image).with_canvas_to_image(image_to_canvas.into().try_inverse()?))
    }

    pub const fn with_canvas_to_image(mut self, canvas_to_image: Projective2) -> Self {
        self.canvas_to_image = canvas_to_image;
        self
    }

    pub const fn with_sampling(mut self, sampling: ImageSampling) -> Self {
        self.sampling = sampling;
        self
    }

    pub const fn with_extend(mut self, extend: ImageExtend) -> Self {
        self.extend_x = extend;
        self.extend_y = extend;
        self
    }

    pub const fn with_extend_x(mut self, extend: ImageExtend) -> Self {
        self.extend_x = extend;
        self
    }

    pub const fn with_extend_y(mut self, extend: ImageExtend) -> Self {
        self.extend_y = extend;
        self
    }

    pub const fn image(&self) -> &Image {
        &self.image
    }

    pub const fn canvas_to_image(&self) -> Projective2 {
        self.canvas_to_image
    }

    pub const fn sampling(&self) -> ImageSampling {
        self.sampling
    }

    pub const fn extend_x(&self) -> ImageExtend {
        self.extend_x
    }

    pub const fn extend_y(&self) -> ImageExtend {
        self.extend_y
    }
}
