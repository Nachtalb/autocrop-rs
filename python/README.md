# autocrop-rs

[![PyPI](https://img.shields.io/pypi/v/autocrop-rs.svg)](https://pypi.org/project/autocrop-rs/)
[![crates.io](https://img.shields.io/crates/v/autocrop.svg)](https://crates.io/crates/autocrop)
[![CI](https://github.com/Nachtalb/autocrop-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Nachtalb/autocrop-rs/actions/workflows/ci.yml)

Finds the actual content inside a screenshot and crops away letterbox bars, app chrome and meme text, while leaving photos, art and manga pages untouched. Python bindings for the Rust [`autocrop`](https://github.com/Nachtalb/autocrop-rs) crate: the package installs as `autocrop-rs`, imports as `autocrop_rs` and ships the command line as `uvx autocrop-rs`.

## Example

A phone screenshot of a tweet with an embedded stream clip (1194 x 2560), the
detector's view of it (row and column profiles along the edges, the chosen
box in red), and the crop `autocrop` writes (1194 x 670).

| input | analysis | crop |
|---|---|---|
| <img src="https://raw.githubusercontent.com/Nachtalb/autocrop-rs/main/docs/example.jpg" width="220" alt="input"> | <img src="https://raw.githubusercontent.com/Nachtalb/autocrop-rs/main/docs/example_debug.png" width="220" alt="analysis overlay"> | <img src="https://raw.githubusercontent.com/Nachtalb/autocrop-rs/main/docs/example_crop.jpg" width="220" alt="crop"> |

```
$ uvx autocrop-rs example.jpg --out out --time
example.jpg: chrome score=0.86 box=Some((0, 822, 1194, 1492))
  decode 16.6 ms, detect 11.0 ms (1194x2560)
```


## Build and test

Rust toolchain required (the extension is compiled on install):

```
cd python
uv sync            # builds the extension into the venv via maturin
uv run pytest
uv sync --reinstall-package autocrop-rs   # after changing Rust code (uv caches the build)
```

Released wheels are on PyPI: `pip install autocrop-rs` (or `uv add
autocrop-rs`). To build one locally: `uv run maturin build --release`. The
wheel is abi3 for CPython 3.11 and newer.

## Usage

```python
import autocrop_rs

result = autocrop_rs.detect_file("screenshot.jpg")
if result:  # truthy when a crop was found
    x0, y0, x1, y1 = result.box  # original pixel coordinates
print(result.reason, result.score)

# Encoded bytes (e.g. straight from an HTTP response or a database blob)
result = autocrop_rs.detect_bytes(data)

# An already decoded HxWx3 uint8 RGB array (numpy, or anything with the buffer protocol)
import numpy as np
from PIL import Image

result = autocrop_rs.detect_array(np.asarray(Image.open("screenshot.jpg").convert("RGB")))

# Detect and write the crop in one call (format by extension); writes nothing when box is None
autocrop_rs.crop_file("screenshot.jpg", "out/screenshot.png")

# Bytes in, bytes out: no decoding or re-encoding on the Python side
result, png = autocrop_rs.crop_bytes(data)  # PNG by default
result, jpg = autocrop_rs.crop_bytes(data, format="jpeg", quality=85)
result = autocrop_rs.crop_bytes_to_file(data, "out/screenshot.webp")  # format by extension

# Thresholds
params = autocrop_rs.Params(accept_score=0.5)
result = autocrop_rs.detect_file("screenshot.jpg", params)
params.to_dict()  # every field and its value
```

All four functions release the GIL while decoding and detecting, so they can
run in a thread pool.
