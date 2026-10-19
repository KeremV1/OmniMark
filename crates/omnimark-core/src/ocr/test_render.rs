//! Test helpers: in-memory PNGs and text rendered with a system font (no font is committed).

use std::io::Cursor;

use ab_glyph::{point, Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

use super::OcrImage;

const FONT_CANDIDATES: [&str; 5] = [
    "C:/Windows/Fonts/arial.ttf",
    "/System/Library/Fonts/Supplemental/Arial.ttf",
    "/Library/Fonts/Arial.ttf",
    "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    "/usr/share/fonts/dejavu/DejaVuSans.ttf",
];

/// A `width` x `height` PNG filled with one RGBA colour.
pub(crate) fn png_bytes(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let image = RgbaImage::from_pixel(width, height, Rgba(pixel));
    let mut out = Vec::new();
    DynamicImage::ImageRgba8(image)
        .write_to(&mut Cursor::new(&mut out), ImageFormat::Png)
        .expect("encode png");
    out
}

/// `OMNIMARK_TEST_FONT`, else the first font found in common system locations.
pub(crate) fn system_font() -> Option<Vec<u8>> {
    if let Some(path) = std::env::var_os("OMNIMARK_TEST_FONT") {
        return std::fs::read(path).ok();
    }
    FONT_CANDIDATES.iter().find_map(|p| std::fs::read(p).ok())
}

/// Black text on white with 20 px margins; one image line per `\n`.
pub(crate) fn render_text(text: &str, font_data: &[u8], px: f32) -> OcrImage {
    let font = FontRef::try_from_slice(font_data).expect("font");
    let scale = PxScale::from(px);
    let scaled = font.as_scaled(scale);
    let line_height = (scaled.ascent() - scaled.descent() + scaled.line_gap()).ceil();
    let lines: Vec<&str> = text.lines().collect();
    let text_width = lines
        .iter()
        .map(|l| {
            l.chars()
                .map(|c| scaled.h_advance(font.glyph_id(c)))
                .sum::<f32>()
        })
        .fold(0.0, f32::max);
    let width = text_width.ceil() as u32 + 40;
    let height = (line_height * lines.len() as f32).ceil() as u32 + 40;
    let mut canvas = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));
    for (row, line) in lines.iter().enumerate() {
        let mut x = 20.0;
        let baseline = 20.0 + scaled.ascent() + line_height * row as f32;
        for c in line.chars() {
            let id = font.glyph_id(c);
            let glyph = id.with_scale_and_position(scale, point(x, baseline));
            x += scaled.h_advance(id);
            let Some(outline) = font.outline_glyph(glyph) else {
                continue;
            };
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px = i64::from(gx) + bounds.min.x as i64;
                let py = i64::from(gy) + bounds.min.y as i64;
                if px >= 0 && py >= 0 && (px as u32) < width && (py as u32) < height {
                    let value = (255.0 * (1.0 - coverage.clamp(0.0, 1.0))) as u8;
                    let pixel = canvas.get_pixel_mut(px as u32, py as u32);
                    for channel in 0..3 {
                        pixel.0[channel] = pixel.0[channel].min(value);
                    }
                }
            });
        }
    }
    OcrImage::new(width, height, canvas.into_raw()).expect("rgba length")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_dark_pixels_for_text() {
        let Some(font) = system_font() else {
            eprintln!("no system font; skipping");
            return;
        };
        let image = render_text("Hi\nİğ", &font, 32.0);
        assert!(image.width() > 40 && image.height() > 40);
        assert!(image.rgba().chunks(4).any(|p| p[0] < 100));
    }
}
