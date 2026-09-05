# autocrop (Rust)

Rust port of [autocrop](https://github.com/Nachtalb/autocrop), the screenshot
content-rectangle detector: it finds the photo, video frame or game viewport
inside a screenshot and crops away letterbox bars, phone and desktop chrome
and meme text. Images that are not screenshots (photos, art, manga pages) are
left untouched.

The algorithm is identical to the Python reference implementation, module for
module, and is measured against the same ground-truth boxes. See the Python
README for the full description of the pipeline and its thresholds.

## Build

```
cargo build --release
```

The release profile uses fat LTO, a single codegen unit, `panic = "abort"`
and symbol stripping. `.cargo/config.toml` adds `-C target-cpu=native`, so a
binary built here only runs on CPUs at least as new as the build machine.
Remove that file for a portable build. The stripped `autocrop` binary is
about 1.2 MB and links only `image` (JPEG, PNG, WebP, GIF, BMP decoders),
`lexopt` and `serde_json`.

## Usage

```
autocrop <files or folders>... [--out DIR] [--all]
autocrop-eval [--samples DIR] [--ground-truth FILE] [--explain NAME]...
```

`autocrop` writes each cropped image under `--out` (default `out/crops`) with
its original name and prints one line per file:

```
$ autocrop docs/example.jpg --out out
example.jpg: chrome score=0.86 box=Some((0, 822, 1194, 1492))
```

`autocrop-eval` needs the labelled sample set and the `ground_truth.json`
written by the Python harness (defaults: `../Samples` and
`../autocrop/out/ground_truth.json`). `--explain NAME` prints the background
estimate, the bar-trim result, the candidate lines and the full score
breakdown of the best rectangle and of the ground-truth rectangle.

From Rust:

```rust
use autocrop::{Params, crop_image, find_crop};

let (image, result) = crop_image("screenshot.jpg", &Params::default())?;
let result = find_crop(&rgb_image, &Params::default()); // CropResult { rect, score, reason }
```

## Results

Same sample set as the Python project (20 screenshots with manual crops,
12 non-screenshots), release build:

| metric | Rust | Python |
|---|---|---|
| positives with IoU >= 0.85 | 20 / 20 | 20 / 20 |
| mean IoU on positives | 0.979 | 0.980 |
| false crops on negatives | 0 / 12 | 0 / 12 |
| detector time per image (800 px working copy) | ~9.5 ms | ~55 ms |

Boxes differ from the Python output by at most a few pixels, which comes from
the area-averaging downscale being implemented independently.

## Layout

| file | role |
|---|---|
| `src/params.rs` | every threshold, with docs, `Params::default()` |
| `src/image.rs` | RGB container, decode/encode, box downscale, colour distance |
| `src/profiles.rs` | background estimation, row/column profiles, integral images |
| `src/letterbox.rs` | flat bar trimming |
| `src/chrome.rs` | candidate lines and the scored rectangle search |
| `src/detector.rs` | `analyze`, `find_crop`, `crop_image` |
| `src/bin/autocrop.rs` | CLI |
| `src/bin/autocrop-eval.rs` | evaluation and `--explain` diagnostics |
| `tests/synthetic.rs` | end-to-end tests on generated layouts |

Quality gate: `cargo fmt --check`, `cargo clippy --all-targets` (pedantic,
warning free), `cargo test`.
