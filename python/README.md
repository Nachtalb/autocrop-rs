# autocrop-rs (Python bindings)

Python bindings for the Rust `autocrop` detector, built with PyO3 and
maturin. The extension module is called `autocrop_rs` so it does not clash
with the pure-Python reference package `autocrop`.

## Build and test

Rust toolchain required (the extension is compiled on install):

```
cd python
uv sync            # builds the extension into the venv via maturin
uv run pytest
uv sync --reinstall-package autocrop-rs   # after changing Rust code (uv caches the build)
```

To build a wheel: `uv run maturin build --release`. The wheel is abi3 for
CPython 3.11 and newer. Note that the workspace's `.cargo/config.toml` adds
`-C target-cpu=native`; delete it or override `build.rustflags` before
building wheels meant for other machines.

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
