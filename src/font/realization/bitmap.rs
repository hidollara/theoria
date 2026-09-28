use std::io::Cursor;

use skrifa::bitmap::{BitmapData, BitmapGlyph, Origin};

use crate::{
    canvas::{Canvas, ColorField, Coverage, ImagePattern, ImageSampling},
    image::Srgba8Image,
    math::{Matrix2, Vector2},
    transform::Affine2,
};

use super::GlyphPresentationError;

pub(super) fn canvas(
    glyph: BitmapGlyph<'_>,
    units_per_em: u16,
    foreground: &ColorField,
) -> Result<Canvas, GlyphPresentationError> {
    let (image, monochrome) = decode_image(&glyph)?;
    let unit_x = f32::from(units_per_em) / glyph.ppem_x;
    let unit_y = f32::from(units_per_em) / glyph.ppem_y;
    if !unit_x.is_finite() || !unit_y.is_finite() || unit_x <= 0.0 || unit_y <= 0.0 {
        return Err(GlyphPresentationError::InvalidBitmap);
    }

    let left = glyph.bearing_x + glyph.inner_bearing_x * unit_x;
    let origin_y = glyph.bearing_y + glyph.inner_bearing_y * unit_y;
    let top = match glyph.placement_origin {
        Origin::TopLeft => origin_y,
        Origin::BottomLeft => origin_y + glyph.height as f32 * unit_y,
    };
    let image_to_font = Affine2::new(
        Matrix2::from_columns(Vector2::new(unit_x, 0.0), Vector2::new(0.0, -unit_y)),
        Vector2::new(left, top),
    );
    let pattern = ImagePattern::placed(image, image_to_font)
        .ok_or(GlyphPresentationError::InvalidBitmap)?
        .with_sampling(ImageSampling::Linear);

    if monochrome {
        Ok(Canvas::draw(
            foreground.clone(),
            Coverage::from_alpha(pattern),
        ))
    } else {
        Ok(Canvas::draw(pattern, Coverage::all()))
    }
}

fn decode_image(glyph: &BitmapGlyph<'_>) -> Result<(Srgba8Image, bool), GlyphPresentationError> {
    match &glyph.data {
        BitmapData::Png(data) => decode_png(data).map(|image| (image, false)),
        BitmapData::Bgra(data) => {
            let pixel_count = pixel_count(glyph.width, glyph.height)?;
            if data.len() != pixel_count * 4 {
                return Err(GlyphPresentationError::InvalidBitmap);
            }
            let mut rgba = Vec::with_capacity(data.len());
            for pixel in data.chunks_exact(4) {
                let [blue, green, red, alpha] = [pixel[0], pixel[1], pixel[2], pixel[3]];
                if alpha == 0 {
                    rgba.extend_from_slice(&[0, 0, 0, 0]);
                } else {
                    rgba.extend_from_slice(&[
                        unpremultiply(red, alpha),
                        unpremultiply(green, alpha),
                        unpremultiply(blue, alpha),
                        alpha,
                    ]);
                }
            }
            Srgba8Image::new(glyph.width, glyph.height, rgba)
                .map(|image| (image, false))
                .map_err(|_| GlyphPresentationError::InvalidBitmap)
        }
        BitmapData::Mask(mask) => {
            let alpha = mask
                .decode(glyph.width, glyph.height)
                .map_err(|_| GlyphPresentationError::InvalidBitmap)?;
            let mut rgba = Vec::with_capacity(alpha.len() * 4);
            for alpha in alpha {
                rgba.extend_from_slice(&[255, 255, 255, alpha]);
            }
            Srgba8Image::new(glyph.width, glyph.height, rgba)
                .map(|image| (image, true))
                .map_err(|_| GlyphPresentationError::InvalidBitmap)
        }
    }
}

fn decode_png(data: &[u8]) -> Result<Srgba8Image, GlyphPresentationError> {
    let mut decoder = png::Decoder::new(Cursor::new(data));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder
        .read_info()
        .map_err(|_| GlyphPresentationError::InvalidBitmap)?;
    let buffer_size = reader
        .output_buffer_size()
        .ok_or(GlyphPresentationError::InvalidBitmap)?;
    let mut pixels = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut pixels)
        .map_err(|_| GlyphPresentationError::InvalidBitmap)?;
    let bytes = &pixels[..info.buffer_size()];
    let pixel_count = pixel_count(info.width, info.height)?;
    let mut rgba = Vec::with_capacity(pixel_count * 4);
    match info.color_type {
        png::ColorType::Rgba => rgba.extend_from_slice(bytes),
        png::ColorType::Rgb => {
            for pixel in bytes.chunks_exact(3) {
                rgba.extend_from_slice(&[pixel[0], pixel[1], pixel[2], 255]);
            }
        }
        png::ColorType::Grayscale => {
            for value in bytes {
                rgba.extend_from_slice(&[*value, *value, *value, 255]);
            }
        }
        png::ColorType::GrayscaleAlpha => {
            for pixel in bytes.chunks_exact(2) {
                rgba.extend_from_slice(&[pixel[0], pixel[0], pixel[0], pixel[1]]);
            }
        }
        png::ColorType::Indexed => return Err(GlyphPresentationError::InvalidBitmap),
    }
    Srgba8Image::new(info.width, info.height, rgba)
        .map_err(|_| GlyphPresentationError::InvalidBitmap)
}

fn pixel_count(width: u32, height: u32) -> Result<usize, GlyphPresentationError> {
    width
        .checked_mul(height)
        .and_then(|count| usize::try_from(count).ok())
        .ok_or(GlyphPresentationError::InvalidBitmap)
}

fn unpremultiply(component: u8, alpha: u8) -> u8 {
    ((u32::from(component) * 255 + u32::from(alpha) / 2) / u32::from(alpha)).min(255) as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::Color;
    use crate::{canvas::Opacity, color::Srgb};

    #[test]
    fn unpremultiplies_bgra_channels() {
        assert_eq!(unpremultiply(64, 128), 128);
        assert_eq!(unpremultiply(255, 128), 255);
    }

    #[test]
    fn embedded_bitmap_font_produces_a_canvas() {
        use skrifa::{GlyphId, MetadataProvider, instance::Size, raw::TableProvider};

        let face = skrifa::FontRef::new(font_test_data::CBDT).unwrap();
        let strikes = face.bitmap_strikes();
        let glyph = (0..256)
            .find_map(|id| strikes.glyph_for_size(Size::new(16.0), GlyphId::new(id)))
            .expect("test font contains a bitmap glyph");
        let result = canvas(
            glyph,
            face.head().unwrap().units_per_em(),
            &Color::new(Srgb, [0_u8, 0, 0], Opacity::<u8>::ONE).into(),
        );
        assert!(result.is_ok());
    }
}
