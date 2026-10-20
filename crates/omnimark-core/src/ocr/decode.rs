//! Decoding images for OCR within fixed limits (untrusted input).

use std::io::Cursor;

use image::{DynamicImage, ImageReader, Limits};

use super::{OcrError, OcrImage};

/// Larger images are rejected before decoding.
pub const MAX_INPUT_DIMENSION: u32 = 20_000;
/// Most memory one decode may allocate.
pub const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;
/// The long side is shrunk to this (never enlarged).
pub const MAX_OCR_SIDE: u32 = 4096;
/// Images whose short side is smaller hold no readable text and are skipped.
pub const MIN_OCR_SIDE: u32 = 16;

/// Decodes JPEG, PNG, WebP, GIF, BMP or TIFF bytes. `Ok(None)`: too small to hold text.
pub fn decode(bytes: &[u8]) -> Result<Option<OcrImage>, OcrError> {
    let mut reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| OcrError::Image(e.to_string()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_INPUT_DIMENSION);
    limits.max_image_height = Some(MAX_INPUT_DIMENSION);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let image = reader
        .decode()
        .map_err(|e| OcrError::Image(e.to_string()))?;
    Ok(from_dynamic(image))
}

/// Shrinks the long side to [`MAX_OCR_SIDE`] and flattens transparency onto white
/// (transparent pixels are black in RGB and would hide dark text).
///
/// Oversized images are flattened, reduced to grey and area-averaged with `thumbnail`:
/// `resize` allocates an f32 intermediate of old width x new height x 16 bytes (about 1 GB
/// for a 16000 px square), `thumbnail` only allocates its output.
pub fn from_dynamic(image: DynamicImage) -> Option<OcrImage> {
    if image.width().min(image.height()) < MIN_OCR_SIDE {
        return None;
    }
    let image = if image.width().max(image.height()) > MAX_OCR_SIDE {
        let grey = match image {
            DynamicImage::ImageLuma8(grey) => grey,
            other if other.color().has_alpha() => {
                let mut rgba = other.into_rgba8();
                flatten_on_white(&mut rgba);
                DynamicImage::ImageRgba8(rgba).into_luma8()
            }
            other => other.into_luma8(),
        };
        DynamicImage::ImageLuma8(grey).thumbnail(MAX_OCR_SIDE, MAX_OCR_SIDE)
    } else {
        image
    };
    if image.width().min(image.height()) < MIN_OCR_SIDE {
        return None;
    }
    let has_alpha = image.color().has_alpha();
    let mut rgba = image.into_rgba8();
    if has_alpha {
        flatten_on_white(&mut rgba);
    }
    let (width, height) = rgba.dimensions();
    OcrImage::new(width, height, rgba.into_raw()).ok()
}

fn flatten_on_white(rgba: &mut image::RgbaImage) {
    for pixel in rgba.pixels_mut() {
        let [r, g, b, a] = pixel.0;
        let a = u32::from(a);
        let blend = |c: u8| ((u32::from(c) * a + 255 * (255 - a)) / 255) as u8;
        pixel.0 = [blend(r), blend(g), blend(b), 255];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ocr::test_render::png_bytes;
    use image::RgbaImage;

    #[test]
    fn decodes_png_and_flattens_transparency_onto_white() {
        let clear = decode(&png_bytes(20, 30, [0, 0, 0, 0])).unwrap().unwrap();
        assert_eq!((clear.width(), clear.height()), (20, 30));
        assert_eq!(&clear.rgba()[..4], &[255, 255, 255, 255]);
        let opaque = decode(&png_bytes(20, 20, [10, 20, 30, 255]))
            .unwrap()
            .unwrap();
        assert_eq!(&opaque.rgba()[..4], &[10, 20, 30, 255]);
    }

    #[test]
    fn tiny_images_are_skipped() {
        assert_eq!(decode(&png_bytes(15, 100, [0, 0, 0, 255])).unwrap(), None);
    }

    #[test]
    fn long_side_is_shrunk_but_never_enlarged() {
        let wide = from_dynamic(DynamicImage::ImageRgba8(RgbaImage::new(8192, 100))).unwrap();
        assert_eq!((wide.width(), wide.height()), (4096, 50));
        let photo = from_dynamic(DynamicImage::ImageRgba8(RgbaImage::new(1615, 1967))).unwrap();
        assert_eq!((photo.width(), photo.height()), (1615, 1967));
        assert!(from_dynamic(DynamicImage::ImageRgba8(RgbaImage::new(20_000, 30))).is_none());
    }

    #[test]
    fn huge_grey_image_is_downscaled_without_the_resize_blowup() {
        use image::GrayImage;
        let huge = from_dynamic(DynamicImage::ImageLuma8(GrayImage::new(16_000, 16_000))).unwrap();
        assert_eq!((huge.width(), huge.height()), (4096, 4096));
    }

    #[test]
    fn huge_transparent_image_is_flattened_white_when_downscaled() {
        let wide = from_dynamic(DynamicImage::ImageRgba8(RgbaImage::new(8192, 100))).unwrap();
        assert_eq!(&wide.rgba()[..4], &[255, 255, 255, 255]);
    }

    #[test]
    fn garbage_and_oversized_headers_are_errors() {
        assert!(decode(b"not an image").is_err());
        let mut bytes = png_bytes(20, 20, [0, 0, 0, 255]);
        bytes[16..20].copy_from_slice(&30_000u32.to_be_bytes());
        bytes[20..24].copy_from_slice(&30_000u32.to_be_bytes());
        assert!(decode(&bytes).is_err());
    }
}
