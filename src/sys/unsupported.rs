use std::{convert::Infallible, marker::PhantomData, rc::Rc};

use crate::{Cancel, Error, Image, ImageFormat};

pub(crate) struct Pdf(Infallible, PhantomData<Rc<()>>);

impl Pdf {
    pub(crate) fn from_bytes(_: &[u8]) -> Result<Self, Error> {
        Err(Error::Unsupported)
    }

    pub(crate) fn page_count(&self) -> u32 {
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

    pub(crate) fn image_counts(&self, _: &Cancel) -> Result<Vec<(u32, usize)>, Error> {
        match self.0 {}
    }

    pub(crate) fn split(&self, _: &[u32], _: &Cancel) -> Result<Vec<u8>, Error> {
        match self.0 {}
    }
}
