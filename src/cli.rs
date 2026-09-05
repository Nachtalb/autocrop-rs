//! Command line front end, shared by the `autocrop` binary and the Python
//! bindings (`uvx autocrop-rs`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::{Params, RgbImage, find_crop};

const IMAGE_SUFFIXES: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];

const USAGE: &str = "\
usage: autocrop <files or folders>... [--out DIR] [--all] [--time]
  --out DIR   folder for cropped images (default out/crops)
  --all       also write images that were not cropped
  --time      print decode and detect time per image";

struct Args {
    inputs: Vec<PathBuf>,
    out: PathBuf,
    all: bool,
    time: bool,
}

enum Parsed {
    Run(Args),
    Help,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Parsed, lexopt::Error> {
    use lexopt::prelude::*;
    let mut inputs = Vec::new();
    let mut out = PathBuf::from("out/crops");
    let mut all = false;
    let mut time = false;
    let mut parser = lexopt::Parser::from_args(args);
    while let Some(arg) = parser.next()? {
        match arg {
            Short('o') | Long("out") => out = PathBuf::from(parser.value()?),
            Long("all") => all = true,
            Long("time") => time = true,
            Short('h') | Long("help") => return Ok(Parsed::Help),
            Value(v) => inputs.push(PathBuf::from(v)),
            _ => return Err(arg.unexpected()),
        }
    }
    if inputs.is_empty() {
        return Err("at least one input file or folder is required".into());
    }
    Ok(Parsed::Run(Args {
        inputs,
        out,
        all,
        time,
    }))
}

fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_SUFFIXES.contains(&e.to_ascii_lowercase().as_str()))
}

/// Image files from a mix of files and directories (directories are not recursed).
fn iter_images(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for item in inputs {
        if item.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(item)
                .into_iter()
                .flatten()
                .flatten()
                .map(|e| e.path())
                .filter(|p| is_image(p))
                .collect();
            entries.sort();
            files.extend(entries);
        } else if is_image(item) {
            files.push(item.clone());
        }
    }
    files
}

fn run_args(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&args.out)?;
    let params = Params::default();
    let (mut count, mut cropped) = (0, 0);
    for path in iter_images(&args.inputs) {
        let t0 = Instant::now();
        let img = RgbImage::load(&path)?;
        let t1 = Instant::now();
        let result = find_crop(&img, &params);
        let t2 = Instant::now();
        count += 1;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let boxed = result.rect.map(|r| r.as_tuple());
        println!(
            "{name}: {} score={:.2} box={boxed:?}",
            result.reason, result.score
        );
        if args.time {
            println!(
                "  decode {:.1} ms, detect {:.1} ms ({}x{})",
                (t1 - t0).as_secs_f64() * 1000.0,
                (t2 - t1).as_secs_f64() * 1000.0,
                img.width,
                img.height
            );
        }
        if let Some(rect) = &result.rect {
            cropped += 1;
            img.crop(rect).save(args.out.join(&name))?;
        } else if args.all {
            img.save(args.out.join(&name))?;
        }
    }
    eprintln!("{cropped}/{count} images cropped -> {}", args.out.display());
    Ok(())
}

/// Run the command line with the given arguments (without the program name).
///
/// Prints results to stdout and errors to stderr; returns the process exit
/// code: 0 on success, 1 on a runtime error, 2 on a usage error.
#[must_use]
pub fn run(args: impl IntoIterator<Item = String>) -> i32 {
    let parsed = match parse_args(args) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("error: {e}\n{USAGE}");
            return 2;
        }
    };
    let args = match parsed {
        Parsed::Help => {
            println!("{USAGE}");
            return 0;
        }
        Parsed::Run(a) => a,
    };
    match run_args(&args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("error: {e}");
            1
        }
    }
}
