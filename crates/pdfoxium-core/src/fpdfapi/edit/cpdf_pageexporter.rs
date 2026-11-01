// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2024 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/edit/cpdf_pageexporter.{h,cpp}`: one-shot page copying. Failure
//! preserves preceding mutations, including the source's create-before-lookup.

use super::cpdf_pageorganizer::{dict, dict_mut, CpdfPageOrganizer};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fxcrt::bytestring::eq_cstr;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use std::cell::RefCell;
use std::rc::Rc;

pub struct CpdfPageExporter {
    organizer: CpdfPageOrganizer,
}
impl CpdfPageExporter {
    pub fn new(dest: Rc<RefCell<CpdfDocument>>, src: Rc<RefCell<CpdfDocument>>) -> Self {
        Self {
            organizer: CpdfPageOrganizer::new(dest, src),
        }
    }
    pub fn export_pages(&mut self, page_indices: &[u32], index: i32) -> bool {
        if !self.organizer.init() {
            return false;
        }
        let mut current = index;
        for &page_index in page_indices {
            let dest = self.organizer.dest.borrow_mut().create_new_page(current);
            let src = self
                .organizer
                .src
                .borrow_mut()
                .page_dictionary(page_index as usize);
            let (Some(dest), Some(src)) = (dest, src) else {
                return false;
            };
            dict(&src, |d| {
                d.with_locked_entries(|entries| {
                    for (key, value) in entries {
                        if eq_cstr(key, b"Type") || eq_cstr(key, b"Parent") {
                            continue;
                        }
                        let cloned = value.borrow().clone_object();
                        dict_mut(&dest, |d| {
                            d.set_for(key, Some(cloned));
                        });
                    }
                })
            });
            if !CpdfPageOrganizer::copy_inheritable(&dest, &src, b"MediaBox") {
                if let Some(crop) =
                    CpdfPageOrganizer::page_dict_get_inheritable_tag(&src, b"CropBox")
                {
                    let cloned = crop.borrow().clone_object();
                    dict_mut(&dest, |d| {
                        d.set_for(b"MediaBox", Some(cloned));
                    });
                } else {
                    dict_mut(&dest, |d| {
                        d.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 612.0, 792.0))
                    });
                }
            }
            if !CpdfPageOrganizer::copy_inheritable(&dest, &src, b"Resources") {
                dict_mut(&dest, |d| {
                    d.set_new_for(
                        b"Resources",
                        CpdfDictionary::with_pool(d.byte_string_pool()),
                    );
                });
            }
            CpdfPageOrganizer::copy_inheritable(&dest, &src, b"CropBox");
            CpdfPageOrganizer::copy_inheritable(&dest, &src, b"Rotate");
            self.organizer
                .add_object_mapping(src.borrow().obj_num(), dest.borrow().obj_num());
            self.organizer.update_reference(&dest);
            current = current
                .checked_add(1)
                .expect("native signed page index overflow");
        }
        self.organizer.copy_optional_content()
    }
}
