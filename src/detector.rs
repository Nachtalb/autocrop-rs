//! Top-level detector: wires preprocessing, bar trimming and chrome removal together.

use std::path::Path;

use crate::chrome::{Candidate, find_content_rect};
use crate::geometry::Rect;
use crate::image::{RgbImage, downscale, is_grayscale};
use crate::letterbox::{BarReasons, trim_flat_bars};
use crate::params::Params;
use crate::profiles::{Background, Profiles, compute_profiles, estimate_background};

/// Result of [`find_crop`].
#[derive(Clone, Debug, PartialEq)]
pub struct CropResult {
    /// Crop rectangle in original pixels, or `None` when nothing should be cropped.
    pub rect: Option<Rect>,
    /// Score of the decision (stage dependent).
    pub score: f64,
    /// Which stage produced the decision: `letterbox`, `chrome`, `no-crop`, `no-crop:...`.
    pub reason: String,
}

/// Everything the detector computed, for diagnostics.
#[derive(Clone, Debug)]
pub struct Analysis {
    /// The downscaled working image.
    pub small: RgbImage,
    /// `small / original`.
    pub scale: f64,
    /// Background estimate.
    pub background: Background,
    /// Profiles of the working image.
    pub profiles: Profiles,
    /// Whether the image is grayscale.
    pub grayscale: bool,
    /// Region left after flat bar trimming.
    pub bar_rect: Rect,
    /// Per-side bar trimming reasons.
    pub bar_reasons: BarReasons,
    /// Best chrome-removal candidate, if any.
    pub candidate: Option<Candidate>,
    /// The decision.
    pub result: CropResult,
}

fn finalize(
    rect: &Rect,
    scale: f64,
    img: &RgbImage,
    score: f64,
    reason: &str,
    params: &Params,
) -> CropResult {
    let full_area = (img.width * img.height) as f64;
    let boxed = rect.scaled(1.0 / scale, img.width, img.height);
    let removed = 1.0 - boxed.area() as f64 / full_area;
    if removed < params.min_removed_frac {
        return CropResult {
            rect: None,
            score,
            reason: format!("no-crop:removes-{removed:.3}"),
        };
    }
    CropResult {
        rect: Some(boxed),
        score,
        reason: reason.to_string(),
    }
}

/// Mean luminance of `region` minus `rect` is above the light threshold.
fn outside_is_light(prof: &Profiles, region: &Rect, rect: &Rect, params: &Params) -> bool {
    let total = prof.luma_int.rect_sum(region);
    let inside = prof.luma_int.rect_sum(rect);
    let outside_area = region.area() as f64 - rect.area() as f64;
    if outside_area <= 0.0 {
        return false;
    }
    (total - inside) / outside_area > params.light_luma
}

/// Run the full detector and keep all intermediate data.
#[must_use]
pub fn analyze(img: &RgbImage, params: &Params) -> Analysis {
    let (small, scale) = downscale(img, params.max_side);
    let bg = estimate_background(&small, params);
    let prof = compute_profiles(&small, &bg, params);
    let grayscale = is_grayscale(&small, params.gray_chroma_tol, params.gray_max_color_frac);
    let (bar_rect, bar_reasons) = trim_flat_bars(&small, &bg, &prof, grayscale, params);

    let candidate = find_content_rect(&prof, &bar_rect, params);
    let result = match candidate {
        Some(c) if c.score >= params.accept_score => {
            if grayscale && outside_is_light(&prof, &bar_rect, &c.rect, params) {
                CropResult {
                    rect: None,
                    score: c.score,
                    reason: "no-crop:grayscale-light".into(),
                }
            } else {
                finalize(&c.rect, scale, img, c.score, "chrome", params)
            }
        }
        _ if bar_rect.area() < small.width * small.height => {
            let nonflat = 1.0 - prof.flat_int.rect_mean(&bar_rect);
            if nonflat < params.inside_nonflat_min {
                CropResult {
                    rect: None,
                    score: nonflat,
                    reason: "no-crop:flat-inside".into(),
                }
            } else {
                finalize(&bar_rect, scale, img, nonflat, "letterbox", params)
            }
        }
        _ => CropResult {
            rect: None,
            score: 0.0,
            reason: "no-crop".into(),
        },
    };

    Analysis {
        small,
        scale,
        background: bg,
        profiles: prof,
        grayscale,
        bar_rect,
        bar_reasons,
        candidate,
        result,
    }
}

/// Detect the content rectangle of an image.
#[must_use]
pub fn find_crop(img: &RgbImage, params: &Params) -> CropResult {
    analyze(img, params).result
}

/// Load an image file and return it cropped (or unchanged) with the detection result.
///
/// # Errors
/// Any decoding error from the `image` crate.
pub fn crop_image(
    path: impl AsRef<Path>,
    params: &Params,
) -> Result<(RgbImage, CropResult), ::image::ImageError> {
    let img = RgbImage::load(path)?;
    let result = find_crop(&img, params);
    let out = match &result.rect {
        Some(rect) => img.crop(rect),
        None => img,
    };
    Ok((out, result))
}
