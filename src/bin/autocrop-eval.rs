//! `autocrop-eval`: run the detector over the labelled sample set and report metrics.
//!
//! Ground-truth boxes (`eval/ground_truth.json`) were recovered by template
//! matching the manual crops into the originals.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use autocrop::chrome::{Scores, explain_rect, line_candidates};
use autocrop::{Params, Rect, RgbImage, analyze, iou};
use serde::Deserialize;

const POSITIVE_DIR: &str = "screenshots";
const NEGATIVE_DIR: &str = "not screenshots";
const IOU_GOOD: f64 = 0.85;

#[derive(Deserialize)]
struct GroundTruth {
    #[serde(rename = "box")]
    rect: (usize, usize, usize, usize),
}

struct Args {
    samples: PathBuf,
    ground_truth: PathBuf,
    explain: Vec<String>,
}

fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut samples = here.join("..").join("Samples");
    let mut ground_truth = here.join("eval").join("ground_truth.json");
    let mut explain = Vec::new();
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Long("samples") => samples = PathBuf::from(parser.value()?),
            Long("ground-truth") => ground_truth = PathBuf::from(parser.value()?),
            Long("explain") => explain.push(parser.value()?.to_string_lossy().into_owned()),
            Short('h') | Long("help") => {
                println!(
                    "usage: autocrop-eval [--samples DIR] [--ground-truth FILE] [--explain NAME]..."
                );
                std::process::exit(0);
            }
            _ => return Err(arg.unexpected()),
        }
    }
    Ok(Args {
        samples,
        ground_truth,
        explain,
    })
}

fn sorted_images(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case("jpg") || e.eq_ignore_ascii_case("png"))
        })
        .collect();
    files.sort();
    files
}

struct Sample {
    name: String,
    positive: bool,
    predicted: Option<Rect>,
    truth: Option<Rect>,
    iou: f64,
    reason: String,
    millis: f64,
}

fn evaluate(args: &Args, truths: &BTreeMap<String, GroundTruth>, params: &Params) -> Vec<Sample> {
    let mut jobs: Vec<(PathBuf, bool)> = sorted_images(&args.samples.join(POSITIVE_DIR))
        .into_iter()
        .map(|p| (p, true))
        .collect();
    jobs.extend(
        sorted_images(&args.samples.join(NEGATIVE_DIR))
            .into_iter()
            .map(|p| (p, false)),
    );
    let mut results = Vec::new();
    for (path, positive) in jobs {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let Ok(img) = RgbImage::load(&path) else {
            eprintln!("cannot read {}", path.display());
            continue;
        };
        let start = Instant::now();
        let analysis = analyze(&img, params);
        let millis = start.elapsed().as_secs_f64() * 1000.0;
        let full = Rect::new(0, 0, img.width, img.height);
        let truth = if positive {
            truths
                .get(&name)
                .map(|t| Rect::new(t.rect.0, t.rect.1, t.rect.2, t.rect.3))
        } else {
            None
        };
        let predicted = analysis.result.rect;
        let score = iou(&predicted.unwrap_or(full), &truth.unwrap_or(full));
        results.push(Sample {
            name,
            positive,
            predicted,
            truth,
            iou: score,
            reason: analysis.result.reason,
            millis,
        });
    }
    results
}

fn fmt_rect(r: Option<Rect>) -> String {
    r.map_or_else(|| "None".to_string(), |r| format!("{:?}", r.as_tuple()))
}

fn print_table(results: &[Sample]) {
    println!(
        "{:<22} {:<4} {:>5}  {:<26} {:<26} reason",
        "sample", "set", "iou", "predicted", "truth"
    );
    for r in results {
        let hit = if r.positive {
            r.iou >= IOU_GOOD
        } else {
            r.predicted.is_none()
        };
        let flag = if hit { "" } else { "  <-- MISS" };
        println!(
            "{:<22} {:<4} {:>5.2}  {:<26} {:<26} {}{flag}",
            r.name,
            if r.positive { "pos" } else { "neg" },
            r.iou,
            fmt_rect(r.predicted),
            fmt_rect(r.truth),
            r.reason
        );
    }
}

fn print_summary(results: &[Sample]) {
    let pos: Vec<&Sample> = results.iter().filter(|r| r.positive).collect();
    let neg: Vec<&Sample> = results.iter().filter(|r| !r.positive).collect();
    let hits = pos.iter().filter(|r| r.iou >= IOU_GOOD).count();
    let mean_iou = pos.iter().map(|r| r.iou).sum::<f64>() / pos.len().max(1) as f64;
    let false_crops = neg.iter().filter(|r| r.predicted.is_some()).count();
    let mean_ms = results.iter().map(|r| r.millis).sum::<f64>() / results.len().max(1) as f64;
    println!();
    println!(
        "positives: {hits}/{} with IoU >= {IOU_GOOD} (mean IoU {mean_iou:.3})",
        pos.len()
    );
    println!("negatives: {false_crops}/{} false crops", neg.len());
    println!("mean time: {mean_ms:.1} ms/image (detector only, excluding decode)");
}

fn print_scores(s: &Scores) {
    println!(
        "    area_frac={:.3} support=({:.3},{:.3},{:.3},{:.3}) min_support={:.3} inside_nonflat={:.3} outside_flat={:.3} outside_strict_flat={:.3} inside_flat_lines={:.3} centred={} valid={} score={:.3}",
        s.area_frac,
        s.support_top,
        s.support_bottom,
        s.support_left,
        s.support_right,
        s.min_support,
        s.inside_nonflat,
        s.outside_flat,
        s.outside_strict_flat,
        s.inside_flat_lines,
        s.centred,
        s.valid,
        s.score
    );
}

fn explain(args: &Args, name: &str, truths: &BTreeMap<String, GroundTruth>, params: &Params) {
    let path = [POSITIVE_DIR, NEGATIVE_DIR]
        .iter()
        .map(|d| args.samples.join(d).join(name))
        .find(|p| p.exists());
    let Some(path) = path else {
        println!("sample not found: {name}");
        return;
    };
    let Ok(img) = RgbImage::load(&path) else {
        println!("cannot read {}", path.display());
        return;
    };
    let a = analyze(&img, params);
    println!(
        "== {name}: {}x{} -> {}x{} (scale {:.3})",
        img.width, img.height, a.small.width, a.small.height, a.scale
    );
    println!(
        "background {:?} grayscale={}",
        a.background.color, a.grayscale
    );
    println!(
        "bar rect {:?} reasons {:?}",
        a.bar_rect.as_tuple(),
        a.bar_reasons
    );
    let (ys, xs) = line_candidates(&a.profiles, &a.bar_rect, params);
    println!("line candidates rows={ys:?}");
    println!("line candidates cols={xs:?}");
    match &a.candidate {
        Some(c) => {
            println!(
                "best candidate {:?} score={:.3}",
                c.rect.as_tuple(),
                c.score
            );
            print_scores(&explain_rect(&a.profiles, &a.bar_rect, &c.rect, params));
        }
        None => println!("best candidate: none"),
    }
    if let Some(t) = truths.get(name) {
        let s = a.scale;
        let gt = Rect::new(
            ((t.rect.0 as f64 * s).round() as usize).min(a.small.width),
            ((t.rect.1 as f64 * s).round() as usize).min(a.small.height),
            ((t.rect.2 as f64 * s).round() as usize).min(a.small.width),
            ((t.rect.3 as f64 * s).round() as usize).min(a.small.height),
        );
        println!("ground truth (small) {:?}", gt.as_tuple());
        print_scores(&explain_rect(&a.profiles, &a.bar_rect, &gt, params));
    }
    println!("result {:?}", a.result);
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    if !args.samples.is_dir() {
        eprintln!("sample folder not found: {}", args.samples.display());
        return ExitCode::from(2);
    }
    let truths: BTreeMap<String, GroundTruth> = match std::fs::read_to_string(&args.ground_truth) {
        Ok(text) => match serde_json::from_str(&text) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("cannot parse {}: {e}", args.ground_truth.display());
                return ExitCode::from(2);
            }
        },
        Err(e) => {
            eprintln!("cannot read {}: {e}", args.ground_truth.display());
            return ExitCode::from(2);
        }
    };
    let params = Params::default();
    if !args.explain.is_empty() {
        for name in &args.explain {
            explain(&args, name, &truths, &params);
        }
        return ExitCode::SUCCESS;
    }
    let results = evaluate(&args, &truths, &params);
    print_table(&results);
    print_summary(&results);
    ExitCode::SUCCESS
}
