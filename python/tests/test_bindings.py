"""Tests for the autocrop_rs extension module."""

from __future__ import annotations

import io
from pathlib import Path

import numpy as np
import pytest
from PIL import Image

import autocrop_rs

EXAMPLE = Path(__file__).resolve().parents[3] / "autocrop" / "docs" / "example.jpg"
EXAMPLE_BOX = (0, 822, 1194, 1492)


def synthetic_meme() -> tuple[np.ndarray, tuple[int, int, int, int]]:
    """Text on black above a photo touching the left, right and bottom edges."""
    rng = np.random.default_rng(3)
    canvas = np.zeros((640, 400, 3), dtype=np.uint8)
    for y in range(4, 200, 14):
        x = 8
        while x + 12 < 392:
            length = int(rng.integers(4, 12))
            canvas[y : y + 6, x : x + length] = 255
            x += length + int(rng.integers(3, 8))
    canvas[220:, :] = rng.integers(40, 220, size=(420, 400, 3), dtype=np.uint8)
    return canvas, (0, 220, 400, 640)


def iou(a: tuple[int, int, int, int], b: tuple[int, int, int, int]) -> float:
    ix0, iy0, ix1, iy1 = max(a[0], b[0]), max(a[1], b[1]), min(a[2], b[2]), min(a[3], b[3])
    inter = max(0, ix1 - ix0) * max(0, iy1 - iy0)
    area = lambda r: (r[2] - r[0]) * (r[3] - r[1])  # noqa: E731
    return inter / (area(a) + area(b) - inter)


def test_detect_array_on_synthetic_meme() -> None:
    img, truth = synthetic_meme()
    result = autocrop_rs.detect_array(img)
    assert result
    assert result.reason == "chrome"
    assert result.box is not None
    assert iou(result.box, truth) > 0.95


def test_detect_array_accepts_non_contiguous_views() -> None:
    img, truth = synthetic_meme()
    padded = np.zeros((640, 410, 3), dtype=np.uint8)
    padded[:, :400] = img
    result = autocrop_rs.detect_array(padded[:, :400])  # a strided view
    assert result.box is not None
    assert iou(result.box, truth) > 0.95


def test_detect_array_rejects_wrong_shape() -> None:
    with pytest.raises(TypeError):
        autocrop_rs.detect_array(np.zeros((10, 10), dtype=np.uint8))


def test_plain_noise_is_not_cropped() -> None:
    rng = np.random.default_rng(11)
    noise = rng.integers(40, 220, size=(480, 640, 3), dtype=np.uint8)
    result = autocrop_rs.detect_array(noise)
    assert not result
    assert result.box is None


def test_params_kwargs_and_replace() -> None:
    default = autocrop_rs.Params()
    assert default.to_dict()["max_side"] == 800
    custom = autocrop_rs.Params(max_side=400, accept_score=0.5)
    assert custom.to_dict()["max_side"] == 400
    assert custom.replace(accept_score=0.7).to_dict()["accept_score"] == 0.7
    with pytest.raises(ValueError, match="unknown parameter"):
        autocrop_rs.Params(nonsense=1)


def test_params_change_the_result() -> None:
    img, _ = synthetic_meme()
    strict = autocrop_rs.Params(accept_score=1.5, min_removed_frac=0.99)
    assert autocrop_rs.detect_array(img, strict).box is None


@pytest.mark.skipif(not EXAMPLE.exists(), reason="showcase image not available")
def test_file_bytes_and_crop_agree(tmp_path: Path) -> None:
    from_file = autocrop_rs.detect_file(EXAMPLE)
    from_bytes = autocrop_rs.detect_bytes(EXAMPLE.read_bytes())
    from_array = autocrop_rs.detect_array(np.asarray(Image.open(EXAMPLE).convert("RGB")))
    assert from_file.box == EXAMPLE_BOX
    assert from_bytes.box == EXAMPLE_BOX
    assert from_array.box is not None
    assert iou(from_array.box, EXAMPLE_BOX) > 0.98

    out = tmp_path / "crop.png"
    result = autocrop_rs.crop_file(EXAMPLE, out)
    assert result.box == EXAMPLE_BOX
    with Image.open(out) as cropped:
        assert cropped.size == (1194, 670)


def test_missing_file_raises_oserror(tmp_path: Path) -> None:
    with pytest.raises(OSError):
        autocrop_rs.detect_file(tmp_path / "nope.jpg")


@pytest.mark.skipif(not EXAMPLE.exists(), reason="showcase image not available")
@pytest.mark.parametrize(
    ("fmt", "magic"),
    [("png", b"\x89PNG"), ("jpeg", b"\xff\xd8"), ("webp", b"RIFF")],
)
def test_crop_bytes_returns_encoded_crop(fmt: str, magic: bytes) -> None:
    result, data = autocrop_rs.crop_bytes(EXAMPLE.read_bytes(), format=fmt, quality=85)
    assert result.box == EXAMPLE_BOX
    assert data is not None
    assert data.startswith(magic)
    with Image.open(io.BytesIO(data)) as cropped:
        assert cropped.size == (1194, 670)


def test_crop_bytes_returns_none_without_crop() -> None:
    rng = np.random.default_rng(11)
    noise = Image.fromarray(rng.integers(40, 220, size=(120, 160, 3), dtype=np.uint8))
    buf = io.BytesIO()
    noise.save(buf, format="PNG")
    result, data = autocrop_rs.crop_bytes(buf.getvalue())
    assert not result
    assert data is None


def test_crop_bytes_rejects_bad_format() -> None:
    with pytest.raises(ValueError, match="unsupported format"):
        autocrop_rs.crop_bytes(b"\x89PNG", format="tiff")


@pytest.mark.skipif(not EXAMPLE.exists(), reason="showcase image not available")
def test_crop_bytes_to_file(tmp_path: Path) -> None:
    out = tmp_path / "crop.webp"
    result = autocrop_rs.crop_bytes_to_file(EXAMPLE.read_bytes(), out)
    assert result.box == EXAMPLE_BOX
    with Image.open(out) as cropped:
        assert cropped.format == "WEBP"
        assert cropped.size == (1194, 670)
