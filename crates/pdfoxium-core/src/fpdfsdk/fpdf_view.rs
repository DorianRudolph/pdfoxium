// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Additional document metadata queries from fpdfsdk/fpdf_view.cpp.
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, ObjectRef};

/// FPDF_GetFormType classifies the document dictionaries without loading an
/// XFA runtime. Even an empty AcroForm is an AcroForm, as in the source.
pub fn form_type(root: Option<&ObjectRef>) -> i32 {
    let Some(root) = root else {
        return 0;
    };
    let acro = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|d| d.get_object_for(b"AcroForm"));
    let Some(acro) = acro.and_then(|o| get_dict_object(&o)) else {
        return 0;
    };
    let has_xfa = acro
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|d| d.key_exist(b"XFA"));
    if !has_xfa {
        return 1;
    }
    let needs_rendering = root
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|d| d.get_boolean_for(b"NeedsRendering", false));
    if needs_rendering {
        2
    } else {
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    #[test]
    fn acroform_and_xfa_metadata_types_do_not_require_loading_runtime() {
        assert_eq!(form_type(None), 0);
        let root = retain_object(CpdfDictionary::new());
        assert_eq!(form_type(Some(&root)), 0);
        let acro = retain_object(CpdfDictionary::new());
        root.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"AcroForm", Some(acro.clone()));
        assert_eq!(form_type(Some(&root)), 1);
        acro.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"XFA", CpdfScalarObject::null());
        assert_eq!(form_type(Some(&root)), 3);
        root.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"NeedsRendering", CpdfScalarObject::boolean(true));
        assert_eq!(form_type(Some(&root)), 2);
    }
}
