//! Python bindings for the `autocrop` crate, built with `PyO3` and maturin.
//!
//! The module exposes the detector on three kinds of input (a file path,
//! encoded image bytes, or an `HxWx3` uint8 array through the buffer
//! protocol) plus a `Params` class mirroring `autocrop::Params`. Detection
//! runs with the GIL released.

use std::path::PathBuf;

use autocrop::{Params, RgbImage, find_crop};
use pyo3::buffer::PyBuffer;
use pyo3::exceptions::{PyIOError, PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

/// Generates the field-by-name setter and the dict export for `Params`.
macro_rules! params_fields {
    ($($name:ident : $ty:ty),* $(,)?) => {
        fn set_field(p: &mut Params, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
            match name {
                $(stringify!($name) => {
                    p.$name = value.extract::<$ty>()?;
                    Ok(())
                })*
                _ => Err(PyValueError::new_err(format!("unknown parameter: {name}"))),
            }
        }

        fn fields_dict<'py>(p: &Params, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
            let d = PyDict::new(py);
            $(d.set_item(stringify!($name), p.$name)?;)*
            Ok(d)
        }
    };
}

params_fields!(
    max_side: usize,
    bg_band_frac: f64,
    bg_candidates: usize,
    bg_line_frac: f64,
    bg_tol: u8,
    edge_tol: u8,
    dist_cap: u8,
    strict_tol: u8,
    gray_chroma_tol: u8,
    gray_max_color_frac: f64,
    light_luma: f64,
    side_flat_min: f64,
    bar_nonbg_max: f32,
    bar_stop_nonbg_min: f32,
    bar_stop_edge_run_min: f32,
    min_bar_px: usize,
    max_bar_frac: f64,
    line_edge_min: f32,
    line_run_min: f32,
    line_nms_px: usize,
    max_lines: usize,
    min_rect_side_frac: f64,
    min_rect_area_frac: f64,
    strip_px: usize,
    contrast_scale: f64,
    step_min: f64,
    side_support_min: f64,
    strong_support: f64,
    inside_nonflat_min: f64,
    outside_flat_min: f64,
    outside_nonflat_ratio_max: f64,
    outside_strict_flat_min: f64,
    flat_line_start: f64,
    inside_flat_lines_max: f64,
    center_tol_frac: f64,
    w_support: f64,
    w_flat: f64,
    w_nonflat: f64,
    w_area: f64,
    w_flat_lines: f64,
    accept_score: f64,
    min_removed_frac: f64,
);

/// Detector thresholds. `Params()` is the default; pass any field as a keyword to override it.
#[pyclass(name = "Params", module = "autocrop_rs", skip_from_py_object)]
#[derive(Clone)]
struct PyParams {
    inner: Params,
}

#[pymethods]
impl PyParams {
    #[new]
    #[pyo3(signature = (**kwargs))]
    fn new(kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut inner = Params::default();
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                let name: String = key.extract()?;
                set_field(&mut inner, &name, &value)?;
            }
        }
        Ok(Self { inner })
    }

    /// All fields as a dict.
    fn to_dict<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        fields_dict(&self.inner, py)
    }

    /// A copy with the given fields changed.
    #[pyo3(signature = (**kwargs))]
    fn replace(&self, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Self> {
        let mut inner = self.inner.clone();
        if let Some(kwargs) = kwargs {
            for (key, value) in kwargs.iter() {
                let name: String = key.extract()?;
                set_field(&mut inner, &name, &value)?;
            }
        }
        Ok(Self { inner })
    }

    fn __repr__(&self) -> String {
        format!("{:?}", self.inner)
    }
}

/// Result of a detection.
#[pyclass(name = "CropResult", module = "autocrop_rs", frozen)]
struct PyCropResult {
    /// `(x0, y0, x1, y1)` in original pixels, or `None` when nothing should be cropped.
    #[pyo3(get, name = "box")]
    rect: Option<(usize, usize, usize, usize)>,
    /// Score of the decision.
    #[pyo3(get)]
    score: f64,
    /// Which stage produced the decision.
    #[pyo3(get)]
    reason: String,
}

#[pymethods]
impl PyCropResult {
    fn __repr__(&self) -> String {
        format!(
            "CropResult(box={:?}, score={:.3}, reason={:?})",
            self.rect, self.score, self.reason
        )
    }

    fn __bool__(&self) -> bool {
        self.rect.is_some()
    }
}

fn to_result(result: autocrop::CropResult) -> PyCropResult {
    PyCropResult {
        rect: result.rect.map(|r| r.as_tuple()),
        score: result.score,
        reason: result.reason,
    }
}

fn params_of(params: Option<&PyParams>) -> Params {
    params.map_or_else(Params::default, |p| p.inner.clone())
}

fn image_error(e: ::image::ImageError) -> PyErr {
    PyIOError::new_err(e.to_string())
}

fn decode_bytes(data: &[u8]) -> Result<RgbImage, ::image::ImageError> {
    let decoded = ::image::load_from_memory(data)?.to_rgb8();
    let (w, h) = (decoded.width() as usize, decoded.height() as usize);
    let pixels = decoded
        .as_raw()
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();
    Ok(RgbImage::new(w, h, pixels))
}

/// Detect the content rectangle of an image file.
#[pyfunction]
#[pyo3(signature = (path, params=None))]
fn detect_file(py: Python<'_>, path: PathBuf, params: Option<&PyParams>) -> PyResult<PyCropResult> {
    let p = params_of(params);
    let result = py
        .detach(|| RgbImage::load(&path).map(|img| find_crop(&img, &p)))
        .map_err(image_error)?;
    Ok(to_result(result))
}

/// Detect the content rectangle of an encoded image (JPEG, PNG, WebP, GIF, BMP) held in `bytes`.
#[pyfunction]
#[pyo3(signature = (data, params=None))]
fn detect_bytes(py: Python<'_>, data: &[u8], params: Option<&PyParams>) -> PyResult<PyCropResult> {
    let p = params_of(params);
    let result = py
        .detach(|| decode_bytes(data).map(|img| find_crop(&img, &p)))
        .map_err(image_error)?;
    Ok(to_result(result))
}

/// Detect the content rectangle of a decoded `HxWx3` uint8 RGB array (numpy or any buffer).
#[pyfunction]
#[pyo3(signature = (array, params=None))]
fn detect_array(
    py: Python<'_>,
    array: &Bound<'_, PyAny>,
    params: Option<&PyParams>,
) -> PyResult<PyCropResult> {
    let buffer = PyBuffer::<u8>::get(array)?;
    let shape = buffer.shape();
    if buffer.dimensions() != 3 || shape[2] != 3 {
        return Err(PyTypeError::new_err(format!(
            "expected an HxWx3 uint8 array, got shape {shape:?}"
        )));
    }
    let (h, w) = (shape[0], shape[1]);
    let raw = buffer.to_vec(py)?;
    let p = params_of(params);
    let result = py.detach(move || {
        let pixels = raw.chunks_exact(3).map(|c| [c[0], c[1], c[2]]).collect();
        find_crop(&RgbImage::new(w, h, pixels), &p)
    });
    Ok(to_result(result))
}

/// Detect and, when a crop is found, write the cropped image to `out_path` (format by extension).
///
/// Returns the detection result; nothing is written when `box` is `None`.
#[pyfunction]
#[pyo3(signature = (path, out_path, params=None))]
fn crop_file(
    py: Python<'_>,
    path: PathBuf,
    out_path: PathBuf,
    params: Option<&PyParams>,
) -> PyResult<PyCropResult> {
    let p = params_of(params);
    let result = py
        .detach(|| {
            let img = RgbImage::load(&path)?;
            let result = find_crop(&img, &p);
            if let Some(rect) = &result.rect {
                img.crop(rect).save(&out_path)?;
            }
            Ok::<_, ::image::ImageError>(result)
        })
        .map_err(image_error)?;
    Ok(to_result(result))
}

/// Native module entry point.
#[pymodule]
fn autocrop_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyParams>()?;
    m.add_class::<PyCropResult>()?;
    m.add_function(wrap_pyfunction!(detect_file, m)?)?;
    m.add_function(wrap_pyfunction!(detect_bytes, m)?)?;
    m.add_function(wrap_pyfunction!(detect_array, m)?)?;
    m.add_function(wrap_pyfunction!(crop_file, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
