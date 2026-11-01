// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use pdfoxium_core::fpdfapi::parser::cpdf_object::retain_object;
struct Restore(
    *mut UNSUPPORT_INFO,
    Option<cpdfsdk_unsupported::UnsupportedHandler>,
);
impl Restore {
    fn new() -> Self {
        Self(
            UNSUPPORT_HANDLER.with(Cell::get),
            cpdfsdk_unsupported::set_unsupported_handler(None),
        )
    }
}
impl Drop for Restore {
    fn drop(&mut self) {
        UNSUPPORT_HANDLER.with(|h| h.set(self.0));
        cpdfsdk_unsupported::set_unsupported_handler(self.1.take());
    }
}
#[repr(C)]
struct Capture {
    info: UNSUPPORT_INFO,
    document: *mut c_void,
    events: Vec<c_int>,
    replacement: *mut UNSUPPORT_INFO,
}
unsafe extern "C" fn notify(info: *mut UNSUPPORT_INFO, feature: c_int) {
    let capture = info.cast::<Capture>();
    unsafe {
        (*capture).events.push(feature);
    }
    let document = unsafe { (*capture).document };
    if !document.is_null() {
        assert_eq!(unsafe { FPDFDoc_GetPageMode(document) }, PAGEMODE_USENONE);
        assert_eq!(unsafe { FPDF_GetPageCount(document) }, 0);
    }
    let replacement = unsafe { (*capture).replacement };
    if !replacement.is_null() {
        unsafe {
            (*capture).replacement = std::ptr::null_mut();
            assert_eq!(FSDK_SetUnSpObjProcessHandler(replacement), 1);
        }
    }
}
#[test]
fn callback_registration_reentry_replacement_and_null_callback() {
    unsafe {
        let _restore = Restore::new();
        for (feature, value) in [
            (UnsupportedFeature::DocumentXfaForm, FPDF_UNSP_DOC_XFAFORM),
            (
                UnsupportedFeature::DocumentPortableCollection,
                FPDF_UNSP_DOC_PORTABLECOLLECTION,
            ),
            (
                UnsupportedFeature::DocumentAttachment,
                FPDF_UNSP_DOC_ATTACHMENT,
            ),
            (UnsupportedFeature::DocumentSecurity, FPDF_UNSP_DOC_SECURITY),
            (
                UnsupportedFeature::DocumentSharedReview,
                FPDF_UNSP_DOC_SHAREDREVIEW,
            ),
            (
                UnsupportedFeature::DocumentSharedFormAcrobat,
                FPDF_UNSP_DOC_SHAREDFORM_ACROBAT,
            ),
            (
                UnsupportedFeature::DocumentSharedFormFilesystem,
                FPDF_UNSP_DOC_SHAREDFORM_FILESYSTEM,
            ),
            (
                UnsupportedFeature::DocumentSharedFormEmail,
                FPDF_UNSP_DOC_SHAREDFORM_EMAIL,
            ),
            (UnsupportedFeature::Annotation3d, FPDF_UNSP_ANNOT_3DANNOT),
            (UnsupportedFeature::AnnotationMovie, FPDF_UNSP_ANNOT_MOVIE),
            (UnsupportedFeature::AnnotationSound, FPDF_UNSP_ANNOT_SOUND),
            (
                UnsupportedFeature::AnnotationScreenMedia,
                FPDF_UNSP_ANNOT_SCREEN_MEDIA,
            ),
            (
                UnsupportedFeature::AnnotationScreenRichMedia,
                FPDF_UNSP_ANNOT_SCREEN_RICHMEDIA,
            ),
            (
                UnsupportedFeature::AnnotationAttachment,
                FPDF_UNSP_ANNOT_ATTACHMENT,
            ),
            (UnsupportedFeature::AnnotationSignature, FPDF_UNSP_ANNOT_SIG),
        ] {
            assert_eq!(feature as c_int, value);
        }

        UNSUPPORT_HANDLER.with(|h| h.set(std::ptr::null_mut()));
        assert!(!raise_unsupported_error(
            UnsupportedFeature::DocumentSecurity
        ));
        assert_eq!(FSDK_SetUnSpObjProcessHandler(std::ptr::null_mut()), 0);
        let document = FPDF_CreateNewDocument();
        let owner = (*document.cast::<DocumentHandle>()).document.clone();
        let root = owner.borrow().root().unwrap();
        root.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"Collection", Some(retain_object(CpdfDictionary::new())));
        let mut second = Capture {
            info: UNSUPPORT_INFO {
                version: 1,
                FSDK_UnSupport_Handler: Some(notify),
            },
            document,
            events: Vec::new(),
            replacement: std::ptr::null_mut(),
        };
        let mut first = Capture {
            info: UNSUPPORT_INFO {
                version: 1,
                FSDK_UnSupport_Handler: Some(notify),
            },
            document,
            events: Vec::new(),
            replacement: &mut second.info,
        };
        assert_eq!(FSDK_SetUnSpObjProcessHandler(&mut first.info), 1);
        let mut invalid = UNSUPPORT_INFO {
            version: 2,
            FSDK_UnSupport_Handler: Some(notify),
        };
        assert_eq!(FSDK_SetUnSpObjProcessHandler(&mut invalid), 0);
        report_unsupported_features(&owner);
        assert_eq!(first.events, [FPDF_UNSP_DOC_PORTABLECOLLECTION]);
        assert!(second.events.is_empty());
        report_unsupported_features(&owner);
        assert_eq!(second.events, [FPDF_UNSP_DOC_PORTABLECOLLECTION]);
        cpdfsdk_unsupported::notify_unsupported_annot(
            pdfoxium_core::fpdfdoc::cpdf_annot::Subtype::Movie,
            &root,
        );
        assert_eq!(
            second.events,
            [FPDF_UNSP_DOC_PORTABLECOLLECTION, FPDF_UNSP_ANNOT_MOVIE]
        );
        let mut empty = UNSUPPORT_INFO {
            version: 1,
            FSDK_UnSupport_Handler: None,
        };
        assert_eq!(FSDK_SetUnSpObjProcessHandler(&mut empty), 1);
        assert!(raise_unsupported_error(
            UnsupportedFeature::DocumentSecurity
        ));
        FPDF_CloseDocument(document);
    }
}
#[test]
fn native_page_mode_embedder_cases_and_other_modes() {
    unsafe {
        // FPDFExtEmbedderTest.PageModeUnknown, PageModeUseNone and PageModeUseOutlines.
        assert_eq!(FPDFDoc_GetPageMode(std::ptr::null_mut()), PAGEMODE_UNKNOWN);
        for (file, expected) in [
            ("hello_world.pdf", PAGEMODE_USENONE),
            ("use_outlines.pdf", PAGEMODE_USEOUTLINES),
        ] {
            let path = std::ffi::CString::new(format!(
                "{}/../../upstream/pdfium/testing/resources/{file}",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap();
            let document = FPDF_LoadDocument(path.as_ptr(), std::ptr::null());
            assert!(!document.is_null());
            assert_eq!(FPDFDoc_GetPageMode(document), expected);
            FPDF_CloseDocument(document);
        }
        let document = FPDF_CreateNewDocument();
        let owner = (*document.cast::<DocumentHandle>()).document.clone();
        let root = owner.borrow().root().unwrap();
        for (mode, expected) in [
            (b"UseThumbs".as_slice(), 2),
            (b"FullScreen", 3),
            (b"UseOC", 4),
            (b"UseAttachments", 5),
            (b"UsEnOnE", 0),
            (b"bad", -1),
        ] {
            root.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_name_for(b"PageMode", mode);
            assert_eq!(FPDFDoc_GetPageMode(document), expected);
        }
        FPDF_CloseDocument(document);
    }
}
