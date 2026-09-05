//! Background estimation, row/column profiles and integral images.
//!
//! These are the only measurements the detector uses: per-pixel comparisons
//! followed by row/column sums or 2-D prefix sums.

use std::ops::{Add, Sub};

use crate::geometry::Rect;
use crate::image::{Color, RgbImage, dist, luminance};
use crate::params::Params;

const QUANT: usize = 16;
const BINS: usize = QUANT * QUANT * QUANT;

/// Colour statistics of the outer band on one side of the image.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Side {
    /// Dominant colour of the band.
    pub color: Color,
    /// Fraction of the band within `bg_tol` of `color`.
    pub flat_frac: f64,
}

impl Side {
    /// True when the band is (nearly) a single colour.
    #[must_use]
    pub fn is_flat(&self, params: &Params) -> bool {
        self.flat_frac >= params.side_flat_min
    }
}

/// Background colour of the chrome and per-side band statistics.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Background {
    /// The chrome background colour.
    pub color: Color,
    /// Top band.
    pub top: Side,
    /// Bottom band.
    pub bottom: Side,
    /// Left band.
    pub left: Side,
    /// Right band.
    pub right: Side,
}

/// 2-D prefix sum with a zero row and column in front: `(H+1) x (W+1)`.
#[derive(Clone, Debug)]
pub struct Integral<T> {
    stride: usize,
    data: Vec<T>,
}

impl<T> Integral<T>
where
    T: Copy + Default + Add<Output = T> + Sub<Output = T>,
{
    /// Build from a per-pixel function over a `width x height` grid.
    pub fn build(width: usize, height: usize, mut f: impl FnMut(usize, usize) -> T) -> Self {
        let stride = width + 1;
        let mut data = vec![T::default(); stride * (height + 1)];
        for y in 0..height {
            let mut row_acc = T::default();
            for x in 0..width {
                row_acc = row_acc + f(x, y);
                data[(y + 1) * stride + x + 1] = data[y * stride + x + 1] + row_acc;
            }
        }
        Self { stride, data }
    }

    /// Sum over `[x0, x1) x [y0, y1)`.
    #[inline]
    #[must_use]
    pub fn sum(&self, x0: usize, y0: usize, x1: usize, y1: usize) -> T {
        let s = self.stride;
        self.data[y1 * s + x1] - self.data[y0 * s + x1] - self.data[y1 * s + x0]
            + self.data[y0 * s + x0]
    }

    /// Sum over a rectangle.
    #[inline]
    #[must_use]
    pub fn rect_sum(&self, r: &Rect) -> T {
        self.sum(r.x0, r.y0, r.x1, r.y1)
    }
}

impl Integral<i32> {
    /// Mean of the mask inside `r` (0 for empty rectangles).
    #[must_use]
    pub fn rect_mean(&self, r: &Rect) -> f64 {
        if r.area() == 0 {
            0.0
        } else {
            f64::from(self.rect_sum(r)) / r.area() as f64
        }
    }
}

impl Integral<f64> {
    /// Mean of the values inside `r` (0 for empty rectangles).
    #[must_use]
    pub fn rect_mean(&self, r: &Rect) -> f64 {
        if r.area() == 0 {
            0.0
        } else {
            self.rect_sum(r) / r.area() as f64
        }
    }
}

/// Row/column profiles and integral images of the (downscaled) image.
#[derive(Clone, Debug)]
pub struct Profiles {
    /// Image width.
    pub width: usize,
    /// Image height.
    pub height: usize,
    /// Fraction of non-background pixels per row.
    pub nonbg_row: Vec<f32>,
    /// Fraction of non-background pixels per column.
    pub nonbg_col: Vec<f32>,
    /// `edge_h[y*W + x]`: row `y` differs from row `y-1` at `x`. Row 0 is all false.
    pub edge_h: Vec<bool>,
    /// `edge_v[y*W + x]`: column `x` differs from column `x-1` at `y`. Column 0 is false.
    pub edge_v: Vec<bool>,
    /// Row-wise prefix sums of `edge_h`: `H x (W+1)`.
    pub edge_h_cum: Vec<i32>,
    /// Column-wise prefix sums of `edge_v`: `(H+1) x W`.
    pub edge_v_cum: Vec<i32>,
    /// Longest contiguous run in `edge_h` per row, as a fraction of the width.
    pub edge_run_row: Vec<f32>,
    /// Longest contiguous run in `edge_v` per column, as a fraction of the height.
    pub edge_run_col: Vec<f32>,
    /// Integral of the non-background mask.
    pub nonbg_int: Integral<i32>,
    /// Integral of the clipped, normalised distance to the background (0..1 per pixel).
    pub dist_int: Integral<f64>,
    /// Integral of "within `bg_tol` of the background" (complement of `nonbg`).
    pub flat_int: Integral<i32>,
    /// Same with `strict_tol`.
    pub strict_flat_int: Integral<i32>,
    /// Integral of luminance, for the manga guard.
    pub luma_int: Integral<f64>,
}

impl Profiles {
    /// `edge_h` prefix sum: number of edge pixels in row `y` left of column `x`.
    #[inline]
    #[must_use]
    pub fn edge_h_cum_at(&self, y: usize, x: usize) -> i32 {
        self.edge_h_cum[y * (self.width + 1) + x]
    }

    /// `edge_v` prefix sum: number of edge pixels in column `x` above row `y`.
    #[inline]
    #[must_use]
    pub fn edge_v_cum_at(&self, y: usize, x: usize) -> i32 {
        self.edge_v_cum[y * self.width + x]
    }
}

// --------------------------------------------------------------------------- helpers

/// Length of the longest run of `true` in a slice.
#[must_use]
pub fn longest_run(values: impl IntoIterator<Item = bool>) -> usize {
    let mut best = 0;
    let mut current = 0;
    for v in values {
        if v {
            current += 1;
            best = best.max(current);
        } else {
            current = 0;
        }
    }
    best
}

fn color_dist(a: Color, b: Color) -> u8 {
    dist(a, b)
}

/// Up to `count` most common colours of a pixel set.
///
/// Colours are found on a coarse quantisation grid and each result is the mean
/// of the pixels in its bin. Bins closer than `min_dist` to an already chosen or
/// excluded colour are skipped, and bins covering less than `min_frac` of the
/// pixels are ignored.
#[must_use]
pub fn dominant_colors<'a>(
    pixels: impl IntoIterator<Item = &'a Color>,
    count: usize,
    min_dist: u8,
    min_frac: f64,
    exclude: &[Color],
) -> Vec<Color> {
    let mut counts = vec![0u32; BINS];
    let mut sums = vec![[0u64; 3]; BINS];
    let mut total = 0usize;
    for &p in pixels {
        let key = (usize::from(p[0]) / QUANT * QUANT + usize::from(p[1]) / QUANT) * QUANT
            + usize::from(p[2]) / QUANT;
        counts[key] += 1;
        sums[key][0] += u64::from(p[0]);
        sums[key][1] += u64::from(p[1]);
        sums[key][2] += u64::from(p[2]);
        total += 1;
    }
    if total == 0 {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..BINS).filter(|&k| counts[k] > 0).collect();
    order.sort_by(|&a, &b| counts[b].cmp(&counts[a]).then(a.cmp(&b)));
    let min_count = min_frac * total as f64;
    let mut result: Vec<Color> = Vec::new();
    for key in order {
        if f64::from(counts[key]) < min_count || result.len() >= count {
            break;
        }
        let n = f64::from(counts[key]);
        let color = [
            (sums[key][0] as f64 / n).round_ties_even() as u8,
            (sums[key][1] as f64 / n).round_ties_even() as u8,
            (sums[key][2] as f64 / n).round_ties_even() as u8,
        ];
        let far = result
            .iter()
            .chain(exclude)
            .all(|&c| color_dist(color, c) > min_dist);
        if far {
            result.push(color);
        }
    }
    result
}

fn side_stats(pixels: &[Color], params: &Params) -> Side {
    let color = dominant_colors(pixels, 1, 0, 0.0, &[])
        .first()
        .copied()
        .unwrap_or([0, 0, 0]);
    let within = pixels
        .iter()
        .filter(|&&p| dist(p, color) <= params.bg_tol)
        .count();
    Side {
        color,
        flat_frac: within as f64 / pixels.len().max(1) as f64,
    }
}

fn frame_pixels(img: &RgbImage, band: usize) -> Vec<Color> {
    let (w, h) = (img.width, img.height);
    let band = band.clamp(1, (h / 2).max(1)).min((w / 2).max(1));
    let mut out = Vec::with_capacity(2 * band * (w + h));
    for y in 0..band {
        out.extend_from_slice(img.row(y));
    }
    for y in h - band..h {
        out.extend_from_slice(img.row(y));
    }
    for y in 0..h {
        let row = img.row(y);
        out.extend_from_slice(&row[..band]);
        out.extend_from_slice(&row[w - band..]);
    }
    out
}

/// Number of rows plus columns (on a 2x subsample) almost entirely within `bg_tol` of `color`.
fn flat_lines(img: &RgbImage, color: Color, params: &Params) -> usize {
    let sw = img.width.div_ceil(2);
    let sh = img.height.div_ceil(2);
    let mut col_hits = vec![0usize; sw];
    let mut rows = 0;
    for sy in 0..sh {
        let row = img.row(sy * 2);
        let mut hits = 0u32;
        for sx in 0..sw {
            if dist(row[sx * 2], color) <= params.bg_tol {
                hits += 1;
                col_hits[sx] += 1;
            }
        }
        if f64::from(hits) / sw as f64 >= params.bg_line_frac {
            rows += 1;
        }
    }
    let cols = col_hits
        .iter()
        .filter(|&&h| h as f64 / sh as f64 >= params.bg_line_frac)
        .count();
    rows + cols
}

// --------------------------------------------------------------------------- public

/// Find the chrome background colour and per-side band statistics.
///
/// The background is chosen among the dominant colours of a wider outer band
/// as the one that forms the most fully flat rows and columns, so a thin
/// outline or status bar does not hide the real chrome colour and the dark
/// tones of a large photo (which never fill whole rows) do not win either.
#[must_use]
pub fn estimate_background(img: &RgbImage, params: &Params) -> Background {
    let (w, h) = (img.width, img.height);
    let band = 2usize.clamp(1, (h / 2).max(1)).min((w / 2).max(1));
    let top: Vec<Color> = (0..band).flat_map(|y| img.row(y).iter().copied()).collect();
    let bottom: Vec<Color> = (h - band..h)
        .flat_map(|y| img.row(y).iter().copied())
        .collect();
    let left: Vec<Color> = (0..h)
        .flat_map(|y| img.row(y)[..band].iter().copied())
        .collect();
    let right: Vec<Color> = (0..h)
        .flat_map(|y| img.row(y)[w - band..].iter().copied())
        .collect();

    let wide = band.max((w.min(h) as f64 * params.bg_band_frac) as usize);
    let mut candidates = dominant_colors(
        &frame_pixels(img, wide),
        params.bg_candidates,
        params.bg_tol,
        0.0,
        &[],
    );
    if candidates.is_empty() {
        candidates.push([0, 0, 0]);
    }
    let mut best = candidates[0];
    let mut best_lines = 0;
    for (i, &c) in candidates.iter().enumerate() {
        let lines = flat_lines(img, c, params);
        if i == 0 || lines > best_lines {
            best = c;
            best_lines = lines;
        }
    }
    Background {
        color: best,
        top: side_stats(&top, params),
        bottom: side_stats(&bottom, params),
        left: side_stats(&left, params),
        right: side_stats(&right, params),
    }
}

/// Compute every profile and integral image the detector needs.
#[must_use]
pub fn compute_profiles(img: &RgbImage, bg: &Background, params: &Params) -> Profiles {
    let (w, h) = (img.width, img.height);
    let n = w * h;

    let dist_bg: Vec<u8> = img.pixels.iter().map(|&p| dist(p, bg.color)).collect();
    let nonbg: Vec<bool> = dist_bg.iter().map(|&d| d > params.bg_tol).collect();

    let mut edge_h = vec![false; n];
    let mut edge_v = vec![false; n];
    for y in 0..h {
        for x in 0..w {
            let p = img.pixels[y * w + x];
            if y > 0 {
                edge_h[y * w + x] = dist(p, img.pixels[(y - 1) * w + x]) > params.edge_tol;
            }
            if x > 0 {
                edge_v[y * w + x] = dist(p, img.pixels[y * w + x - 1]) > params.edge_tol;
            }
        }
    }

    let mut edge_h_cum = vec![0i32; h * (w + 1)];
    for y in 0..h {
        let mut acc = 0;
        for x in 0..w {
            acc += i32::from(edge_h[y * w + x]);
            edge_h_cum[y * (w + 1) + x + 1] = acc;
        }
    }
    let mut edge_v_cum = vec![0i32; (h + 1) * w];
    for x in 0..w {
        let mut acc = 0;
        for y in 0..h {
            acc += i32::from(edge_v[y * w + x]);
            edge_v_cum[(y + 1) * w + x] = acc;
        }
    }

    let edge_run_row: Vec<f32> = (0..h)
        .map(|y| longest_run(edge_h[y * w..(y + 1) * w].iter().copied()) as f32 / w as f32)
        .collect();
    let edge_run_col: Vec<f32> = (0..w)
        .map(|x| longest_run((0..h).map(|y| edge_v[y * w + x])) as f32 / h as f32)
        .collect();

    let mut nonbg_row = vec![0f32; h];
    let mut nonbg_col = vec![0f32; w];
    for y in 0..h {
        for x in 0..w {
            if nonbg[y * w + x] {
                nonbg_row[y] += 1.0;
                nonbg_col[x] += 1.0;
            }
        }
    }
    for v in &mut nonbg_row {
        *v /= w as f32;
    }
    for v in &mut nonbg_col {
        *v /= h as f32;
    }

    let cap = params.dist_cap;
    Profiles {
        width: w,
        height: h,
        nonbg_row,
        nonbg_col,
        edge_h_cum,
        edge_v_cum,
        edge_run_row,
        edge_run_col,
        nonbg_int: Integral::build(w, h, |x, y| i32::from(nonbg[y * w + x])),
        dist_int: Integral::build(w, h, |x, y| {
            f64::from(f32::from(dist_bg[y * w + x].min(cap)) / f32::from(cap))
        }),
        flat_int: Integral::build(w, h, |x, y| i32::from(!nonbg[y * w + x])),
        strict_flat_int: Integral::build(w, h, |x, y| {
            i32::from(dist_bg[y * w + x] <= params.strict_tol)
        }),
        luma_int: Integral::build(w, h, |x, y| f64::from(luminance(img.pixels[y * w + x]))),
        edge_h,
        edge_v,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_run_counts_true_runs() {
        assert_eq!(longest_run([true, true, false, true, true, true]), 3);
        assert_eq!(longest_run([false; 4]), 0);
        assert_eq!(longest_run([true; 6]), 6);
    }

    #[test]
    fn integral_sums_rectangles() {
        let integ = Integral::build(10, 10, |x, y| {
            i32::from((2..6).contains(&y) && (3..8).contains(&x))
        });
        assert_eq!(integ.sum(3, 2, 8, 6), 20);
        assert_eq!(integ.sum(0, 0, 10, 10), 20);
        assert_eq!(integ.sum(0, 0, 3, 10), 0);
        assert!((integ.rect_mean(&Rect::new(0, 0, 10, 10)) - 0.2).abs() < 1e-12);
    }

    #[test]
    fn dominant_colors_orders_and_merges() {
        let mut pixels = vec![[10u8, 10, 10]; 500];
        pixels.extend(vec![[250u8, 250, 250]; 300]);
        pixels.extend(vec![[14u8, 12, 11]; 200]);
        let colors = dominant_colors(&pixels, 3, 18, 0.0, &[]);
        assert!(colors[0][0] < 20);
        assert_eq!(colors[1], [250, 250, 250]);
        assert_eq!(colors.len(), 2);
    }

    #[test]
    fn background_prefers_colour_filling_whole_lines() {
        let mut img = RgbImage::solid(200, 300, [40, 40, 40]);
        for y in [0, 1, 298, 299] {
            for x in 0..200 {
                img.pixels[y * 200 + x] = [0, 0, 0];
            }
        }
        let mut seed = 7u32;
        for y in 90..210 {
            for x in 0..200 {
                seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let v = (seed >> 24) as u8;
                img.pixels[y * 200 + x] = [v, v.wrapping_add(60), 200];
            }
        }
        let bg = estimate_background(&img, &Params::default());
        assert!(bg.color.iter().all(|&c| (i32::from(c) - 40).abs() <= 2));
        assert!(bg.top.is_flat(&Params::default()));
    }
}
