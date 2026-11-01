// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{backend, prelude::*};
use pdfoxium_core::{
    fpdfapi::parser::cpdf_object::ObjectRef,
    fpdfdoc::{cpdf_link::CpdfLink, cpdf_linklist::CpdfLinkList},
    fpdfsdk::fpdf_annot,
    fxcrt::fx_coordinates::CfxPointF,
};
use std::{
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
pub struct PdfLink<'a> {
    document: Rc<backend::document::DocumentState>,
    core: CpdfLink,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfLink<'a> {
    pub(crate) fn from_core(
        document: Rc<backend::document::DocumentState>,
        dict: ObjectRef,
    ) -> Self {
        Self {
            document,
            core: CpdfLink::new(Some(dict)),
            lifetime: PhantomData,
        }
    }
}
impl<'a> PdfLink<'a> {
    pub fn action(&self) -> Option<PdfAction<'a>> {
        PdfAction::from_core(self.document.clone(), self.core.action())
    }
    pub fn destination(&self) -> Option<PdfDestination<'a>> {
        PdfDestination::from_core(
            self.document.clone(),
            self.core.dest(&self.document.core.borrow()),
        )
    }
    pub fn rect(&self) -> Result<PdfRect, PdfiumError> {
        Ok(PdfRect::from_core(self.core.rect()))
    }
}
pub type PdfPageLinkIndex = usize;
pub struct PdfPageLinks<'a> {
    page: backend::PdfPage,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageLinks<'a> {
    pub(crate) fn new(page: backend::PdfPage) -> Self {
        Self {
            page,
            lifetime: PhantomData,
        }
    }
}
impl<'a> PdfPageLinks<'a> {
    pub fn len(&self) -> PdfPageLinkIndex {
        let page = self.page.state.core.borrow();
        (0..fpdf_annot::annotation_count(&page))
            .filter_map(|i| fpdf_annot::annotation_at(&page, i))
            .filter(|a| fpdf_annot::subtype(a) as i32 == 2)
            .count()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageLinkIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageLinkIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&'a self, index: PdfPageLinkIndex) -> Result<PdfLink<'a>, PdfiumError> {
        self.page.check_live()?;
        let page = self.page.state.core.borrow();
        let dict = (0..fpdf_annot::annotation_count(&page))
            .filter_map(|i| fpdf_annot::annotation_at(&page, i))
            .filter(|a| fpdf_annot::subtype(a) as i32 == 2)
            .nth(index)
            .ok_or(PdfiumError::LinkIndexOutOfBounds)?;
        Ok(PdfLink::from_core(self.page.document.clone(), dict))
    }
    pub fn first(&'a self) -> Result<PdfLink<'a>, PdfiumError> {
        if self.is_empty() {
            Err(PdfiumError::NoPageLinksInCollection)
        } else {
            self.get(0)
        }
    }
    pub fn last(&'a self) -> Result<PdfLink<'a>, PdfiumError> {
        if self.is_empty() {
            Err(PdfiumError::NoPageLinksInCollection)
        } else {
            self.get(self.len() - 1)
        }
    }
    pub fn link_at_point(&self, x: PdfPoints, y: PdfPoints) -> Option<PdfLink<'_>> {
        self.page.check_live().ok()?;
        let (core, _) = CpdfLinkList::new().get_link_at_point(
            &self.page.state.core.borrow(),
            CfxPointF::new(x.value, y.value),
        )?;
        Some(PdfLink {
            document: self.page.document.clone(),
            core,
            lifetime: PhantomData,
        })
    }
    pub fn iter(&self) -> PdfPageLinksIterator<'_> {
        PdfPageLinksIterator {
            links: self,
            next_index: 0,
        }
    }
}
pub struct PdfPageLinksIterator<'a> {
    links: &'a PdfPageLinks<'a>,
    next_index: usize,
}
impl<'a> Iterator for PdfPageLinksIterator<'a> {
    type Item = PdfLink<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let result = self.links.get(self.next_index).ok();
        if result.is_some() {
            self.next_index += 1;
        }
        result
    }
}
