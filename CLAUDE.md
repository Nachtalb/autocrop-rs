# Working on autocrop

Screenshot content-rectangle detector. Rust library plus two binaries at the
root, Python bindings in `python/` as a second workspace member. The README
describes the algorithm; this file is about the code.

## Layout

| path | role |
|---|---|
| `src/params.rs` | every threshold with a doc comment, `Params::default()` |
| `src/image.rs` | `RgbImage`, decode/encode, area-averaging downscale, colour distance |
| `src/profiles.rs` | background estimation, row/column profiles, integral images |
| `src/letterbox.rs` | step 3, flat bar trimming |
| `src/chrome.rs` | step 4, candidate lines and the scored rectangle search |
| `src/detector.rs` | `analyze`, `find_crop`, `crop_image` |
| `src/bin/autocrop.rs` | CLI |
| `src/bin/autocrop-eval.rs` | evaluation against `eval/ground_truth.json`, `--explain` diagnostics |
| `tests/synthetic.rs` | end-to-end tests on generated layouts |
| `examples/decode_bench.rs` | `image` crate vs libjpeg-turbo decode benchmark (optional `turbojpeg` feature) |
| `python/` | PyO3 bindings, module `autocrop_rs`, own `pyproject.toml` and tests |
| `eval/ground_truth.json` | manual-crop boxes for the sample set, recovered by template matching |
| `docs/` | showcase images used by the README |

The workspace has `default-members = ["."]`, so plain `cargo build` and
`cargo test` skip the bindings crate. Use `--workspace` to include it.

## Quality gate

```
cargo fmt --all --check
cargo clippy --workspace --all-targets      # pedantic, must be warning free
cargo test
cd python && uv run ruff check . && uv run ruff format --check . && uv run pytest
```

CI runs exactly this on Linux, macOS and Windows with `RUSTFLAGS=-D warnings`
on stable Rust. Local nightly clippy is more lenient than stable in places
(float comparisons, for one), so a green local run is not proof.

## Python bindings

- `cd python && uv sync` compiles the extension into the venv through maturin.
- uv caches the build: after changing Rust code run
  `uv sync --reinstall-package autocrop-rs`, otherwise tests run the old
  extension and fail with `AttributeError`.
- abi3 for CPython 3.11+. The floor is 3.11 because PyO3 only exposes the
  buffer protocol under the limited API from 3.11 on.
- Everything runs under `py.detach` (GIL released). Keep it that way for any
  new function.

## Evaluation

`autocrop-eval` needs the labelled sample set at `../Samples` relative to
the repository (folders `screenshots`, `screenshots cropped approximated`,
`not screenshots`); it is private and not committed. Ground truth lives in
the repository. `--explain "screenshot 3.jpg"` prints every measurement for
one sample including the ground-truth rectangle's scores, which is the
fastest way to see which gate rejects a correct box.

Acceptance bar: 0 false crops on the negatives, at least 18/20 positives at
IoU >= 0.85. Current: 20/20, mean IoU 0.979.

## Decode benchmark

```
cargo run --release --features turbojpeg --example decode_bench -- FILE...
```

Builds libjpeg-turbo from source (cmake, nasm, C compiler). Findings so far:
`zune-jpeg` (the `image` crate) is faster than libjpeg-turbo at full size on
the test machine; libjpeg-turbo's DCT-scaled decode only pays for baseline
JPEGs. The default build stays pure Rust.

## Conventions

- Rust 2024 edition, `unsafe` forbidden, public items documented.
- No `target-cpu=native`: prebuilt binaries must be portable, and it was
  measured to change nothing.
- Commit messages: plain, no generated attribution trailers.
- Releases are tags; see `RELEASING.md`. Versions must match in
  `Cargo.toml`, `python/Cargo.toml` and `python/pyproject.toml`.
