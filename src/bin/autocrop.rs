//! `autocrop` command line: crop files or folders.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use autocrop::{Params, RgbImage, find_crop};

const IMAGE_SUFFIXES: &[&str] = &["jpg", "jpeg", "png", "webp", "bmp", "gif"];

struct Args {
    inputs: Vec<PathBuf>,
    out: PathBuf,
    all: bool,
}

fn parse_args() -> Result<Args, lexopt::Error> {
    use lexopt::prelude::*;
    let mut inputs = Vec::new();
    let mut out = PathBuf::from("out/crops");
    let mut all = false;
    let mut parser = lexopt::Parser::from_env();
    while let Some(arg) = parser.next()? {
        match arg {
            Short('o') | Long("out") => out = PathBuf::from(parser.value()?),
            Long("all") => all = true,
            Short('h') | Long("help") => {
                println!("usage: autocrop <files or folders>... [--out DIR] [--all]");
                println!("  --out DIR   folder for cropped images (default out/crops)");
                println!("  --all       also write images that were not cropped");
                std::process::exit(0);
            }
            Value(v) => inputs.push(PathBuf::from(v)),
            _ => return Err(arg.unexpected()),
        }
    }
    if inputs.is_empty() {
        return Err("at least one input file or folder is required".into());
    }
    Ok(Args { inputs, out, all })
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

fn run(args: &Args) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(&args.out)?;
    let params = Params::default();
    let (mut count, mut cropped) = (0, 0);
    for path in iter_images(&args.inputs) {
        let img = RgbImage::load(&path)?;
        let result = find_crop(&img, &params);
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

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
