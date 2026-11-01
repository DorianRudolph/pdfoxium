// Interface copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend::PdfPage as CorePage;
use crate::prelude::*;
use pdfoxium_core::fpdfsdk::fpdf_transformpage as sdk;
use std::marker::PhantomData;
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageBoundaryBoxType {
    Media,
    Art,
    Bleed,
    Trim,
    Crop,
    Bounding,
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct PdfPageBoundaryBox {
    pub box_type: PdfPageBoundaryBoxType,
    pub bounds: PdfRect,
}
pub struct PdfPageBoundaries<'a> {
    page: CorePage,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageBoundaries<'a> {
    pub(crate) fn new(page: CorePage) -> Self {
        Self {
            page,
            lifetime: PhantomData,
        }
    }
    fn read(
        &self,
        key: &[u8],
        box_type: PdfPageBoundaryBoxType,
    ) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.page.check_live()?;
        let values = sdk::bounding_box(&self.page.state.core.borrow(), key).ok_or(
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::Unknown),
        )?;
        Ok(PdfPageBoundaryBox {
            box_type,
            bounds: PdfRect::new_from_values(values[1], values[0], values[3], values[2]),
        })
    }
    fn write(&mut self, key: &[u8], rect: PdfRect) -> Result<(), PdfiumError> {
        self.page.edit(|page| {
            sdk::set_bounding_box(page, key, rect.as_core());
            Ok(())
        })
    }
}
impl<'a> PdfPageBoundaries<'a> {
    pub fn get(&self, boundary: PdfPageBoundaryBoxType) -> Result<PdfPageBoundaryBox, PdfiumError> {
        match boundary {
            PdfPageBoundaryBoxType::Media => self.media(),
            PdfPageBoundaryBoxType::Art => self.art(),
            PdfPageBoundaryBoxType::Bleed => self.bleed(),
            PdfPageBoundaryBoxType::Trim => self.trim(),
            PdfPageBoundaryBoxType::Crop => self.crop(),
            PdfPageBoundaryBoxType::Bounding => self.bounding(),
        }
    }
    pub fn set(
        &mut self,
        box_type: PdfPageBoundaryBoxType,
        rect: PdfRect,
    ) -> Result<(), PdfiumError> {
        match box_type {
            PdfPageBoundaryBoxType::Media => self.set_media(rect),
            PdfPageBoundaryBoxType::Art => self.set_art(rect),
            PdfPageBoundaryBoxType::Bleed => self.set_bleed(rect),
            PdfPageBoundaryBoxType::Trim => self.set_trim(rect),
            PdfPageBoundaryBoxType::Crop => self.set_crop(rect),
            PdfPageBoundaryBoxType::Bounding => Ok(()),
        }
    }
    pub fn media(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.read(b"MediaBox", PdfPageBoundaryBoxType::Media)
    }
    pub fn set_media(&mut self, rect: PdfRect) -> Result<(), PdfiumError> {
        self.write(b"MediaBox", rect)
    }
    pub fn art(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.read(b"ArtBox", PdfPageBoundaryBoxType::Art)
    }
    pub fn set_art(&mut self, rect: PdfRect) -> Result<(), PdfiumError> {
        self.write(b"ArtBox", rect)
    }
    pub fn bleed(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.read(b"BleedBox", PdfPageBoundaryBoxType::Bleed)
    }
    pub fn set_bleed(&mut self, rect: PdfRect) -> Result<(), PdfiumError> {
        self.write(b"BleedBox", rect)
    }
    pub fn trim(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.read(b"TrimBox", PdfPageBoundaryBoxType::Trim)
    }
    pub fn set_trim(&mut self, rect: PdfRect) -> Result<(), PdfiumError> {
        self.write(b"TrimBox", rect)
    }
    pub fn crop(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.read(b"CropBox", PdfPageBoundaryBoxType::Crop)
    }
    pub fn set_crop(&mut self, rect: PdfRect) -> Result<(), PdfiumError> {
        self.write(b"CropBox", rect)
    }
    pub fn bounding(&self) -> Result<PdfPageBoundaryBox, PdfiumError> {
        self.page.check_live()?;
        Ok(PdfPageBoundaryBox {
            box_type: PdfPageBoundaryBoxType::Bounding,
            bounds: PdfRect::from_core(self.page.state.core.borrow().bbox()),
        })
    }
    pub fn iter(&'a self) -> PageBoundaryIterator<'a> {
        PageBoundaryIterator {
            boundaries: self,
            next_index: 0,
        }
    }
}
pub struct PageBoundaryIterator<'a> {
    boundaries: &'a PdfPageBoundaries<'a>,
    next_index: usize,
}
impl<'a> Iterator for PageBoundaryIterator<'a> {
    type Item = PdfPageBoundaryBox;
    fn next(&mut self) -> Option<Self::Item> {
        const ORDER: [PdfPageBoundaryBoxType; 6] = [
            PdfPageBoundaryBoxType::Media,
            PdfPageBoundaryBoxType::Art,
            PdfPageBoundaryBoxType::Bleed,
            PdfPageBoundaryBoxType::Trim,
            PdfPageBoundaryBoxType::Crop,
            PdfPageBoundaryBoxType::Bounding,
        ];
        while let Some(&kind) = ORDER.get(self.next_index) {
            self.next_index += 1;
            if let Ok(value) = self.boundaries.get(kind) {
                return Some(value);
            }
        }
        None
    }
}
