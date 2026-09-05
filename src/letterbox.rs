//! Step 3: trim flat bars (letterbox, pillarbox, plain margins) from the image edges.
//!
//! A bar is a run of rows (or columns) starting at an image edge whose pixels
//! all match the colour of that edge. The trim is only accepted when the bar
//! ends at a real content edge, so soft glows fading into a background are
//! left alone.

use crate::geometry::Rect;
use crate::image::{RgbImage, color_luma, dist};
use crate::params::Params;
use crate::profiles::{Background, Profiles, Side};

/// Why each side was or was not trimmed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BarReasons {
    /// Top side.
    pub top: &'static str,
    /// Bottom side.
    pub bottom: &'static str,
    /// Left side.
    pub left: &'static str,
    /// Right side.
    pub right: &'static str,
}

struct Walk {
    pixels: usize,
    reason: &'static str,
}

/// Walk inward from index 0 while rows/columns are flat.
///
/// `nonbg[i]` is the fraction of pixels in line `i` (counted from the side being
/// walked) that differ from the side colour; `edge_run[i]` is the longest
/// contiguous edge between `i` and `i-1` as a fraction of its length.
fn walk(nonbg: &[f32], edge_run: &[f32], limit: usize, params: &Params) -> Walk {
    let n = nonbg.len();
    let mut i = 0;
    while i < n && nonbg[i] <= params.bar_nonbg_max {
        i += 1;
    }
    if i < params.min_bar_px {
        return Walk {
            pixels: 0,
            reason: "no-bar",
        };
    }
    if i >= n || i > limit {
        return Walk {
            pixels: 0,
            reason: "blank",
        };
    }
    let stop_is_content = nonbg[i] >= params.bar_stop_nonbg_min;
    let stop_is_edge = edge_run[i] >= params.bar_stop_edge_run_min;
    if !(stop_is_content || stop_is_edge) {
        return Walk {
            pixels: 0,
            reason: "soft-edge",
        };
    }
    Walk {
        pixels: i,
        reason: "bar",
    }
}

/// Fraction of pixels per row that differ from `side.color`.
fn row_nonbg(img: &RgbImage, side: &Side, params: &Params) -> Vec<f32> {
    (0..img.height)
        .map(|y| {
            let hits = img
                .row(y)
                .iter()
                .filter(|&&p| dist(p, side.color) > params.bg_tol)
                .count();
            hits as f32 / img.width as f32
        })
        .collect()
}

/// Fraction of pixels per column that differ from `side.color`.
fn col_nonbg(img: &RgbImage, side: &Side, params: &Params) -> Vec<f32> {
    let mut hits = vec![0u32; img.width];
    for y in 0..img.height {
        for (x, &p) in img.row(y).iter().enumerate() {
            if dist(p, side.color) > params.bg_tol {
                hits[x] += 1;
            }
        }
    }
    hits.iter().map(|&h| h as f32 / img.height as f32).collect()
}

fn light_guard(side: &Side, grayscale: bool, params: &Params) -> bool {
    grayscale && color_luma(side.color) > params.light_luma
}

fn walk_side(
    side: &Side,
    nonbg: &[f32],
    run: &[f32],
    limit: usize,
    grayscale: bool,
    params: &Params,
) -> (usize, &'static str) {
    if !side.is_flat(params) {
        return (0, "not-flat");
    }
    if light_guard(side, grayscale, params) {
        return (0, "grayscale-light");
    }
    let w = walk(nonbg, run, limit, params);
    (w.pixels, w.reason)
}

/// Reversed edge-run profile for walking from the far side: entry `i` is the
/// run between line `n-1-i` and `n-i`, with a zero at the end.
fn reversed_runs(run: &[f32]) -> Vec<f32> {
    let mut out: Vec<f32> = run[1..].iter().rev().copied().collect();
    out.push(0.0);
    out
}

/// The rectangle left after trimming flat bars, plus a per-side reason.
#[must_use]
pub fn trim_flat_bars(
    img: &RgbImage,
    bg: &Background,
    prof: &Profiles,
    grayscale: bool,
    params: &Params,
) -> (Rect, BarReasons) {
    let (w, h) = (img.width, img.height);
    let limit_y = (h as f64 * params.max_bar_frac) as usize;
    let limit_x = (w as f64 * params.max_bar_frac) as usize;

    let run_row_rev = reversed_runs(&prof.edge_run_row);
    let run_col_rev = reversed_runs(&prof.edge_run_col);

    let row_top = row_nonbg(img, &bg.top, params);
    let row_bottom = if bg.bottom.color == bg.top.color {
        row_top.clone()
    } else {
        row_nonbg(img, &bg.bottom, params)
    };
    let col_left = col_nonbg(img, &bg.left, params);
    let col_right = if bg.right.color == bg.left.color {
        col_left.clone()
    } else {
        col_nonbg(img, &bg.right, params)
    };
    let row_bottom_rev: Vec<f32> = row_bottom.iter().rev().copied().collect();
    let col_right_rev: Vec<f32> = col_right.iter().rev().copied().collect();

    let (top, r_top) = walk_side(
        &bg.top,
        &row_top,
        &prof.edge_run_row,
        limit_y,
        grayscale,
        params,
    );
    let (bottom, r_bottom) = walk_side(
        &bg.bottom,
        &row_bottom_rev,
        &run_row_rev,
        limit_y,
        grayscale,
        params,
    );
    let (left, r_left) = walk_side(
        &bg.left,
        &col_left,
        &prof.edge_run_col,
        limit_x,
        grayscale,
        params,
    );
    let (right, r_right) = walk_side(
        &bg.right,
        &col_right_rev,
        &run_col_rev,
        limit_x,
        grayscale,
        params,
    );

    let (y0, y1, x0, x1) = (top, h - bottom, left, w - right);
    if y1 < y0 + params.min_bar_px || x1 < x0 + params.min_bar_px {
        let d = "degenerate";
        return (
            Rect::new(0, 0, w, h),
            BarReasons {
                top: d,
                bottom: d,
                left: d,
                right: d,
            },
        );
    }
    (
        Rect::new(x0, y0, x1, y1),
        BarReasons {
            top: r_top,
            bottom: r_bottom,
            left: r_left,
            right: r_right,
        },
    )
}
