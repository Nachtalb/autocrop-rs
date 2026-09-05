//! Axis-aligned rectangles in pixel coordinates.

/// Half-open rectangle `[x0, x1) x [y0, y1)`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Rect {
    /// Left edge (inclusive).
    pub x0: usize,
    /// Top edge (inclusive).
    pub y0: usize,
    /// Right edge (exclusive).
    pub x1: usize,
    /// Bottom edge (exclusive).
    pub y1: usize,
}

impl Rect {
    /// Construct from edges.
    #[must_use]
    pub const fn new(x0: usize, y0: usize, x1: usize, y1: usize) -> Self {
        Self { x0, y0, x1, y1 }
    }

    /// Width in pixels.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.x1 - self.x0
    }

    /// Height in pixels.
    #[must_use]
    pub const fn height(&self) -> usize {
        self.y1 - self.y0
    }

    /// Area in pixels.
    #[must_use]
    pub const fn area(&self) -> usize {
        self.width() * self.height()
    }

    /// `(x0, y0, x1, y1)`.
    #[must_use]
    pub const fn as_tuple(&self) -> (usize, usize, usize, usize) {
        (self.x0, self.y0, self.x1, self.y1)
    }

    /// Scale by `factor` (rounding outward) and clamp to `width` x `height`.
    #[must_use]
    pub fn scaled(&self, factor: f64, width: usize, height: usize) -> Self {
        Self {
            x0: (self.x0 as f64 * factor).floor().max(0.0) as usize,
            y0: (self.y0 as f64 * factor).floor().max(0.0) as usize,
            x1: ((self.x1 as f64 * factor).ceil() as usize).min(width),
            y1: ((self.y1 as f64 * factor).ceil() as usize).min(height),
        }
    }
}

/// Intersection over union of two rectangles.
#[must_use]
pub fn iou(a: &Rect, b: &Rect) -> f64 {
    let ix0 = a.x0.max(b.x0);
    let iy0 = a.y0.max(b.y0);
    let ix1 = a.x1.min(b.x1);
    let iy1 = a.y1.min(b.y1);
    let inter = ix1.saturating_sub(ix0) * iy1.saturating_sub(iy0);
    let union = a.area() + b.area() - inter;
    if union == 0 {
        0.0
    } else {
        inter as f64 / union as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iou_of_identical_and_disjoint() {
        let a = Rect::new(0, 0, 10, 10);
        assert!((iou(&a, &a) - 1.0).abs() < 1e-12);
        assert_eq!(iou(&a, &Rect::new(20, 20, 30, 30)), 0.0);
        let half = Rect::new(0, 0, 10, 5);
        assert!((iou(&a, &half) - 0.5).abs() < 1e-12);
    }

    #[test]
    fn scaled_rounds_outward_and_clamps() {
        let r = Rect::new(1, 1, 5, 5).scaled(1.6, 8, 7);
        assert_eq!(r, Rect::new(1, 1, 8, 7));
    }
}
