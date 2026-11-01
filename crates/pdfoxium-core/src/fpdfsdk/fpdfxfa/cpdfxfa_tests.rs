// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source-derived SDK XFA ownership, packet-stream and page regressions.
use super::*;
use crate::{
    fpdfapi::parser::{
        cpdf_object::retain_object,
        cpdf_reference::CpdfReference,
        cpdf_scalar_object::{CpdfScalarObject, ObjectType},
        cpdf_stream::CpdfStream,
    },
    fpdfsdk::cpdfsdk_formfillenvironment::FormFillHost,
    fxcrt::{
        fx_coordinates::{CfxPointF, FxRect},
        fx_stream::StreamWithSize,
    },
};
fn pdf(xfa: Option<ObjectRef>) -> Rc<RefCell<CpdfDocument>> {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    for index in 0..2 {
        let page = doc.create_new_page(index).unwrap();
        let mut array = CpdfArray::new();
        for value in [0, 0, 200, 300] {
            array.append_new(CpdfScalarObject::number_from_signed(value));
        }
        page.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"MediaBox", Some(retain_object(array)));
    }
    if let Some(xfa) = xfa {
        let xfa = if xfa.borrow().object_type() == ObjectType::Stream {
            let holder = doc.holder();
            let number = holder.borrow_mut().add_indirect_object(xfa.clone());
            retain_object(CpdfReference::new(&holder, number))
        } else {
            xfa
        };
        let mut acro = CpdfDictionary::new();
        acro.set_for(b"XFA", Some(xfa));
        doc.root()
            .unwrap()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(b"AcroForm", Some(retain_object(acro)));
    }
    Rc::new(RefCell::new(doc))
}
#[test]
fn xfa_packet_stream_concatenates_only_stream_entries_and_retains_owners() {
    let doc = pdf(None);
    let holder = doc.borrow().holder();
    let first = retain_object(CpdfStream::from_bytes(b"<xdp>"));
    let weak = Rc::downgrade(&first);
    let mut array = CpdfArray::new();
    array.append_new_string(b"xdp");
    let number = holder.borrow_mut().add_indirect_object(first);
    array.append_new(CpdfReference::new(&holder, number));
    array.append_new_string(b"bad");
    array.append_new_string(b"not a stream");
    array.append_new_string(b"tail");
    let number = holder
        .borrow_mut()
        .add_indirect_object(retain_object(CpdfStream::from_bytes(b"</xdp>")));
    array.append_new(CpdfReference::new(&holder, number));
    array.append_new_string(b"unpaired");
    let mut acro = CpdfDictionary::new();
    acro.set_for(b"XFA", Some(retain_object(array)));
    doc.borrow()
        .root()
        .unwrap()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"AcroForm", Some(retain_object(acro)));
    drop(holder);
    let stream = create_xfa_multistream(&doc.borrow()).unwrap();
    assert_eq!(stream.get_size(), 11);
    drop(doc);
    assert!(weak.upgrade().is_some());
    let mut bytes = [0; 11];
    assert!(stream.read_block_at_offset(&mut bytes, 0));
    assert_eq!(&bytes, b"<xdp></xdp>");
    drop(stream);
    assert!(weak.upgrade().is_none());
    let doc = pdf(Some(retain_object(CpdfStream::from_bytes(b"<single/>"))));
    let stream = create_xfa_multistream(&doc.borrow()).unwrap();
    assert_eq!(stream.get_size(), 9);
    assert!(create_xfa_multistream(&pdf(None).borrow()).is_none());
}
#[test]
fn foreground_page_cache_preserves_identity_geometry_and_indices() {
    let doc = pdf(None);
    let context = CpdfXfaContext::new(&doc);
    assert_eq!(context.get_page_count(), 2);
    let first = context.get_or_create_xfa_page(0).unwrap();
    let second = context.get_or_create_xfa_page(1).unwrap();
    assert!(Rc::ptr_eq(
        &first,
        &context.get_or_create_xfa_page(0).unwrap()
    ));
    assert!(context.get_or_create_xfa_page(-1).is_none());
    assert!(context.get_or_create_xfa_page(i32::MAX).is_none());
    assert_eq!((first.width(), first.height()), (200., 300.));
    let display = FxRect::new(10, 20, 410, 620);
    let point = CfxPointF::new(15., 25.);
    for rotation in 0..4 {
        let device = first.page_to_device(display, rotation, point).unwrap();
        let restored = first.device_to_page(display, rotation, device).unwrap();
        assert!((restored.x - point.x).abs() < 0.001);
        assert!((restored.y - point.y).abs() < 0.001);
    }
    let sdk = context.sdk_page(&first).unwrap();
    assert!(Rc::ptr_eq(&sdk, &context.sdk_page(&first).unwrap()));
    assert!(sdk.pdf_dict().is_some());
    context.pages_inserted(0, 1);
    assert_eq!(first.page_index(), 1);
    assert_eq!(second.page_index(), 2);
    context.pages_inserted(3, 1);
    assert_eq!(context.original_page_count(), 4);
    context.close();
    assert!(!sdk.is_alive());
    drop(context);
    assert!(first.context().is_none());
}
struct Host {
    javascript: bool,
}
impl FormFillHost for Host {
    fn javascript_enabled(&self) -> bool {
        self.javascript
    }
}
#[test]
fn failed_load_keeps_sdk_runtime_and_releases_xfa_roots() {
    let doc = pdf(Some(retain_object(CpdfStream::from_bytes(b"<not-xfa/>"))));
    let host: Rc<dyn FormFillHost> = Rc::new(Host { javascript: true });
    let env = CpdfSdkFormFillEnvironment::new(&doc, Some(&host));
    let runtime = env.get_or_create_runtime().unwrap();
    let context = CpdfXfaContext::new(&doc);
    context.set_form_fill_env(Some(&env));
    assert_eq!(context.load_xfa_doc(), Err(XfaLoadError::OpenDocument));
    assert_eq!(context.load_status(), LoadStatus::Preload);
    assert!(context.xfa_doc().is_none());
    assert!(context.doc_view().is_none());
    assert!(context.xml_document().is_none());
    assert!(env.is_alive());
    assert!(Rc::ptr_eq(&runtime, &env.get_or_create_runtime().unwrap()));
    context.close();
    assert!(!env.is_alive());
    assert_eq!(context.load_xfa_doc(), Err(XfaLoadError::Detached));
}
#[test]
fn javascript_disabled_does_not_create_an_independent_xfa_runtime() {
    let doc = pdf(Some(retain_object(CpdfStream::from_bytes(b"<xdp/>"))));
    let host: Rc<dyn FormFillHost> = Rc::new(Host { javascript: false });
    let env = CpdfSdkFormFillEnvironment::new(&doc, Some(&host));
    let context = CpdfXfaContext::new(&doc);
    context.set_form_fill_env(Some(&env));
    assert_eq!(
        context.load_xfa_doc(),
        Err(XfaLoadError::JavaScriptUnavailable)
    );
    assert!(context.xfa_doc().is_none());
}

#[test]
fn unloaded_extension_save_events_preserve_ordinary_pdf_change_mark() {
    let doc = pdf(None);
    let host: Rc<dyn FormFillHost> = Rc::new(Host { javascript: true });
    let env = CpdfSdkFormFillEnvironment::new(&doc, Some(&host));
    let context = CpdfXfaContext::new(&doc);
    context.set_form_fill_env(Some(&env));
    env.set_change_mark();
    context.send_pre_save_to_xfa_doc();
    context.send_post_save_to_xfa_doc();
    assert!(env.change_mark());
    assert!(context.xfa_doc().is_none());
    context.close();
}
