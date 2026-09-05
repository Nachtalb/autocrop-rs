//! Step 4: find the content rectangle inside app chrome.
//!
//! Candidate boundaries are rows and columns carrying a long straight edge or a
//! jump in mean background distance. Every combination of two horizontal and
//! two vertical candidates is scored on side support, outside flatness, inside
//! non-flatness, flat lines swallowed inside and area. All measurements come
//! from integral images so each rectangle costs a handful of lookups.

use crate::geometry::Rect;
use crate::params::Params;
use crate::profiles::{Profiles, longest_run};

/// A scored rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    /// The rectangle (downscaled coordinates).
    pub rect: Rect,
    /// Combined score.
    pub score: f64,
    /// Weakest side support.
    pub min_support: f64,
    /// Fraction of outside pixels that are background.
    pub outside_flat: f64,
    /// Fraction of inside pixels that are not background.
    pub inside_nonflat: f64,
    /// Mean flat-line weight inside.
    pub inside_flat_lines: f64,
}

/// Every measurement of one rectangle.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Scores {
    /// Rectangle area / region area.
    pub area_frac: f64,
    /// Support of the top side.
    pub support_top: f64,
    /// Support of the bottom side.
    pub support_bottom: f64,
    /// Support of the left side.
    pub support_left: f64,
    /// Support of the right side.
    pub support_right: f64,
    /// Minimum of the four supports.
    pub min_support: f64,
    /// Fraction of inside pixels that are not background.
    pub inside_nonflat: f64,
    /// Fraction of outside pixels that are not background.
    pub outside_nonflat: f64,
    /// Fraction of outside pixels that are background.
    pub outside_flat: f64,
    /// Fraction of outside pixels within the strict tolerance of the background.
    pub outside_strict_flat: f64,
    /// Mean flat-line weight of the rows and columns inside.
    pub inside_flat_lines: f64,
    /// Horizontally centred (or full width).
    pub centred: bool,
    /// All hard gates passed.
    pub valid: bool,
    /// Combined score, `-1` when invalid.
    pub score: f64,
}

impl Scores {
    fn candidate(&self, rect: Rect) -> Candidate {
        Candidate {
            rect,
            score: self.score,
            min_support: self.min_support,
            outside_flat: self.outside_flat,
            inside_nonflat: self.inside_nonflat,
            inside_flat_lines: self.inside_flat_lines,
        }
    }
}

// --------------------------------------------------------------------------- candidate lines

fn nms(positions: &[usize], strength: &[f32], gap: usize, keep: usize) -> Vec<usize> {
    let mut order: Vec<usize> = positions.to_vec();
    order.sort_by(|&a, &b| {
        strength[b]
            .partial_cmp(&strength[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cmp(&b))
    });
    let mut chosen: Vec<usize> = Vec::new();
    for p in order {
        if chosen.iter().all(|&c| p.abs_diff(c) > gap) {
            chosen.push(p);
            if chosen.len() >= keep {
                break;
            }
        }
    }
    chosen
}

/// Absolute jump of a profile: mean of the next `strip` entries minus the previous.
///
/// `cum` is the prefix sum (with a leading zero) of the per-line mean distance.
fn step_profile(cum: &[f64], strip: usize) -> Vec<f64> {
    let n = cum.len() - 1;
    (0..n)
        .map(|idx| {
            let lo = idx.saturating_sub(strip);
            let hi = (idx + strip).min(n);
            let before = (cum[idx] - cum[lo]) / (idx - lo).max(1) as f64;
            let after = (cum[hi] - cum[idx]) / (hi - idx).max(1) as f64;
            (after - before).abs()
        })
        .collect()
}

fn local_maxima(values: &[f64], minimum: f64) -> Vec<usize> {
    let n = values.len();
    (0..n)
        .filter(|&i| {
            values[i] >= minimum
                && (i == 0 || values[i] >= values[i - 1])
                && (i + 1 == n || values[i] > values[i + 1])
        })
        .collect()
}

fn candidates_1d(
    frac: &[f32],
    run: &[f32],
    step: &[f64],
    origin: usize,
    end: usize,
    params: &Params,
) -> Vec<usize> {
    let n = frac.len();
    let strength: Vec<f32> = (0..n)
        .map(|i| (frac[i] + run[i]).max((step[i] / params.contrast_scale) as f32))
        .collect();
    let mut ok: Vec<bool> = (0..n)
        .map(|i| frac[i] >= params.line_edge_min && run[i] >= params.line_run_min)
        .collect();
    for i in local_maxima(step, params.step_min) {
        ok[i] = true;
    }
    ok[0] = false;
    let positions: Vec<usize> = (0..n).filter(|&i| ok[i]).collect();
    let mut lines: Vec<usize> = nms(&positions, &strength, params.line_nms_px, params.max_lines)
        .into_iter()
        .map(|i| origin + i)
        .collect();
    lines.push(origin);
    lines.push(end);
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// Rows and columns inside `region` that look like a content boundary (sorted, bounds included).
#[must_use]
pub fn line_candidates(
    prof: &Profiles,
    region: &Rect,
    params: &Params,
) -> (Vec<usize>, Vec<usize>) {
    let (w, h) = (region.width(), region.height());
    let stride = prof.width;

    let mut frac_h = vec![0f32; h];
    let mut run_h = vec![0f32; h];
    let mut frac_v = vec![0f32; w];
    let mut col_run_current = vec![0usize; w];
    let mut col_run_best = vec![0usize; w];
    for (i, y) in (region.y0..region.y1).enumerate() {
        let row = &prof.edge_h[y * stride + region.x0..y * stride + region.x1];
        frac_h[i] = row.iter().filter(|&&e| e).count() as f32 / w as f32;
        run_h[i] = longest_run(row.iter().copied()) as f32 / w as f32;
        for (j, x) in (region.x0..region.x1).enumerate() {
            if prof.edge_v[y * stride + x] {
                frac_v[j] += 1.0;
                col_run_current[j] += 1;
                col_run_best[j] = col_run_best[j].max(col_run_current[j]);
            } else {
                col_run_current[j] = 0;
            }
        }
    }
    for v in &mut frac_v {
        *v /= h as f32;
    }
    let run_v: Vec<f32> = col_run_best.iter().map(|&r| r as f32 / h as f32).collect();

    // Prefix sums (leading zero) of the per-line mean distance inside the region.
    let d = &prof.dist_int;
    let row_cum: Vec<f64> = (region.y0..=region.y1)
        .map(|y| d.sum(region.x0, region.y0, region.x1, y) / w as f64)
        .collect();
    let col_cum: Vec<f64> = (region.x0..=region.x1)
        .map(|x| d.sum(region.x0, region.y0, x, region.y1) / h as f64)
        .collect();
    let step_h = step_profile(&row_cum, params.strip_px);
    let step_v = step_profile(&col_cum, params.strip_px);

    let ys = candidates_1d(&frac_h, &run_h, &step_h, region.y0, region.y1, params);
    let xs = candidates_1d(&frac_v, &run_v, &step_v, region.x0, region.x1, params);
    (ys, xs)
}

// --------------------------------------------------------------------------- scoring

fn pairs(values: &[usize], min_len: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    for &a in values {
        for &b in values {
            if b >= a + min_len {
                out.push((a, b));
            }
        }
    }
    out
}

/// Prefix sums of per-line flatness weights for one span.
///
/// A line's weight rises linearly from 0 at `start_frac` background coverage
/// to 1 at full coverage. For `along_rows` the span is a column interval and
/// the result has `H+1` entries; otherwise it is a row interval with `W+1`.
fn flat_line_prefix(
    prof: &Profiles,
    start: usize,
    end: usize,
    along_rows: bool,
    start_frac: f64,
) -> Vec<f64> {
    let length = (end - start) as f64;
    let n = if along_rows { prof.height } else { prof.width };
    let mut out = Vec::with_capacity(n + 1);
    out.push(0.0);
    let mut acc = 0.0;
    for i in 0..n {
        let sum = if along_rows {
            prof.flat_int.sum(start, i, end, i + 1)
        } else {
            prof.flat_int.sum(i, start, i + 1, end)
        };
        let coverage = f64::from(sum) / length;
        acc += ((coverage - start_frac) / (1.0 - start_frac)).clamp(0.0, 1.0);
        out.push(acc);
    }
    out
}

/// Scored search over candidate rectangles inside a region.
pub struct Search<'a> {
    prof: &'a Profiles,
    region: Rect,
    params: &'a Params,
    /// Row intervals `(y0, y1)`.
    pub ys: Vec<(usize, usize)>,
    /// Column intervals `(x0, x1)`.
    pub xs: Vec<(usize, usize)>,
    row_prefix: Vec<Vec<f64>>,
    col_prefix: Vec<Vec<f64>>,
    flat_region: f64,
    strict_region: f64,
}

impl<'a> Search<'a> {
    /// Prepare a search over explicit row and column intervals.
    #[must_use]
    pub fn new(
        prof: &'a Profiles,
        region: Rect,
        ys: Vec<(usize, usize)>,
        xs: Vec<(usize, usize)>,
        params: &'a Params,
    ) -> Self {
        let row_prefix = xs
            .iter()
            .map(|&(x0, x1)| flat_line_prefix(prof, x0, x1, true, params.flat_line_start))
            .collect();
        let col_prefix = ys
            .iter()
            .map(|&(y0, y1)| flat_line_prefix(prof, y0, y1, false, params.flat_line_start))
            .collect();
        Self {
            prof,
            region,
            params,
            ys,
            xs,
            row_prefix,
            col_prefix,
            flat_region: f64::from(prof.flat_int.rect_sum(&region)),
            strict_region: f64::from(prof.strict_flat_int.rect_sum(&region)),
        }
    }

    /// Prepare a search over every combination of the candidate lines of `region`.
    #[must_use]
    pub fn over_candidates(prof: &'a Profiles, region: Rect, params: &'a Params) -> Self {
        let (ys, xs) = line_candidates(prof, &region, params);
        let min_h = ((region.height() as f64 * params.min_rect_side_frac) as usize).max(1);
        let min_w = ((region.width() as f64 * params.min_rect_side_frac) as usize).max(1);
        Self::new(prof, region, pairs(&ys, min_h), pairs(&xs, min_w), params)
    }

    /// The rectangle at `(row interval i, column interval j)`.
    #[must_use]
    pub fn rect(&self, i: usize, j: usize) -> Rect {
        Rect::new(self.xs[j].0, self.ys[i].0, self.xs[j].1, self.ys[i].1)
    }

    fn support_h(&self, y: usize, x0: usize, x1: usize, inward: bool) -> f64 {
        let p = self.prof;
        let r = &self.region;
        let width = (x1 - x0) as f64;
        let y_row = y.min(p.height - 1);
        let edge = f64::from(p.edge_h_cum_at(y_row, x1) - p.edge_h_cum_at(y_row, x0)) / width;
        let s = self.params.strip_px;
        let (in_y0, in_y1, out_y0, out_y1) = if inward {
            (y, (y + s).min(r.y1), y.saturating_sub(s).max(r.y0), y)
        } else {
            (y.saturating_sub(s).max(r.y0), y, y, (y + s).min(r.y1))
        };
        let in_area = ((in_y1 - in_y0) * (x1 - x0)).max(1) as f64;
        let out_area = ((out_y1 - out_y0) * (x1 - x0)).max(1) as f64;
        let inside = p.dist_int.sum(x0, in_y0, x1, in_y1) / in_area;
        let outside = p.dist_int.sum(x0, out_y0, x1, out_y1) / out_area;
        let contrast = (inside - outside) / self.params.contrast_scale;
        edge.max(contrast).clamp(0.0, 1.0)
    }

    fn support_v(&self, x: usize, y0: usize, y1: usize, inward: bool) -> f64 {
        let p = self.prof;
        let r = &self.region;
        let height = (y1 - y0) as f64;
        let x_col = x.min(p.width - 1);
        let edge = f64::from(p.edge_v_cum_at(y1, x_col) - p.edge_v_cum_at(y0, x_col)) / height;
        let s = self.params.strip_px;
        let (in_x0, in_x1, out_x0, out_x1) = if inward {
            (x, (x + s).min(r.x1), x.saturating_sub(s).max(r.x0), x)
        } else {
            (x.saturating_sub(s).max(r.x0), x, x, (x + s).min(r.x1))
        };
        let in_area = ((in_x1 - in_x0) * (y1 - y0)).max(1) as f64;
        let out_area = ((out_x1 - out_x0) * (y1 - y0)).max(1) as f64;
        let inside = p.dist_int.sum(in_x0, y0, in_x1, y1) / in_area;
        let outside = p.dist_int.sum(out_x0, y0, out_x1, y1) / out_area;
        let contrast = (inside - outside) / self.params.contrast_scale;
        edge.max(contrast).clamp(0.0, 1.0)
    }

    /// Score the rectangle at `(i, j)`.
    #[must_use]
    pub fn score(&self, i: usize, j: usize) -> Scores {
        let pr = self.params;
        let r = &self.region;
        let (y0, y1) = self.ys[i];
        let (x0, x1) = self.xs[j];
        let area = ((y1 - y0) * (x1 - x0)) as f64;
        let region_area = r.area() as f64;
        let area_frac = area / region_area;

        let support_top = if y0 == r.y0 {
            1.0
        } else {
            self.support_h(y0, x0, x1, true)
        };
        let support_bottom = if y1 == r.y1 {
            1.0
        } else {
            self.support_h(y1, x0, x1, false)
        };
        let support_left = if x0 == r.x0 {
            1.0
        } else {
            self.support_v(x0, y0, y1, true)
        };
        let support_right = if x1 == r.x1 {
            1.0
        } else {
            self.support_v(x1, y0, y1, false)
        };
        let min_support = support_top
            .min(support_bottom)
            .min(support_left)
            .min(support_right);

        let flat_inside = f64::from(self.prof.flat_int.sum(x0, y0, x1, y1));
        let outside_area = (region_area - area).max(1.0);
        let flat_out = (self.flat_region - flat_inside) / outside_area;
        let nonflat_in = 1.0 - flat_inside / area;
        let nonflat_out = 1.0 - flat_out;

        let strict_inside = f64::from(self.prof.strict_flat_int.sum(x0, y0, x1, y1));
        let strict_out = (self.strict_region - strict_inside) / outside_area;

        let rp = &self.row_prefix[j];
        let cp = &self.col_prefix[i];
        let flat_rows = (rp[y1] - rp[y0]) / (y1 - y0) as f64;
        let flat_cols = (cp[x1] - cp[x0]) / (x1 - x0) as f64;
        let flat_lines = flat_rows.max(flat_cols);

        let centre_off = ((x0 + x1) as f64 / 2.0 - (r.x0 + r.x1) as f64 / 2.0).abs();
        let centred =
            centre_off <= pr.center_tol_frac * r.width() as f64 || (x0 == r.x0 && x1 == r.x1);

        let valid = area_frac >= pr.min_rect_area_frac
            && area_frac < 1.0
            && min_support >= pr.side_support_min
            && nonflat_in >= pr.inside_nonflat_min
            && flat_out >= pr.outside_flat_min
            && (strict_out >= pr.outside_strict_flat_min || min_support >= pr.strong_support)
            && nonflat_out <= pr.outside_nonflat_ratio_max * nonflat_in
            && flat_lines <= pr.inside_flat_lines_max
            && centred;
        let raw = pr.w_support * min_support
            + pr.w_flat * flat_out
            + pr.w_nonflat * nonflat_in
            + pr.w_area * area_frac
            - pr.w_flat_lines * flat_lines;
        Scores {
            area_frac,
            support_top,
            support_bottom,
            support_left,
            support_right,
            min_support,
            inside_nonflat: nonflat_in,
            outside_nonflat: nonflat_out,
            outside_flat: flat_out,
            outside_strict_flat: strict_out,
            inside_flat_lines: flat_lines,
            centred,
            valid,
            score: if valid { raw } else { -1.0 },
        }
    }

    /// Best valid rectangle, if any.
    #[must_use]
    pub fn best(&self) -> Option<Candidate> {
        let mut best: Option<Candidate> = None;
        for i in 0..self.ys.len() {
            for j in 0..self.xs.len() {
                let s = self.score(i, j);
                if s.valid && best.is_none_or(|b| s.score > b.score) {
                    best = Some(s.candidate(self.rect(i, j)));
                }
            }
        }
        best
    }
}

/// Score a single rectangle inside `region` (diagnostics).
#[must_use]
pub fn explain_rect(prof: &Profiles, region: &Rect, rect: &Rect, params: &Params) -> Scores {
    Search::new(
        prof,
        *region,
        vec![(rect.y0, rect.y1)],
        vec![(rect.x0, rect.x1)],
        params,
    )
    .score(0, 0)
}

/// Best-scoring content rectangle strictly smaller than `region`, or `None`.
#[must_use]
pub fn find_content_rect(prof: &Profiles, region: &Rect, params: &Params) -> Option<Candidate> {
    let search = Search::over_candidates(prof, *region, params);
    if search.ys.is_empty() || search.xs.is_empty() {
        return None;
    }
    search.best()
}
