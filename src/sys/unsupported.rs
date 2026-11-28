use std::{convert::Infallible, marker::PhantomData, rc::Rc};

use zeroize::Zeroizing;

use crate::edit::{Annotation, Field, Info, OutlineItem, Plan, Save};
use crate::ocr::Observation;
use crate::{Cancel, Error, Image, ImageFormat, OcrOptions};

pub(crate) fn warm_up(_: &OcrOptions) -> Result<(), Error> {
    Err(Error::Unsupported)
}

pub(crate) fn searchable(
    _: &crate::Document,
    _: &[u32],
    _: &OcrOptions,
    _: &Cancel,
    _: &dyn Fn(u32),
) -> Result<Vec<u8>, Error> {
    Err(Error::Unsupported)
}

pub(crate) struct Pdf(Infallible, PhantomData<Rc<()>>);

pub(crate) fn apply(_: &Pdf, _: &[&Pdf], _: &Plan, _: &Save, _: &Cancel) -> Result<Vec<u8>, Error> {
    Err(Error::Unsupported)
}

pub(crate) fn info(doc: &Pdf) -> Info {
    match doc.0 {}
}

pub(crate) fn outline(doc: &Pdf) -> Vec<OutlineItem> {
    match doc.0 {}
}

pub(crate) fn annotations(_: &Pdf, _: u32) -> Result<Vec<Annotation>, Error> {
    Err(Error::Unsupported)
}

#[cfg(feature = "pdfium")]
pub(crate) fn page_objects(_: &Pdf, _: u32) -> Result<Vec<crate::edit::PageObject>, Error> {
    Err(Error::Unsupported)
}

pub(crate) fn fields(_: &Pdf, _: &Cancel) -> Result<Vec<Field>, Error> {
    Err(Error::Unsupported)
}

pub(crate) fn images(_: &[crate::compose::ImageSource<'_>], _: &Cancel) -> Result<Vec<u8>, Error> {
    Err(Error::Unsupported)
}

impl Pdf {
    pub(crate) fn from_bytes(_: &[u8]) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    pub(crate) fn from_bytes_with_password(_: &[u8], _: &Zeroizing<String>) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    pub(crate) fn page_count(&self) -> u32 {
        match self.0 {}
    }

    pub(crate) fn original_bytes(&self) -> &[u8] {
        match self.0 {}
    }

    pub(crate) fn label(&self, _: u32) -> Option<String> {
        match self.0 {}
    }

    pub(crate) fn text(&self, _: u32) -> Result<String, Error> {
        match self.0 {}
    }

    pub(crate) fn page(&self, _: u32) -> Result<(Option<String>, String, bool), Error> {
        match self.0 {}
    }

    pub(crate) fn render(&self, _: u32, _: ImageFormat, _: u32) -> Result<Image, Error> {
        match self.0 {}
    }

    pub(crate) fn page_box(&self, _: u32) -> Result<(crate::edit::Rect, i32), Error> {
        match self.0 {}
    }

    pub(crate) fn ocr(&self, _: u32, _: &OcrOptions, _: bool) -> Result<Vec<Observation>, Error> {
        match self.0 {}
    }

    pub(crate) fn image_counts(&self, _: &Cancel) -> Result<Vec<(u32, usize)>, Error> {
        match self.0 {}
    }

    #[cfg(feature = "samples")]
    pub(crate) fn scan(&self, _: u32) -> Result<Vec<u8>, Error> {
        match self.0 {}
    }

    pub(crate) fn split(&self, _: &[u32], _: &Cancel) -> Result<Vec<u8>, Error> {
        match self.0 {}
    }
}

pub(crate) fn scan_find(
    _: &crate::Document,
    _: &[u32],
    _: &crate::find::Options,
    _: &crate::Cancel,
    _: &dyn Fn(usize, u32),
    _: &mut engine_core::find::Finder<crate::find::Location>,
) -> Result<(), crate::Error> {
    Err(crate::Error::Unsupported)
}
