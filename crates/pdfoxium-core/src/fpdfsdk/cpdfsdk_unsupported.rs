// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Portable unsupported-feature reporting from `fpdfsdk/cpdfsdk_helpers.cpp`
//! and page mode from `fpdfsdk/fpdf_ext.cpp`. Callbacks never retain model borrows.
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfdoc::cpdf_annot::Subtype;
use crate::fpdfdoc::cpdf_metadata::{CpdfMetadata, UnsupportedFeature};
use std::collections::HashSet;
use std::rc::Rc;

/// Shared SDK observer; C ABI descriptors remain outside the safe engine.
pub type UnsupportedHandler = Rc<dyn Fn(UnsupportedFeature) -> bool>;
thread_local! {
    static UNSUPPORTED_HANDLER: std::cell::RefCell<Option<UnsupportedHandler>> = const { std::cell::RefCell::new(None) };
}
pub fn set_unsupported_handler(handler: Option<UnsupportedHandler>) -> Option<UnsupportedHandler> {
    UNSUPPORTED_HANDLER.with(|slot| slot.replace(handler))
}
pub fn raise_unsupported_error(feature: UnsupportedFeature) -> bool {
    let callback = UNSUPPORTED_HANDLER.with(|slot| slot.borrow().clone());
    callback.is_some_and(|callback| callback(feature))
}
pub fn notify_unsupported_annot(subtype: Subtype, dict: &ObjectRef) {
    check_for_unsupported_annot(subtype, dict, &mut |feature| {
        raise_unsupported_error(feature);
    });
}

pub const PAGEMODE_UNKNOWN: i32 = -1;
pub const PAGEMODE_USENONE: i32 = 0;
pub const PAGEMODE_USEOUTLINES: i32 = 1;
pub const PAGEMODE_USETHUMBS: i32 = 2;
pub const PAGEMODE_FULLSCREEN: i32 = 3;
pub const PAGEMODE_USEOC: i32 = 4;
pub const PAGEMODE_USEATTACHMENTS: i32 = 5;
fn raw(dict: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
fn has(dict: &ObjectRef, key: &[u8]) -> bool {
    dict.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|d| d.key_exist(key))
}
fn direct(object: &ObjectRef) -> Option<ObjectRef> {
    let mut object = object.clone();
    let mut seen = HashSet::new();
    loop {
        let reference = object.borrow().object_type() == ObjectType::Reference;
        if !reference {
            return Some(object);
        }
        if !seen.insert(Rc::as_ptr(&object) as *const () as usize) {
            return None;
        }
        object = get_direct_object(&object)?;
    }
}
fn typed(dict: &ObjectRef, key: &[u8], kind: ObjectType) -> Option<ObjectRef> {
    let object = direct(&raw(dict, key)?)?;
    let matches = object.borrow().object_type() == kind;
    matches.then_some(object)
}
fn bytes(object: &ObjectRef) -> Vec<u8> {
    direct(object).map_or_else(Vec::new, |o| o.borrow().get_string())
}

pub fn get_page_mode(root: Option<ObjectRef>) -> i32 {
    let Some(root) = root else {
        return PAGEMODE_UNKNOWN;
    };
    let Some(mode) = raw(&root, b"PageMode") else {
        return PAGEMODE_USENONE;
    };
    let mode = bytes(&mode);
    if mode.is_empty() || mode.eq_ignore_ascii_case(b"UseNone") {
        return PAGEMODE_USENONE;
    }
    for (name, value) in [
        (b"UseOutlines".as_slice(), PAGEMODE_USEOUTLINES),
        (b"UseThumbs", PAGEMODE_USETHUMBS),
        (b"FullScreen", PAGEMODE_FULLSCREEN),
        (b"UseOC", PAGEMODE_USEOC),
        (b"UseAttachments", PAGEMODE_USEATTACHMENTS),
    ] {
        if mode.eq_ignore_ascii_case(name) {
            return value;
        }
    }
    PAGEMODE_UNKNOWN
}
pub fn report_unsupported_features(
    root: Option<ObjectRef>,
    report: &mut dyn FnMut(UnsupportedFeature),
) {
    let Some(root) = root else {
        return;
    };
    if has(&root, b"Collection") {
        report(UnsupportedFeature::DocumentPortableCollection);
    }
    if let Some(names) = typed(&root, b"Names", ObjectType::Dictionary) {
        if has(&names, b"EmbeddedFiles") {
            report(UnsupportedFeature::DocumentAttachment);
        }
        if let Some(javascript) = typed(&names, b"JavaScript", ObjectType::Dictionary) {
            if let Some(array) = typed(&javascript, b"Names", ObjectType::Array) {
                let count = array
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .unwrap()
                    .size();
                for index in 0..count {
                    let object = array
                        .borrow()
                        .as_any()
                        .downcast_ref::<CpdfArray>()
                        .unwrap()
                        .get_object_at(index);
                    if object
                        .is_some_and(|o| bytes(&o) == b"com.adobe.acrobat.SharedReview.Register")
                    {
                        report(UnsupportedFeature::DocumentSharedReview);
                        break;
                    }
                }
            }
        }
    }
    if let Some(metadata) = typed(&root, b"Metadata", ObjectType::Stream) {
        for feature in CpdfMetadata::new(metadata).check_for_shared_form() {
            report(feature);
        }
    }
}
pub fn doc_has_xfa(root: Option<&ObjectRef>) -> bool {
    root.and_then(|root| typed(root, b"AcroForm", ObjectType::Dictionary))
        .and_then(|form| typed(&form, b"XFA", ObjectType::Array))
        .is_some()
}
pub fn report_unsupported_xfa(
    root: Option<ObjectRef>,
    has_extension: bool,
    report: &mut dyn FnMut(UnsupportedFeature),
) {
    if !has_extension && doc_has_xfa(root.as_ref()) {
        report(UnsupportedFeature::DocumentXfaForm);
    }
}
pub fn check_for_unsupported_annot(
    subtype: Subtype,
    dict: &ObjectRef,
    report: &mut dyn FnMut(UnsupportedFeature),
) {
    let feature = match subtype {
        Subtype::FileAttachment => Some(UnsupportedFeature::AnnotationAttachment),
        Subtype::Movie => Some(UnsupportedFeature::AnnotationMovie),
        Subtype::RichMedia => Some(UnsupportedFeature::AnnotationScreenRichMedia),
        Subtype::Screen => {
            if raw(dict, b"IT").is_some_and(|o| bytes(&o) == b"Img") {
                None
            } else {
                Some(UnsupportedFeature::AnnotationScreenMedia)
            }
        }
        Subtype::Sound => Some(UnsupportedFeature::AnnotationSound),
        Subtype::ThreeD => Some(UnsupportedFeature::Annotation3d),
        Subtype::Widget => raw(dict, b"FT")
            .filter(|o| bytes(o) == b"Sig")
            .map(|_| UnsupportedFeature::AnnotationSignature),
        _ => None,
    };
    if let Some(feature) = feature {
        report(feature);
    }
}

#[cfg(test)]
#[path = "cpdfsdk_unsupported_tests.rs"]
mod tests;
