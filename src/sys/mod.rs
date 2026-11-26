#[cfg(target_os = "macos")]
mod annotation;
#[cfg(target_os = "macos")]
mod compose;
#[cfg(target_os = "macos")]
mod edit;
#[cfg(target_os = "macos")]
mod flatten;
#[cfg(target_os = "macos")]
mod forms;
#[cfg(target_os = "macos")]
mod image_counts;
#[cfg(target_os = "macos")]
mod objects;
#[cfg(target_os = "macos")]
mod ocr;
#[cfg(target_os = "macos")]
mod password;
#[cfg(target_os = "macos")]
mod pdf;
#[cfg(target_os = "macos")]
mod render;
#[cfg(all(target_os = "macos", feature = "samples"))]
mod samples;
#[cfg(target_os = "macos")]
mod stamp;
#[cfg(target_os = "macos")]
mod write;

#[cfg(target_os = "macos")]
pub(crate) use annotation::annotations;
#[cfg(target_os = "macos")]
pub(crate) use compose::images;
#[cfg(target_os = "macos")]
pub(crate) use edit::{apply, info, outline};
#[cfg(target_os = "macos")]
pub(crate) use forms::fields;
#[cfg(target_os = "macos")]
pub(crate) use ocr::warm_up;
#[cfg(target_os = "macos")]
pub(crate) use pdf::Pdf;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub(crate) use unsupported::{Pdf, annotations, apply, fields, images, info, outline, warm_up};

#[cfg(feature = "samples")]
pub(crate) use samples::generate;

#[cfg(all(not(target_os = "macos"), feature = "samples"))]
mod samples {
    pub(crate) fn generate(_: crate::samples::Sample) -> Result<Vec<u8>, crate::Error> {
        Err(crate::Error::Unsupported)
    }
}
