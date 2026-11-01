// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/parser/cpdf_page_object_avail.{h,cpp}`.

use std::rc::Rc;

use super::cpdf_cross_ref_avail::DocAvailStatus;
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::HolderRef;
use super::cpdf_object::ObjectRef;
use super::cpdf_object_avail::CpdfObjectAvail;
use super::cpdf_read_validator::CpdfReadValidator;
use super::fpdf_parser_utility::validate_dict_type;

pub struct CpdfPageObjectAvail {
    inner: CpdfObjectAvail,
}

impl CpdfPageObjectAvail {
    pub fn from_root(
        validator: Rc<CpdfReadValidator>,
        holder: &HolderRef,
        root: ObjectRef,
    ) -> Self {
        Self::with_exclusion(CpdfObjectAvail::from_root(validator, holder, root))
    }

    pub fn from_obj_num(
        validator: Rc<CpdfReadValidator>,
        holder: &HolderRef,
        obj_num: u32,
    ) -> Self {
        Self::with_exclusion(CpdfObjectAvail::from_obj_num(validator, holder, obj_num))
    }

    fn with_exclusion(mut avail: CpdfObjectAvail) -> Self {
        avail.set_exclude_object(Some(Rc::new(|object| {
            let borrowed = object.borrow();
            validate_dict_type(borrowed.as_any().downcast_ref::<CpdfDictionary>(), b"Page")
        })));
        Self { inner: avail }
    }

    pub fn check_avail(&mut self) -> DocAvailStatus {
        self.inner.check_avail()
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::CpdfPageObjectAvail;
    use crate::fpdfapi::parser::cpdf_cross_ref_avail::DocAvailStatus;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_read_validator::CpdfReadValidator;
    use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    #[test]
    fn excludes_nested_page_dictionary() {
        let holder = CpdfIndirectObjectHolder::new();
        let mut page = CpdfDictionary::new();
        page.set_new_name_for(b"Type", b"Page");
        page.set_for(
            b"Contents",
            Some(retain_object(CpdfReference::new(&holder, 999))),
        );
        let mut root = CpdfDictionary::new();
        root.set_for(b"OtherPage", Some(retain_object(page)));
        let file = Rc::new(CfxReadOnlyContainerStream::new(Vec::new()));
        let validator = Rc::new(CpdfReadValidator::new(file, None));
        let mut avail = CpdfPageObjectAvail::from_root(validator, &holder, retain_object(root));
        assert_eq!(avail.check_avail(), DocAvailStatus::DataAvailable);
    }
}

#[cfg(test)]
#[path = "cpdf_page_object_avail_unittest.rs"]
mod upstream_tests;
