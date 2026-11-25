use std::{convert::Infallible, marker::PhantomData, rc::Rc};

use crate::{Cancel, Error, Image, ImageFormat, OcrBounds, OcrLine, OcrOptions};

pub(crate) fn warm_up(_: &OcrOptions) -> Result<(), Error> {
    Err(Error::Unsupported)
}

pub(crate) struct Pdf(Infallible, PhantomData<Rc<()>>);

impl Pdf {
    pub(crate) fn from_bytes(_: &[u8]) -> Result<Self, Error> {
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

    pub(crate) fn page(&self, _: u32) -> Result<(Option<String>, String), Error> {
        match self.0 {}
    }

    pub(crate) fn render(&self, _: u32, _: ImageFormat, _: u32) -> Result<Image, Error> {
        match self.0 {}
    }

    pub(crate) fn ocr(&self, _: u32, _: &OcrOptions) -> Result<Vec<(OcrLine, OcrBounds)>, Error> {
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
