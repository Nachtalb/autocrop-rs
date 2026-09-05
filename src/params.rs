//! Every tunable threshold of the detector.
//!
//! Colour tolerances are on the 0..255 scale and compare the maximum absolute
//! difference over the three RGB channels. Fractions are in `[0, 1]`.

/// Detector thresholds. Start from [`Params::default`] and adjust fields.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Params {
    // --- preprocessing ------------------------------------------------------
    /// Images are downscaled so the longer side is at most this many pixels.
    pub max_side: usize,

    // --- background estimation ---------------------------------------------
    /// Width of the outer band (fraction of the shorter side) supplying background candidates.
    pub bg_band_frac: f64,
    /// How many dominant band colours compete to be the background.
    pub bg_candidates: usize,
    /// A row/column counts as fully covered by a candidate when this fraction matches it.
    pub bg_line_frac: f64,

    // --- colour tolerances --------------------------------------------------
    /// A pixel counts as background when it is within this distance of the background colour.
    pub bg_tol: u8,
    /// Adjacent rows/columns that differ by more than this form an edge.
    pub edge_tol: u8,
    /// Distances to the background colour are clipped to this before averaging.
    pub dist_cap: u8,
    /// Tight colour tolerance: separates truly flat chrome from dark or light painted areas.
    pub strict_tol: u8,

    // --- grayscale / manga guard --------------------------------------------
    /// A pixel is "coloured" when `max(R,G,B) - min(R,G,B)` exceeds this.
    pub gray_chroma_tol: u8,
    /// The image is grayscale when fewer than this fraction of pixels are coloured.
    pub gray_max_color_frac: f64,
    /// Grayscale images with margins lighter than this (manga pages) are never cropped.
    pub light_luma: f64,

    // --- step 3: flat bar trimming ------------------------------------------
    /// Fraction of the outer 2 px band that must match the side colour for a side to be flat.
    pub side_flat_min: f64,
    /// Rows/columns with at most this fraction of non-background pixels belong to the bar.
    pub bar_nonbg_max: f32,
    /// The first row/column after the bar must have at least this fraction of non-bg pixels ...
    pub bar_stop_nonbg_min: f32,
    /// ... or carry a contiguous edge of at least this fraction of its length.
    pub bar_stop_edge_run_min: f32,
    /// Bars thinner than this (in downscaled pixels) are ignored.
    pub min_bar_px: usize,
    /// A bar covering more than this fraction of the image is not a bar (blank image).
    pub max_bar_frac: f64,

    // --- step 4: chrome removal ---------------------------------------------
    /// Minimum fraction of edge pixels along a row/column to be a line candidate.
    pub line_edge_min: f32,
    /// Minimum longest contiguous edge run (fraction of length) for a line candidate.
    pub line_run_min: f32,
    /// Candidate lines closer than this to a stronger one are suppressed.
    pub line_nms_px: usize,
    /// At most this many candidate lines per orientation are kept.
    pub max_lines: usize,
    /// Candidate rectangles must be at least this fraction of the working region in each axis.
    pub min_rect_side_frac: f64,
    /// Candidate rectangles must cover at least this fraction of the working region.
    pub min_rect_area_frac: f64,
    /// Width of the inside/outside strips used for the background-distance contrast.
    pub strip_px: usize,
    /// A mean distance jump of this (fraction of `dist_cap`) gives full side support.
    pub contrast_scale: f64,
    /// Rows/columns whose mean distance jumps by at least this are boundary candidates.
    pub step_min: f64,
    /// Every side of the rectangle needs at least this much support.
    pub side_support_min: f64,
    /// Sides supported at least this well waive the strict outside-flatness gate.
    pub strong_support: f64,
    /// Fraction of inside pixels that are not background (rejects text on a flat page).
    pub inside_nonflat_min: f64,
    /// Fraction of outside pixels that are background (the rest is text, icons, buttons).
    pub outside_flat_min: f64,
    /// Outside non-flat density must be at most this multiple of the inside density.
    pub outside_nonflat_ratio_max: f64,
    /// Fraction of outside pixels within `strict_tol` of the background.
    pub outside_strict_flat_min: f64,
    /// A row/column starts counting as flat once this fraction of it is background;
    /// it counts fully at 100 percent.
    pub flat_line_start: f64,
    /// Maximum mean flat-line weight over the rows and columns inside the rectangle.
    pub inside_flat_lines_max: f64,
    /// Horizontal centre offset allowed, as a fraction of the working width.
    pub center_tol_frac: f64,
    /// Score weight of the weakest side support.
    pub w_support: f64,
    /// Score weight of outside flatness.
    pub w_flat: f64,
    /// Score weight of inside non-flatness.
    pub w_nonflat: f64,
    /// Score weight of the area fraction.
    pub w_area: f64,
    /// Penalty weight for flat lines inside the rectangle.
    pub w_flat_lines: f64,
    /// Rectangles scoring below this are discarded.
    pub accept_score: f64,

    // --- output ----------------------------------------------------------------
    /// A crop must remove at least this fraction of the image area, otherwise nothing is cropped.
    pub min_removed_frac: f64,
}

impl Default for Params {
    fn default() -> Self {
        Self {
            max_side: 800,
            bg_band_frac: 0.02,
            bg_candidates: 4,
            bg_line_frac: 0.95,
            bg_tol: 18,
            edge_tol: 28,
            dist_cap: 64,
            strict_tol: 6,
            gray_chroma_tol: 24,
            gray_max_color_frac: 0.02,
            light_luma: 128.0,
            side_flat_min: 0.97,
            bar_nonbg_max: 0.01,
            bar_stop_nonbg_min: 0.5,
            bar_stop_edge_run_min: 0.5,
            min_bar_px: 3,
            max_bar_frac: 0.9,
            line_edge_min: 0.45,
            line_run_min: 0.35,
            line_nms_px: 3,
            max_lines: 14,
            min_rect_side_frac: 0.12,
            min_rect_area_frac: 0.20,
            strip_px: 3,
            contrast_scale: 0.4,
            step_min: 0.12,
            side_support_min: 0.3,
            strong_support: 0.8,
            inside_nonflat_min: 0.35,
            outside_flat_min: 0.6,
            outside_nonflat_ratio_max: 0.6,
            outside_strict_flat_min: 0.5,
            flat_line_start: 0.5,
            inside_flat_lines_max: 0.15,
            center_tol_frac: 0.06,
            w_support: 0.35,
            w_flat: 0.25,
            w_nonflat: 0.25,
            w_area: 0.15,
            w_flat_lines: 0.5,
            accept_score: 0.6,
            min_removed_frac: 0.01,
        }
    }
}
