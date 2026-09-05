# autocrop (Rust)

Screenshot content-rectangle detector: it finds the photo, video frame or
game viewport inside a screenshot and crops away letterbox bars, phone and
desktop chrome and meme text. Images that are not screenshots (photos, art,
manga pages) are left untouched.

Ships as a Rust library, two command line tools and Python bindings
(`pip install autocrop-rs`). The algorithm was developed in the archived
Python prototype [Nachtalb/autocrop](https://github.com/Nachtalb/autocrop)
and this crate reproduces its results on the same ground-truth boxes.

## Build

```
cargo build --release
```

The release profile uses fat LTO, a single codegen unit, `panic = "abort"`
and symbol stripping. The stripped `autocrop` binary is about 1.2 MB and
links only `image` (JPEG, PNG, WebP, GIF, BMP decoders), `lexopt` and
`serde_json`. Prebuilt binaries for Linux, macOS and Windows are attached to
each GitHub release; the Python bindings are on PyPI as `autocrop-rs`.

## Example

A phone screenshot of a tweet
with an embedded stream clip (`docs/example.jpg`, 1194 x 2560). The crop
(`docs/example_crop.jpg`, 1194 x 670) is pixel-identical in position to the
Python result. The overlay on the right was rendered by the Python prototype's
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

| stage | Python prototype | Rust |
|---|---|---|
| JPEG decode | 27 ms (Pillow) | 16.6 ms (`image`) |
| detector (downscale + analysis) | 50 ms | 11.0 ms |
| total | 77 ms | 27.6 ms |

On the whole sample set (`autocrop-eval`, detector only) the release build
averages 9.5 ms per image. Building with `-C target-cpu=native` was measured
and changes nothing: the hot loops work on `u8` and `bool` values with
data-dependent branches, which LLVM vectorises poorly with or without AVX2,
so the portable build is what gets shipped. At this point JPEG decoding is
the larger cost, not the detector.


## Usage

```
autocrop <files or folders>... [--out DIR] [--all] [--time]
autocrop-eval [--samples DIR] [--ground-truth FILE] [--explain NAME]...
```

`autocrop` writes each cropped image under `--out` (default `out/crops`) with
its original name (so do not point `--out` at the input folder) and prints
one line per file; `--time` adds decode and detect times.

`autocrop-eval` needs the labelled sample set (default `../Samples`, not part
of the repository) and the ground-truth boxes in `eval/ground_truth.json`,
which were recovered by template-matching the manual crops into the
originals. `--explain NAME` prints the background
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

## Algorithm

All work happens on a copy downscaled to at most 800 px on the long side;
the box is mapped back to original coordinates at the end.

1. **Background colour** (`profiles::estimate_background`). Dominant colours
   of the outer band are candidates; the one that fills the most complete
   rows and columns wins. This is robust against a one pixel outline or a
   status bar in a different shade, and against large dark photos, whose
   dark tones never fill whole rows.
2. **Profiles** (`profiles::compute_profiles`). Per pixel: distance to the
   background colour (clipped and normalised), "is background" within a
   tolerance, a stricter "is background" with a tight tolerance, and
   adjacent-row / adjacent-column edges. From those: per row and per column
   non-background fractions, longest contiguous edge runs, and 2-D prefix
   sums so any rectangle statistic is O(1).
3. **Flat bar trimming** (`letterbox::trim_flat_bars`). From each image side
   whose outer band is a single colour, walk inward while rows or columns
   are entirely that colour. The trim is kept only when the walk stops at a
   real content edge (a mostly non-background line or a long straight edge),
   so a soft glow fading into black is not trimmed. Grayscale images with
   light margins (manga pages) are never trimmed.
4. **Chrome removal** (`chrome::find_content_rect`) inside the trimmed
   region. Boundary candidates are rows and columns with a long straight
   edge or a jump in mean background distance. Every combination of two
   horizontal and two vertical candidates is scored with:
   - *side support*: for each side, the larger of the edge fraction along
     it and the background-distance contrast between a strip just inside
     and just outside (sides on the region border count as fully supported);
   - *outside flatness*: fraction of pixels outside the rectangle that are
     background, plus a strict-tolerance version that separates real chrome
     from dark painted areas (waived when every side is strongly supported);
   - *inside non-flatness*: fraction of pixels inside that are not
     background, which rejects text-on-a-page interiors;
   - *flat lines inside*: rows and columns inside that are mostly
     background, which penalises rectangles that swallow chrome;
   - horizontal centring (or spanning the full width) as a hard constraint;
   - area, as a small bonus so the outer game viewport beats an inner
     dialog box.
   The best valid rectangle is accepted above a score threshold.
5. **Guards**. A crop must remove at least 1 percent of the area. Grayscale
   images whose outside region is light are never cropped (manga pages).
   A bar-trim result whose interior is mostly background is discarded.

Every threshold lives in `autocrop::Params` with a doc comment.

## Known limitations

- A caption box or text line that touches the content with no background
  gap between them is treated as part of the content.
- Content whose edge colour equals the chrome colour along an entire side
  (a black video bottom on a black background) relies on the other sides
  and on the mean-distance jump; very dark content next to black gives low
  support and can be rejected.
- Grayscale content on a light background is never cropped by design, which
  also skips black-and-white photo memes on white.
- Only one rectangle is returned; collages and stacked frames are left as
  they are.

## Results

Labelled sample set of 20 screenshots with manual crops and 12
non-screenshots, release build, compared with the Python prototype:

| metric | Rust | Python |
|---|---|---|
| positives with IoU >= 0.85 | 20 / 20 | 20 / 20 |
| mean IoU on positives | 0.979 | 0.980 |
| false crops on negatives | 0 / 12 | 0 / 12 |
| detector time per image (800 px working copy) | ~9.5 ms | ~55 ms |

Boxes differ from the Python prototype by at most a few pixels, which comes
from the area-averaging downscale being implemented independently.

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
