use crate::backend::prelude::*;
use pdfoxium_core::{
    fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences, fpdftext::cpdf_textpage::CpdfTextPage,
    fxcrt::fx_coordinates::CfxFloatRect,
};
use std::rc::Rc;
pub(crate) fn decode(units: &[u32]) -> String {
    let mut result = String::new();
    let mut i = 0;
    while i < units.len() {
        let mut c = units[i];
        if (0xd800..=0xdbff).contains(&c)
            && units
                .get(i + 1)
                .is_some_and(|c| (0xdc00..=0xdfff).contains(c))
        {
            c = 0x10000 + ((c - 0xd800) << 10) + units[i + 1] - 0xdc00;
            i += 1;
        }
        result.push(char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER));
        i += 1;
    }
    result
}
impl PdfRect {
    pub(crate) fn from_core(r: CfxFloatRect) -> Self {
        Self::new_from_values(r.bottom, r.left, r.top, r.right)
    }
    pub(crate) fn as_core(&self) -> CfxFloatRect {
        CfxFloatRect::new(
            self.left().value,
            self.bottom().value,
            self.right().value,
            self.top().value,
        )
    }
}
pub(crate) struct PdfPageText {
    pub(crate) core: Rc<CpdfTextPage>,
}
impl PdfPageText {
    pub(crate) fn new(page: &PdfPage) -> Result<Self, PdfiumError> {
        if let Some(core) = page.state.text_cache.borrow().as_ref() {
            return Ok(Self { core: core.clone() });
        }
        let rtl = CpdfViewerPreferences::new(&page.document.core.borrow()).is_direction_r2l();
        let core = Rc::new(CpdfTextPage::new(&page.state.core.borrow(), rtl));
        *page.state.text_cache.borrow_mut() = Some(core.clone());
        Ok(Self { core })
    }
}
