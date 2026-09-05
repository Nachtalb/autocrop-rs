//! End-to-end detector behaviour on synthetic layouts (mirrors the Python test suite).

use autocrop::image::is_grayscale;
use autocrop::letterbox::trim_flat_bars;
use autocrop::profiles::{compute_profiles, estimate_background};
use autocrop::{Params, Rect, RgbImage, find_crop, iou};

type Color = [u8; 3];

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u32 {
        // xorshift64*
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        (self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 32) as u32
    }

    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + self.next() % (hi - lo)
    }
}

/// Photo-like content: coloured noise with a smooth gradient underneath.
fn noise(width: usize, height: usize, seed: u64) -> RgbImage {
    let mut rng = Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    let mut pixels = Vec::with_capacity(width * height);
    for _ in 0..height {
        for x in 0..width {
            let ramp = (60 * x / width.max(1)) as u32;
            let c = |rng: &mut Rng| (rng.range(40, 220) + ramp).min(255) as u8;
            pixels.push([c(&mut rng), c(&mut rng), c(&mut rng)]);
        }
    }
    RgbImage::new(width, height, pixels)
}

fn paste(canvas: &mut RgbImage, content: &RgbImage, x: usize, y: usize) {
    for cy in 0..content.height {
        for cx in 0..content.width {
            canvas.pixels[(y + cy) * canvas.width + x + cx] = content.get(cx, cy);
        }
    }
}

/// Fake text lines: short bright dashes on every other row band.
fn sprinkle_text(canvas: &mut RgbImage, y0: usize, y1: usize, color: Color, seed: u64) {
    let mut rng = Rng(seed | 1);
    let (w, h) = (canvas.width, canvas.height);
    let mut y = y0 + 4;
    while y + 6 < y1.min(h) {
        let mut x = 8;
        while x + 12 < w - 8 {
            let length = rng.range(4, 12) as usize;
            for yy in y..y + 6 {
                for xx in x..x + length {
                    canvas.pixels[yy * w + xx] = color;
                }
            }
            x += length + rng.range(3, 8) as usize;
        }
        y += 14;
    }
}

fn assert_iou(rect: Option<Rect>, truth: Rect, min: f64) {
    let rect = rect.expect("a crop rectangle");
    let score = iou(&rect, &truth);
    assert!(score > min, "iou {score:.3} for {rect:?} vs {truth:?}");
}

fn letterboxed() -> (RgbImage, Rect) {
    let mut canvas = RgbImage::solid(300, 500, [0, 0, 0]);
    paste(&mut canvas, &noise(300, 260, 1), 0, 120);
    (canvas, Rect::new(0, 120, 300, 380))
}

fn meme() -> (RgbImage, Rect) {
    let mut canvas = RgbImage::solid(400, 640, [0, 0, 0]);
    sprinkle_text(&mut canvas, 0, 200, [255, 255, 255], 1);
    paste(&mut canvas, &noise(400, 420, 3), 0, 220);
    (canvas, Rect::new(0, 220, 400, 640))
}

fn phone_viewer() -> (RgbImage, Rect) {
    let mut canvas = RgbImage::solid(360, 780, [0, 0, 0]);
    sprinkle_text(&mut canvas, 0, 40, [230, 230, 230], 1);
    paste(&mut canvas, &noise(360, 400, 5), 0, 190);
    for x in [30usize, 160, 290] {
        for y in 700..740 {
            for xx in x..x + 40 {
                canvas.pixels[y * 360 + xx] = [60, 60, 60];
            }
        }
    }
    (canvas, Rect::new(0, 190, 360, 590))
}

fn inset_card() -> (RgbImage, Rect) {
    let mut canvas = RgbImage::solid(500, 420, [2, 2, 2]);
    sprinkle_text(&mut canvas, 0, 50, [240, 240, 240], 1);
    paste(&mut canvas, &noise(440, 300, 7), 30, 60);
    (canvas, Rect::new(30, 60, 470, 360))
}

fn trim(img: &RgbImage) -> (Rect, autocrop::letterbox::BarReasons) {
    let params = Params::default();
    let bg = estimate_background(img, &params);
    let prof = compute_profiles(img, &bg, &params);
    let gray = is_grayscale(img, params.gray_chroma_tol, params.gray_max_color_frac);
    trim_flat_bars(img, &bg, &prof, gray, &params)
}

#[test]
fn letterbox_bars_are_trimmed_exactly() {
    let (img, truth) = letterboxed();
    let (rect, reasons) = trim(&img);
    assert_eq!(rect, truth);
    assert_eq!(reasons.top, "bar");
    assert_eq!(reasons.bottom, "bar");
    assert_eq!(reasons.left, "not-flat");
}

#[test]
fn pillarbox_and_white_margins() {
    let mut canvas = RgbImage::solid(400, 200, [255, 255, 255]);
    paste(&mut canvas, &noise(240, 200, 2), 80, 0);
    let (rect, _) = trim(&canvas);
    assert_eq!(rect, Rect::new(80, 0, 320, 200));
}

#[test]
fn soft_glow_is_not_trimmed() {
    let mut img = RgbImage::solid(300, 300, [0, 0, 0]);
    for y in 0..300 {
        for x in 0..300 {
            let r = (((y as f64 - 150.0).powi(2) + (x as f64 - 150.0).powi(2)).sqrt() * 1.8) as i32;
            let glow = (200 - r).clamp(0, 255) as u8;
            img.pixels[y * 300 + x] = [glow / 3, glow / 2, glow];
        }
    }
    let (rect, reasons) = trim(&img);
    assert_eq!(rect, Rect::new(0, 0, 300, 300));
    assert_eq!(reasons.top, "soft-edge");
}

#[test]
fn grayscale_light_margins_are_kept() {
    let mut img = RgbImage::solid(300, 400, [255, 255, 255]);
    let mut rng = Rng(11);
    for y in 30..370 {
        for x in 20..280 {
            let v = rng.range(0, 200) as u8;
            img.pixels[y * 300 + x] = [v, v, v];
        }
    }
    let (rect, reasons) = trim(&img);
    assert_eq!(rect, Rect::new(0, 0, 300, 400));
    assert_eq!(reasons.top, "grayscale-light");
    assert!(find_crop(&img, &Params::default()).rect.is_none());
}

#[test]
fn blank_image_is_untouched() {
    let (rect, reasons) = trim(&RgbImage::solid(100, 80, [0, 0, 0]));
    assert_eq!(rect, Rect::new(0, 0, 100, 80));
    assert_eq!(reasons.top, "blank");
}

#[test]
fn meme_text_above_photo() {
    let (img, truth) = meme();
    let result = find_crop(&img, &Params::default());
    assert_eq!(result.reason, "chrome");
    assert_iou(result.rect, truth, 0.95);
}

#[test]
fn phone_viewer_with_chrome_above_and_below() {
    let (img, truth) = phone_viewer();
    let result = find_crop(&img, &Params::default());
    assert_eq!(result.reason, "chrome");
    assert_iou(result.rect, truth, 0.95);
}

#[test]
fn inset_card_on_all_sides() {
    let (img, truth) = inset_card();
    let result = find_crop(&img, &Params::default());
    assert_eq!(result.reason, "chrome");
    assert_iou(result.rect, truth, 0.95);
}

#[test]
fn letterbox_only_falls_back_to_bar_trim() {
    let (img, truth) = letterboxed();
    let result = find_crop(&img, &Params::default());
    assert_eq!(result.rect, Some(truth));
    assert_eq!(result.reason, "letterbox");
}

#[test]
fn plain_photo_is_not_cropped() {
    assert!(
        find_crop(&noise(640, 480, 11), &Params::default())
            .rect
            .is_none()
    );
}

#[test]
fn text_page_is_not_cropped() {
    let mut page = RgbImage::solid(600, 900, [255, 255, 255]);
    for p in page.pixels.iter_mut().take(600 * 80) {
        *p = [20, 40, 90];
    }
    sprinkle_text(&mut page, 100, 850, [30, 30, 30], 1);
    assert!(find_crop(&page, &Params::default()).rect.is_none());
}

#[test]
fn stacked_frames_are_not_cropped() {
    let mut img = RgbImage::solid(400, 1000, [0, 0, 0]);
    for (i, seed) in (0..5).enumerate() {
        paste(&mut img, &noise(400, 200, seed as u64), 0, i * 200);
    }
    assert!(find_crop(&img, &Params::default()).rect.is_none());
}

#[test]
fn prefers_outer_viewport_over_inner_dialog() {
    let mut canvas = RgbImage::solid(800, 400, [0, 0, 0]);
    let mut game = noise(500, 400, 2);
    for y in 250..380 {
        for x in 40..460 {
            game.pixels[y * 500 + x] = [230, 240, 230];
        }
    }
    paste(&mut canvas, &game, 150, 0);
    let result = find_crop(&canvas, &Params::default());
    assert_eq!(result.rect, Some(Rect::new(150, 0, 650, 400)));
}

#[test]
fn downscaled_boxes_map_back_to_original_pixels() {
    let (img, truth) = meme();
    let mut big = RgbImage::solid(img.width * 3, img.height * 3, [0, 0, 0]);
    for y in 0..big.height {
        for x in 0..big.width {
            big.pixels[y * big.width + x] = img.get(x / 3, y / 3);
        }
    }
    let result = find_crop(&big, &Params::default());
    let scaled = Rect::new(truth.x0 * 3, truth.y0 * 3, truth.x1 * 3, truth.y1 * 3);
    assert_iou(result.rect, scaled, 0.95);
}
