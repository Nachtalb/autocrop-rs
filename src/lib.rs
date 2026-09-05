//! Detect and crop the content rectangle out of screenshots.
//!
//! Port of the Python reference implementation (`autocrop`). The pipeline is:
//! downscale, estimate the chrome background colour, compute row/column
//! profiles and integral images, trim flat bars, then search for the best
//! scoring content rectangle inside the remaining chrome.
//!
//! ```no_run
//! use autocrop::{Params, crop_image};
//!
//! let (image, result) = crop_image("screenshot.jpg", &Params::default()).unwrap();
//! if let Some(rect) = result.rect {
//!     println!("crop to {rect:?}");
//! }
//! ```

// The optional dependency is only used by `examples/decode_bench.rs`.
#[cfg(feature = "turbojpeg")]
use turbojpeg as _;

pub mod chrome;
pub mod detector;
pub mod geometry;
pub mod image;
pub mod letterbox;
pub mod params;
pub mod profiles;

pub use detector::{Analysis, CropResult, analyze, crop_image, find_crop};
pub use geometry::{Rect, iou};
pub use image::{Encoding, RgbImage};
pub use params::Params;
