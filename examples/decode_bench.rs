//! Decode benchmark: the `image` crate versus libjpeg-turbo, with and without
//! DCT-domain scaling, followed by the detector on whatever came out.
//!
//! ```
//! cargo run --release --features turbojpeg --example decode_bench -- FILE...
//! ```
//!
//! For every JPEG it prints decode time, detector time and the crop box mapped
//! back to full-resolution coordinates, so the effect of decoding at 1/2, 1/4
//! or 1/8 size on the detection result is visible next to the time saved.
//! Non-JPEG files only get the `image` crate row.

use std::path::PathBuf;
use std::time::Instant;

use autocrop::{Params, Rect, RgbImage, find_crop};
use turbojpeg::{Decompressor, Image, PixelFormat, ScalingFactor};

const RUNS: usize = 7;

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

/// Time `f` `RUNS` times and return the median in milliseconds plus the last result.
fn timed<T>(mut f: impl FnMut() -> T) -> (f64, T) {
    let mut times = Vec::with_capacity(RUNS);
    let mut last = None;
    for _ in 0..RUNS {
        let start = Instant::now();
        last = Some(f());
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    (median(times), last.expect("at least one run"))
}

fn turbo_decode(
    decompressor: &mut Decompressor,
    data: &[u8],
    factor: ScalingFactor,
    fast_upsample: bool,
) -> turbojpeg::Result<RgbImage> {
    let header = decompressor.read_header(data)?;
    decompressor.set_scaling_factor(factor)?;
    decompressor.set_fast_upsample(fast_upsample)?;
    let scaled = header.scaled(factor);
    let (w, h) = (scaled.width, scaled.height);
    let mut raw = vec![0u8; w * h * 3];
    decompressor.decompress(
        data,
        Image {
            pixels: raw.as_mut_slice(),
            width: w,
            pitch: w * 3,
            height: h,
            format: PixelFormat::RGB,
        },
    )?;
    let pixels = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
    Ok(RgbImage::new(w, h, pixels))
}

fn scale_up(rect: Rect, denom: usize, width: usize, height: usize) -> Rect {
    Rect::new(
        rect.x0 * denom,
        rect.y0 * denom,
        (rect.x1 * denom).min(width),
        (rect.y1 * denom).min(height),
    )
}

fn report(label: &str, decode_ms: f64, img: &RgbImage, denom: usize, full_w: usize, full_h: usize) {
    let params = Params::default();
    let (detect_ms, result) = timed(|| find_crop(img, &params));
    let boxed = result
        .rect
        .map(|r| scale_up(r, denom, full_w, full_h).as_tuple());
    println!(
        "  {label:<28} {:>5}x{:<5} decode {decode_ms:6.1} ms  detect {detect_ms:5.1} ms  total {:6.1} ms  box={boxed:?}",
        img.width,
        img.height,
        decode_ms + detect_ms
    );
}

fn main() {
    let files: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if files.is_empty() {
        eprintln!("usage: decode_bench FILE...");
        std::process::exit(2);
    }
    for path in files {
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", path.display());
                continue;
            }
        };
        println!("{} ({} KB)", path.display(), data.len() / 1024);

        let (ms, decoded) = timed(|| ::image::load_from_memory(&data).map(|d| d.to_rgb8()));
        let Ok(rgb) = decoded else {
            println!("  image crate: cannot decode");
            continue;
        };
        let (full_w, full_h) = (rgb.width() as usize, rgb.height() as usize);
        let pixels = rgb
            .as_raw()
            .chunks_exact(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        let img = RgbImage::new(full_w, full_h, pixels);
        report("image crate (full)", ms, &img, 1, full_w, full_h);

        let is_jpeg = data.starts_with(&[0xFF, 0xD8]);
        if !is_jpeg {
            continue;
        }
        let mut decompressor = match Decompressor::new() {
            Ok(d) => d,
            Err(e) => {
                println!("  libjpeg-turbo: {e}");
                continue;
            }
        };
        for (label, factor, denom, fast) in [
            ("libjpeg-turbo (full)", ScalingFactor::ONE, 1, false),
            ("libjpeg-turbo (full, fast up)", ScalingFactor::ONE, 1, true),
            ("libjpeg-turbo 1/2", ScalingFactor::ONE_HALF, 2, false),
            ("libjpeg-turbo 1/4", ScalingFactor::ONE_QUARTER, 4, false),
            (
                "libjpeg-turbo 1/4 (fast up)",
                ScalingFactor::ONE_QUARTER,
                4,
                true,
            ),
            ("libjpeg-turbo 1/8", ScalingFactor::ONE_EIGHTH, 8, false),
        ] {
            let (ms, decoded) = timed(|| turbo_decode(&mut decompressor, &data, factor, fast));
            match decoded {
                Ok(img) => report(label, ms, &img, denom, full_w, full_h),
                Err(e) => println!("  {label}: {e}"),
            }
        }
    }
}
