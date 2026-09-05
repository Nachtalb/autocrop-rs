"""Type stubs for the native ``autocrop_rs`` module."""

from collections.abc import Buffer
from os import PathLike
from typing import Any, Literal

__version__: str

Box = tuple[int, int, int, int]
"""``(x0, y0, x1, y1)`` in original pixels."""

class Params:
    """Detector thresholds. ``Params()`` is the default; any field can be overridden by keyword."""

    def __init__(self, **kwargs: Any) -> None: ...
    def to_dict(self) -> dict[str, float | int]: ...
    def replace(self, **kwargs: Any) -> Params: ...

class CropResult:
    """Result of a detection. Truthy when a crop was found."""

    @property
    def box(self) -> Box | None: ...
    @property
    def score(self) -> float: ...
    @property
    def reason(self) -> str: ...
    def __bool__(self) -> bool: ...

def detect_file(path: str | PathLike[str], params: Params | None = None) -> CropResult:
    """Detect the content rectangle of an image file."""

def detect_bytes(data: bytes, params: Params | None = None) -> CropResult:
    """Detect the content rectangle of an encoded image (JPEG, PNG, WebP, GIF, BMP)."""

def detect_array(array: Buffer, params: Params | None = None) -> CropResult:
    """Detect the content rectangle of a decoded ``HxWx3`` uint8 RGB array (numpy or any buffer)."""

def crop_file(
    path: str | PathLike[str], out_path: str | PathLike[str], params: Params | None = None
) -> CropResult:
    """Detect and, when a crop is found, write the cropped image to ``out_path``."""

def crop_bytes(
    data: bytes,
    format: Literal["png", "jpeg", "jpg", "webp"] = "png",
    quality: int = 90,
    params: Params | None = None,
) -> tuple[CropResult, bytes | None]:
    """Detect on encoded image bytes and return ``(result, encoded crop or None)``.

    ``"jpeg"`` honours ``quality``; ``"webp"`` is lossless.
    """

def crop_bytes_to_file(
    data: bytes, out_path: str | PathLike[str], params: Params | None = None
) -> CropResult:
    """Detect on encoded image bytes and write the crop to ``out_path`` (format by extension)."""

def main(argv: list[str] | None = None) -> int:
    """Run the ``autocrop`` command line on ``argv`` (default ``sys.argv[1:]``).

    Returns the exit code.
    """
