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
Remove that file for a portable build, or override it for one build with
`cargo build --release --config 'build.rustflags=[]'`; see the timing table
below for why the portable build loses nothing. The stripped `autocrop`
binary is about 1.2 MB and links only `image` (JPEG, PNG, WebP, GIF, BMP
decoders), `lexopt` and `serde_json`.

## Example

The same showcase image as the Python project: a phone screenshot of a tweet
with an embedded stream clip (`docs/example.jpg`, 1194 x 2560). The crop
(`docs/example_crop.jpg`, 1194 x 670) is pixel-identical in position to the
Python result. The overlay on the right comes from the Python tool's
`--debug` mode (the Rust CLI has no overlay renderer).

| input | crop | debug overlay (Python) |
|---|---|---|
| <img src="docs/example.jpg" width="220" alt="input"> | <img src="docs/example_crop.jpg" width="220" alt="crop"> | <img src="docs/example_debug_python.png" width="220" alt="debug overlay"> |

```
$ autocrop docs/example.jpg --out out --time
example.jpg: chrome score=0.86 box=Some((0, 822, 1194, 1492))
  decode 16.6 ms, detect 11.0 ms (1194x2560)
```

Timing on that image, median of three runs on the same machine:

| stage | Python (`uv run autocrop --time`) | Rust, `target-cpu=native` | Rust, generic x86-64 |
|---|---|---|---|
| JPEG decode | 27 ms (Pillow) | 16.6 ms (`image`) | 16.4 ms |
| detector (downscale + analysis) | 50 ms | 11.0 ms | 10.7 ms |
| total | 77 ms | 27.6 ms | 27.1 ms |

On the whole sample set (`autocrop-eval`, detector only) the native build
averages 9.5 ms per image and the generic build 9.7 ms, three runs each. The
`-C target-cpu=native` flag buys nothing here: the hot loops work on `u8` and
`bool` values with data-dependent branches, which LLVM vectorises poorly
with or without AVX2. It stays in `.cargo/config.toml` because it costs
nothing on the build machine, but the portable build is just as fast. At
this point JPEG decoding is the larger cost, not the detector.


## Usage

```
autocrop <files or folders>... [--out DIR] [--all] [--time]
autocrop-eval [--samples DIR] [--ground-truth FILE] [--explain NAME]...
```

`autocrop` writes each cropped image under `--out` (default `out/crops`) with
its original name (so do not point `--out` at the input folder) and prints
one line per file; `--time` adds decode and detect times.

`autocrop-eval` needs the labelled sample set and the `ground_truth.json`
written by the Python harness (defaults: `../Samples` and
`../autocrop/out/ground_truth.json`). `--explain NAME` prints the background
estimate, the bar-trim result, the candidate lines and the full score
breakdown of the best rectangle and of the ground-truth rectangle.

From Python: the `python/` folder holds PyO3 bindings (`import autocrop_rs`)
with `detect_file`, `detect_bytes`, `detect_array` (numpy), `crop_file`,
`crop_bytes` and `crop_bytes_to_file`;
see `python/README.md`.

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

`examples/decode_bench.rs` compares the `image` crate's decoders with
libjpeg-turbo, including its DCT-domain 1/2, 1/4 and 1/8 scaled decode, and
runs the detector on each output. It needs the optional `turbojpeg` feature,
which builds libjpeg-turbo from source (cmake, nasm and a C compiler):

```
cargo run --release --features turbojpeg --example decode_bench -- FILE...
```
