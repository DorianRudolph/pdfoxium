// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
#[test]
fn document_reports_preserve_order_use_real_metadata_and_allow_model_reentry() {
    let root = retain_object(CpdfDictionary::new());
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"Collection", Some(retain_object(CpdfDictionary::new())));
    let holder = CpdfIndirectObjectHolder::new();
    let stream=retain_object(CpdfStream::from_bytes(b"<node xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>0</adhocwf:workflowType><child xmlns:adhocwf=\"http://ns.adobe.com/AcrobatAdhocWorkflow/1.0/\"><adhocwf:workflowType>2</adhocwf:workflowType></child></node>"));
    let number = holder.borrow_mut().add_indirect_object(stream);
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(
            b"Metadata",
            Some(retain_object(CpdfReference::new(&holder, number))),
        );
    let mut received = Vec::new();
    report_unsupported_features(Some(root.clone()), &mut |feature| {
        received.push(feature);
        // Adding names during the first notification must affect the next
        // native branch, without a dictionary/document borrow panic.
        if feature == UnsupportedFeature::DocumentPortableCollection {
            let mut jsnames = CpdfArray::new();
            jsnames.append_new_string(b"com.adobe.acrobat.SharedReview.Register");
            jsnames.append_new_dictionary();
            let mut js = CpdfDictionary::new();
            js.set_for(b"Names", Some(retain_object(jsnames)));
            let mut names = CpdfDictionary::new();
            names.set_for(b"EmbeddedFiles", Some(retain_object(CpdfDictionary::new())));
            names.set_for(b"JavaScript", Some(retain_object(js)));
            root.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_for(b"Names", Some(retain_object(names)));
        }
    });
    assert_eq!(
        received,
        [
            UnsupportedFeature::DocumentPortableCollection,
            UnsupportedFeature::DocumentAttachment,
            UnsupportedFeature::DocumentSharedReview,
            UnsupportedFeature::DocumentSharedFormEmail,
            UnsupportedFeature::DocumentSharedFormFilesystem
        ]
    );
    assert!(!doc_has_xfa(Some(&root)));
    let mut form = CpdfDictionary::new();
    form.set_for(b"XFA", Some(retain_object(CpdfArray::new())));
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"AcroForm", Some(retain_object(form)));
    assert!(doc_has_xfa(Some(&root)));
    let mut events = Vec::new();
    report_unsupported_xfa(Some(root.clone()), true, &mut |f| events.push(f));
    assert!(events.is_empty());
    report_unsupported_xfa(Some(root), false, &mut |f| events.push(f));
    assert_eq!(events, [UnsupportedFeature::DocumentXfaForm]);
}
#[test]
fn annotation_filters_and_all_page_modes_match_source() {
    let dict = retain_object(CpdfDictionary::new());
    let mut events = Vec::new();
    for (subtype, expected) in [
        (
            Subtype::FileAttachment,
            Some(UnsupportedFeature::AnnotationAttachment),
        ),
        (Subtype::Movie, Some(UnsupportedFeature::AnnotationMovie)),
        (
            Subtype::RichMedia,
            Some(UnsupportedFeature::AnnotationScreenRichMedia),
        ),
        (
            Subtype::Screen,
            Some(UnsupportedFeature::AnnotationScreenMedia),
        ),
        (Subtype::Sound, Some(UnsupportedFeature::AnnotationSound)),
        (Subtype::ThreeD, Some(UnsupportedFeature::Annotation3d)),
        (Subtype::Widget, None),
        (Subtype::Text, None),
    ] {
        events.clear();
        check_for_unsupported_annot(subtype, &dict, &mut |f| events.push(f));
        assert_eq!(events, expected.into_iter().collect::<Vec<_>>());
    }
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"IT", b"Img");
    events.clear();
    check_for_unsupported_annot(Subtype::Screen, &dict, &mut |f| events.push(f));
    assert!(events.is_empty());
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_name_for(b"FT", b"Sig");
    check_for_unsupported_annot(Subtype::Widget, &dict, &mut |f| events.push(f));
    assert_eq!(events, [UnsupportedFeature::AnnotationSignature]);
    assert_eq!(get_page_mode(None), PAGEMODE_UNKNOWN);
    assert_eq!(get_page_mode(Some(dict.clone())), PAGEMODE_USENONE);
    for (name, expected) in [
        (b"".as_slice(), 0),
        (b"UseNone", 0),
        (b"uSeOuTlInEs", 1),
        (b"UseThumbs", 2),
        (b"FullScreen", 3),
        (b"UseOC", 4),
        (b"UseAttachments", 5),
        (b"invalid", -1),
    ] {
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_name_for(b"PageMode", name);
        assert_eq!(get_page_mode(Some(dict.clone())), expected);
    }
}

#[test]
fn sdk_callback_registry_allows_replacement_during_notification() {
    use std::cell::RefCell;
    struct Restore(Option<UnsupportedHandler>);
    impl Drop for Restore {
        fn drop(&mut self) {
            set_unsupported_handler(self.0.take());
        }
    }
    let _restore = Restore(set_unsupported_handler(None));
    assert!(!raise_unsupported_error(
        UnsupportedFeature::DocumentSecurity
    ));
    let events = Rc::new(RefCell::new(Vec::new()));
    let observed = events.clone();
    set_unsupported_handler(Some(Rc::new(move |feature| {
        observed.borrow_mut().push(feature);
        let observed = observed.clone();
        set_unsupported_handler(Some(Rc::new(move |feature| {
            observed.borrow_mut().push(feature);
            set_unsupported_handler(None);
            true
        })));
        true
    })));
    let dict = retain_object(CpdfDictionary::new());
    notify_unsupported_annot(Subtype::Movie, &dict);
    notify_unsupported_annot(Subtype::ThreeD, &dict);
    assert!(!raise_unsupported_error(
        UnsupportedFeature::DocumentSecurity
    ));
    assert_eq!(
        *events.borrow(),
        [
            UnsupportedFeature::AnnotationMovie,
            UnsupportedFeature::Annotation3d
        ]
    );
}
