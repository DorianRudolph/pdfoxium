// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Document-owned link hit-test cache from `core/fpdfdoc/cpdf_linklist.{h,cpp}`.
use super::cpdf_link::CpdfLink;
use crate::fpdfapi::{
    page::cpdf_page::CpdfPage,
    parser::{cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::ObjectRef},
};
use crate::fxcrt::fx_coordinates::CfxPointF;
use std::collections::BTreeMap;
#[derive(Default)]
pub struct CpdfLinkList {
    page_map: BTreeMap<u32, Vec<Option<ObjectRef>>>,
}
impl CpdfLinkList {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn invalidate_page(&mut self, page: &CpdfPage) {
        self.page_map
            .remove(&page.holder().dict().borrow().obj_num());
    }
    pub fn get_link_at_point(
        &mut self,
        page: &CpdfPage,
        point: CfxPointF,
    ) -> Option<(CpdfLink, i32)> {
        let links = self.get_page_links(page)?;
        for (index, dict) in links.iter().enumerate().rev() {
            let Some(dict) = dict else {
                continue;
            };
            let link = CpdfLink::new(Some(dict.clone()));
            if link.rect().contains_point(point) {
                return Some((link, i32::try_from(index).ok()?));
            }
        }
        None
    }
    fn get_page_links(&mut self, page: &CpdfPage) -> Option<&[Option<ObjectRef>]> {
        let number = page.holder().dict().borrow().obj_num();
        if number == 0 {
            return None;
        }
        Some(self.page_map.entry(number).or_insert_with(|| {
            let Some(annots) = page.annots_array() else {
                return Vec::new();
            };
            let borrowed = annots.borrow();
            let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
                return Vec::new();
            };
            (0..array.size())
                .map(|index| {
                    array.get_dict_at(index).filter(|dict| {
                        dict.borrow()
                            .as_any()
                            .downcast_ref::<CpdfDictionary>()
                            .is_some_and(|dict| {
                                dict.get_byte_string_for(b"Subtype", b"") == b"Link"
                            })
                    })
                })
                .collect()
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{cpdf_document::CpdfDocument, cpdf_object::retain_object};
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    #[test]
    fn links_preserve_annotation_z_order_and_last_matching_link_wins() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let dict = document.create_new_page(0).unwrap();
        let mut annots = CpdfArray::new();
        let mut wanted = None;
        for (subtype, rect) in [
            (b"Link".as_slice(), CfxFloatRect::new(0., 0., 20., 20.)),
            (b"Square", CfxFloatRect::new(0., 0., 20., 20.)),
            (b"Link", CfxFloatRect::new(5., 5., 15., 15.)),
        ] {
            let mut annot = CpdfDictionary::new();
            annot.set_new_name_for(b"Subtype", subtype);
            annot.set_rect_for(b"Rect", rect);
            let annot = retain_object(annot);
            wanted = Some(annot.clone());
            annots.append(annot);
        }
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Annots", annots);
        let page = CpdfPage::new(dict).unwrap();
        let mut links = CpdfLinkList::new();
        assert!(links
            .get_link_at_point(&page, CfxPointF::new(40., 40.))
            .is_none());
        let (link, index) = links
            .get_link_at_point(&page, CfxPointF::new(10., 10.))
            .unwrap();
        assert_eq!(index, 2);
        assert!(std::rc::Rc::ptr_eq(&link.dict().unwrap(), &wanted.unwrap()));
        assert_eq!(
            links
                .get_link_at_point(&page, CfxPointF::new(1., 1.))
                .unwrap()
                .1,
            0
        );
        links.invalidate_page(&page);
        assert_eq!(
            links
                .get_link_at_point(&page, CfxPointF::new(10., 10.))
                .unwrap()
                .1,
            2
        );
    }
    #[test]
    fn invalidating_after_removal_refreshes_cached_links_and_z_order() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let dict = document.create_new_page(0).unwrap();
        let mut annots = CpdfArray::new();
        let mut retained = Vec::new();
        for (subtype, rect) in [
            (b"Square".as_slice(), CfxFloatRect::new(0., 0., 20., 20.)),
            (b"Link", CfxFloatRect::new(0., 0., 10., 10.)),
            (b"Link", CfxFloatRect::new(5., 5., 15., 15.)),
        ] {
            let mut annot = CpdfDictionary::new();
            annot.set_new_name_for(b"Subtype", subtype);
            annot.set_rect_for(b"Rect", rect);
            let annot = retain_object(annot);
            retained.push(annot.clone());
            annots.append(annot);
        }
        let annots = dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Annots", annots);
        let page = CpdfPage::new(dict).unwrap();
        let mut links = CpdfLinkList::new();
        let overlap = CfxPointF::new(7., 7.);
        // The second query exercises the populated cache, including the slot
        // retained for the non-link annotation.
        for _ in 0..2 {
            let (link, index) = links.get_link_at_point(&page, overlap).unwrap();
            assert_eq!(index, 2);
            assert!(std::rc::Rc::ptr_eq(&link.dict().unwrap(), &retained[2]));
        }
        assert!(links
            .get_link_at_point(&page, CfxPointF::new(12., 12.))
            .is_some());
        annots
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfArray>()
            .unwrap()
            .remove_at(2);
        links.invalidate_page(&page);
        let (link, index) = links.get_link_at_point(&page, overlap).unwrap();
        assert_eq!(index, 1);
        assert!(std::rc::Rc::ptr_eq(&link.dict().unwrap(), &retained[1]));
        assert!(links
            .get_link_at_point(&page, CfxPointF::new(12., 12.))
            .is_none());
    }
}
