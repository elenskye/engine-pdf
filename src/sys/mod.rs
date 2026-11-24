#[cfg(target_os = "macos")]
mod image_counts;
#[cfg(target_os = "macos")]
mod pdf;
#[cfg(target_os = "macos")]
mod render;
#[cfg(all(target_os = "macos", feature = "samples"))]
mod samples;

#[cfg(target_os = "macos")]
pub(crate) use pdf::Pdf;
#[cfg(not(target_os = "macos"))]
mod unsupported;
#[cfg(not(target_os = "macos"))]
pub(crate) use unsupported::Pdf;

#[cfg(feature = "samples")]
pub(crate) use samples::generate;

#[cfg(all(not(target_os = "macos"), feature = "samples"))]
mod samples {
    pub(crate) fn generate(_: crate::samples::Sample) -> Result<Vec<u8>, crate::Error> {
        Err(crate::Error::Unsupported)
    }
}
