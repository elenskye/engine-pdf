#[cfg(target_os = "macos")]
mod annotation;
#[cfg(target_os = "macos")]
mod cache;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
mod check;
#[cfg(target_os = "macos")]
mod compose;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
mod content;
#[cfg(target_os = "macos")]
mod edit;
#[cfg(target_os = "macos")]
mod find;
#[cfg(target_os = "macos")]
mod flatten;
#[cfg(target_os = "macos")]
mod forms;
#[cfg(target_os = "macos")]
mod image_counts;
#[cfg(target_os = "macos")]
mod layer;
#[cfg(target_os = "macos")]
mod objects;
#[cfg(target_os = "macos")]
mod ocr;
#[cfg(target_os = "macos")]
mod password;
#[cfg(target_os = "macos")]
mod pdf;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
mod pdfium;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
mod redact;
#[cfg(target_os = "macos")]
mod render;
#[cfg(all(target_os = "macos", feature = "samples"))]
mod samples;
#[cfg(target_os = "macos")]
mod searchable;
#[cfg(target_os = "macos")]
mod stamp;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
mod tags;
#[cfg(target_os = "macos")]
mod write;

#[cfg(target_os = "macos")]
pub(crate) use annotation::annotations;
#[cfg(target_os = "macos")]
pub(crate) use compose::images;
#[cfg(target_os = "macos")]
pub(crate) use edit::{apply, info, outline};
#[cfg(target_os = "macos")]
pub(crate) use find::scan_find;
#[cfg(target_os = "macos")]
pub(crate) use forms::fields;
#[cfg(target_os = "macos")]
pub(crate) use ocr::warm_up;
#[cfg(target_os = "macos")]
pub(crate) use pdf::Pdf;
#[cfg(all(target_os = "macos", feature = "pdfium"))]
pub(crate) use pdfium::page_objects;
#[cfg(target_os = "macos")]
pub(crate) use searchable::searchable;
#[cfg(all(not(target_os = "macos"), feature = "pdfium"))]
pub(crate) use unsupported::page_objects;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub(crate) use unsupported::{
    Pdf, annotations, apply, fields, images, info, outline, scan_find, searchable, warm_up,
};

#[cfg(feature = "samples")]
pub(crate) use samples::generate;

#[cfg(all(not(target_os = "macos"), feature = "samples"))]
mod samples {
    pub(crate) fn generate(_: crate::samples::Sample) -> Result<Vec<u8>, crate::Error> {
        Err(crate::Error::Unsupported)
    }
}
