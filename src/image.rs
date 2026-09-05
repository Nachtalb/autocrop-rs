//! Image container, decoding, downscaling and the per-pixel colour operations.

use std::path::Path;

use crate::geometry::Rect;

/// An RGB colour.
pub type Color = [u8; 3];

/// Output format for [`RgbImage::encode`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Encoding {
    /// Lossless PNG.
    Png,
    /// JPEG at the given quality (1..=100).
    Jpeg {
        /// Encoder quality.
        quality: u8,
    },
    /// Lossless WebP (the `image` crate has no lossy WebP encoder).
    WebPLossless,
}

/// A packed 8-bit RGB image in row-major order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbImage {
    /// Width in pixels.
    pub width: usize,
    /// Height in pixels.
    pub height: usize,
    /// `width * height` pixels, row by row.
    pub pixels: Vec<Color>,
}

impl RgbImage {
    /// Wrap a pixel buffer.
    ///
    /// # Panics
    /// When the buffer length does not match `width * height`.
    #[must_use]
    pub fn new(width: usize, height: usize, pixels: Vec<Color>) -> Self {
        assert_eq!(pixels.len(), width * height, "pixel buffer size mismatch");
        Self {
            width,
            height,
            pixels,
        }
    }

    /// A single-colour image.
    #[must_use]
    pub fn solid(width: usize, height: usize, color: Color) -> Self {
        Self {
            width,
            height,
            pixels: vec![color; width * height],
        }
    }

    /// Pixel at `(x, y)`.
    #[inline]
    #[must_use]
    pub fn get(&self, x: usize, y: usize) -> Color {
        self.pixels[y * self.width + x]
    }

    /// One row of pixels.
    #[inline]
    #[must_use]
    pub fn row(&self, y: usize) -> &[Color] {
        &self.pixels[y * self.width..(y + 1) * self.width]
    }

    /// Copy of the pixels inside `rect`.
    #[must_use]
    pub fn crop(&self, rect: &Rect) -> Self {
        let mut pixels = Vec::with_capacity(rect.area());
        for y in rect.y0..rect.y1 {
            pixels.extend_from_slice(&self.row(y)[rect.x0..rect.x1]);
        }
        Self::new(rect.width(), rect.height(), pixels)
    }

    /// Decode an image file.
    ///
    /// # Errors
    /// Any decoding error from the `image` crate.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, ::image::ImageError> {
        let decoded = ::image::open(path)?.to_rgb8();
        let (w, h) = (decoded.width() as usize, decoded.height() as usize);
        let pixels = decoded
            .as_raw()
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        Ok(Self::new(w, h, pixels))
    }

    /// Encode into memory.
    ///
    /// # Errors
    /// Any encoding error from the `image` crate.
    pub fn encode(&self, encoding: Encoding) -> Result<Vec<u8>, ::image::ImageError> {
        use ::image::{ExtendedColorType, ImageEncoder};
        let raw: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        let (w, h) = (self.width as u32, self.height as u32);
        let mut out = Vec::new();
        match encoding {
            Encoding::Png => ::image::codecs::png::PngEncoder::new(&mut out).write_image(
                &raw,
                w,
                h,
                ExtendedColorType::Rgb8,
            )?,
            Encoding::Jpeg { quality } => {
                ::image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality)
                    .write_image(&raw, w, h, ExtendedColorType::Rgb8)?;
            }
            Encoding::WebPLossless => ::image::codecs::webp::WebPEncoder::new_lossless(&mut out)
                .write_image(&raw, w, h, ExtendedColorType::Rgb8)?,
        }
        Ok(out)
    }

    /// Encode to a file; the format follows the extension.
    ///
    /// # Errors
    /// Any encoding error from the `image` crate.
    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), ::image::ImageError> {
        let raw: Vec<u8> = self.pixels.iter().flatten().copied().collect();
        let buffer = ::image::RgbImage::from_raw(self.width as u32, self.height as u32, raw)
            .ok_or_else(|| {
                ::image::ImageError::Parameter(::image::error::ParameterError::from_kind(
                    ::image::error::ParameterErrorKind::DimensionMismatch,
                ))
            })?;
        buffer.save(path)
    }
}

/// Shrink `img` so its longer side is at most `max_side` (area averaging).
///
/// Returns the (possibly unchanged) image and the scale factor `small / original`.
#[must_use]
pub fn downscale(img: &RgbImage, max_side: usize) -> (RgbImage, f64) {
    let longest = img.width.max(img.height);
    if longest <= max_side {
        return (img.clone(), 1.0);
    }
    let scale = max_side as f64 / longest as f64;
    let new_w = ((img.width as f64 * scale).round_ties_even() as usize).max(1);
    let new_h = ((img.height as f64 * scale).round_ties_even() as usize).max(1);

    // Horizontal pass into f32 planes, then vertical pass.
    let weights_x = box_weights(img.width, new_w);
    let weights_y = box_weights(img.height, new_h);
    let mut tmp = vec![[0f32; 3]; new_w * img.height];
    for y in 0..img.height {
        let row = img.row(y);
        for (x, (start, ws)) in weights_x.iter().enumerate() {
            let mut acc = [0f32; 3];
            for (k, w) in ws.iter().enumerate() {
                let p = row[start + k];
                acc[0] += f32::from(p[0]) * w;
                acc[1] += f32::from(p[1]) * w;
                acc[2] += f32::from(p[2]) * w;
            }
            tmp[y * new_w + x] = acc;
        }
    }
    let mut pixels = vec![[0u8; 3]; new_w * new_h];
    for (y, (start, ws)) in weights_y.iter().enumerate() {
        for x in 0..new_w {
            let mut acc = [0f32; 3];
            for (k, w) in ws.iter().enumerate() {
                let p = tmp[(start + k) * new_w + x];
                acc[0] += p[0] * w;
                acc[1] += p[1] * w;
                acc[2] += p[2] * w;
            }
            pixels[y * new_w + x] = [to_u8(acc[0]), to_u8(acc[1]), to_u8(acc[2])];
        }
    }
    (RgbImage::new(new_w, new_h, pixels), scale)
}

fn to_u8(v: f32) -> u8 {
    (v + 0.5).clamp(0.0, 255.0) as u8
}

/// For each output index, the first source index and the normalised coverage
/// weights of the source pixels it averages over.
fn box_weights(src: usize, dst: usize) -> Vec<(usize, Vec<f32>)> {
    let ratio = src as f64 / dst as f64;
    (0..dst)
        .map(|i| {
            let lo = i as f64 * ratio;
            let hi = ((i + 1) as f64 * ratio).min(src as f64);
            let start = lo.floor() as usize;
            let end = (hi.ceil() as usize).min(src).max(start + 1);
            let mut ws: Vec<f32> = (start..end)
                .map(|s| {
                    let a = (s as f64).max(lo);
                    let b = ((s + 1) as f64).min(hi);
                    (b - a).max(0.0) as f32
                })
                .collect();
            let total: f32 = ws.iter().sum();
            for w in &mut ws {
                *w /= total;
            }
            (start, ws)
        })
        .collect()
}

/// Rec. 601 luma.
#[inline]
#[must_use]
pub fn luminance(c: Color) -> f32 {
    0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2])
}

/// Luma of a single colour as `f64`.
#[inline]
#[must_use]
pub fn color_luma(c: Color) -> f64 {
    0.299 * f64::from(c[0]) + 0.587 * f64::from(c[1]) + 0.114 * f64::from(c[2])
}

/// `max(R,G,B) - min(R,G,B)`; zero for gray pixels.
#[inline]
#[must_use]
pub fn chroma(c: Color) -> u8 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}

/// Maximum channel absolute difference between two colours.
#[inline]
#[must_use]
pub fn dist(a: Color, b: Color) -> u8 {
    a[0].abs_diff(b[0])
        .max(a[1].abs_diff(b[1]))
        .max(a[2].abs_diff(b[2]))
}

/// True when almost no pixel carries colour (manga pages, B/W photos).
#[must_use]
pub fn is_grayscale(img: &RgbImage, chroma_tol: u8, max_color_frac: f64) -> bool {
    let colored = img
        .pixels
        .iter()
        .filter(|&&p| chroma(p) > chroma_tol)
        .count();
    (colored as f64 / img.pixels.len().max(1) as f64) < max_color_frac
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downscale_keeps_small_images() {
        let img = RgbImage::solid(100, 50, [1, 2, 3]);
        let (small, scale) = downscale(&img, 800);
        assert_eq!(small, img);
        assert!((scale - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn downscale_dimensions_and_averaging() {
        let mut img = RgbImage::solid(1280, 745, [0, 0, 0]);
        for (i, p) in img.pixels.iter_mut().enumerate() {
            if i % 1280 >= 640 {
                *p = [200, 100, 0];
            }
        }
        let (small, scale) = downscale(&img, 800);
        assert_eq!((small.width, small.height), (800, 466));
        assert!((scale - 0.625).abs() < 1e-12);
        assert_eq!(small.get(10, 100), [0, 0, 0]);
        assert_eq!(small.get(790, 100), [200, 100, 0]);
    }

    #[test]
    fn encode_roundtrips_through_png_and_webp() {
        let mut img = RgbImage::solid(20, 10, [10, 20, 30]);
        img.pixels[25] = [200, 100, 0];
        let flat: Vec<u8> = img.pixels.iter().flatten().copied().collect();
        for enc in [Encoding::Png, Encoding::WebPLossless] {
            let bytes = img.encode(enc).expect("encode");
            let back = ::image::load_from_memory(&bytes).expect("decode").to_rgb8();
            assert_eq!(back.as_raw(), &flat);
        }
        let jpeg = img.encode(Encoding::Jpeg { quality: 90 }).expect("encode");
        assert!(jpeg.starts_with(&[0xFF, 0xD8]));
    }

    #[test]
    fn distance_and_chroma() {
        assert_eq!(dist([10, 20, 30], [12, 5, 30]), 15);
        assert_eq!(chroma([10, 20, 30]), 20);
        assert_eq!(chroma([7, 7, 7]), 0);
    }
}
