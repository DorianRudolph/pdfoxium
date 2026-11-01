// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Foreground AcroForm/XFA synchronization through actual SDK, FF and Boa owners.
use super::*;
use crate::{
    fpdfapi::{
        page::cpdf_page::CpdfPage,
        parser::{
            cpdf_array::CpdfArray,
            cpdf_dictionary::CpdfDictionary,
            cpdf_document::CpdfDocument,
            cpdf_object::{retain_object, ObjectRef},
            cpdf_reference::CpdfReference,
            cpdf_scalar_object::CpdfScalarObject,
            cpdf_stream::CpdfStream,
        },
    },
    fpdfsdk::{
        cpdfsdk_formfillenvironment::{CpdfSdkFormFillEnvironment, FormFillHost},
        cpdfsdk_pageview::SdkPage,
        fpdfxfa::cpdfxfa_context::CpdfXfaContext,
    },
    fxcrt::fx_coordinates::CfxFloatRect,
};
use std::cell::RefCell;
fn set(dict: &ObjectRef, key: &[u8], value: ObjectRef) {
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(key, Some(value));
}
fn array(numbers: &[i32]) -> ObjectRef {
    let mut array = CpdfArray::new();
    for &n in numbers {
        array.append_new(CpdfScalarObject::number_from_signed(n));
    }
    retain_object(array)
}
struct Host;
impl FormFillHost for Host {}
struct Fixture {
    doc: Rc<RefCell<CpdfDocument>>,
    context: Rc<CpdfXfaContext>,
    env: Rc<CpdfSdkFormFillEnvironment>,
    _host: Rc<dyn FormFillHost>,
    _page: Rc<SdkPage>,
    view: Rc<CpdfSdkPageView>,
    widget: Rc<CpdfSdkWidget>,
}
impl Fixture {
    fn new() -> Self {
        let mut doc = CpdfDocument::new();
        doc.create_new_doc();
        let page = doc.create_new_page(0).unwrap();
        set(&page, b"MediaBox", array(&[0, 0, 200, 300]));
        let mut field = CpdfDictionary::new();
        field.set_new_name_for(b"Type", b"Annot");
        field.set_new_name_for(b"Subtype", b"Widget");
        field.set_new_name_for(b"FT", b"Tx");
        field.set_new_string_for(b"T", b"root.amount");
        field.set_new_string_for(b"V", b"pdf");
        field.set_new_string_for(b"DA", b"/Helvetica 12 Tf 0 g");
        field.set_new_for(b"MaxLen", CpdfScalarObject::number_from_signed(3));
        field.set_for(b"Rect", Some(array(&[0, 0, 100, 20])));
        let field = retain_object(field);
        let mut fields = CpdfArray::new();
        fields.append(field.clone());
        let mut annots = CpdfArray::new();
        annots.append(field);
        set(&page, b"Annots", retain_object(annots));
        let mut packets = CpdfArray::new();
        let holder = doc.holder();
        for(name,data)in[(b"preamble".as_slice(),br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">"#.as_slice()),(b"template".as_slice(),br#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root" layout="tb"><pageSet><pageArea name="page"><contentArea w="200pt" h="300pt"/></pageArea></pageSet><field name="amount" w="100pt" h="20pt"><ui><textEdit/></ui><bind match="dataRef" ref="$record.amount"/><event activity="click"><script contentType="application/x-javascript">globalThis.clicks=(globalThis.clicks||0)+1;</script></event><event activity="full"><script contentType="application/x-javascript">globalThis.fulls=(globalThis.fulls||0)+1;</script></event></field></subform></template>"#.as_slice()),(b"datasets".as_slice(),br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><amount>old</amount></root></xfa:data></xfa:datasets>"#.as_slice()),(b"postamble".as_slice(),b"</xdp:xdp>".as_slice())]{packets.append_new_string(name);let number=holder.borrow_mut().add_indirect_object(retain_object(CpdfStream::from_bytes(data)));packets.append_new(CpdfReference::new(&holder,number));}
        let mut acro = CpdfDictionary::new();
        acro.set_for(b"Fields", Some(retain_object(fields)));
        acro.set_for(b"XFA", Some(retain_object(packets)));
        set(&doc.root().unwrap(), b"AcroForm", retain_object(acro));
        let doc = Rc::new(RefCell::new(doc));
        let host: Rc<dyn FormFillHost> = Rc::new(Host);
        let env = CpdfSdkFormFillEnvironment::new(&doc, Some(&host));
        let context = CpdfXfaContext::new(&doc);
        context.set_form_fill_env(Some(&env));
        let runtime = env.get_or_create_runtime().unwrap();
        assert_eq!(
            runtime
                .engine()
                .eval(&"app.viewerVersion".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .as_number(),
            Some(8.)
        );
        context.load_xfa_doc().unwrap();
        assert!(context.contains_extension_foreground_form());
        assert_eq!(
            runtime
                .engine()
                .eval(&"app.viewerVersion".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .as_number(),
            Some(11.)
        );
        let page = CpdfPage::from_document(&mut doc.borrow_mut(), 0).unwrap();
        let page = SdkPage::new(&doc, &page);
        let view = env.get_or_create_page_view(&page).unwrap();
        view.set_valid(true);
        let widget = view
            .annot_list()
            .into_iter()
            .find_map(|annot| annot.widget())
            .unwrap();
        Self {
            doc,
            context,
            env,
            _host: host,
            _page: page,
            view,
            widget,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.context.close();
    }
}
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
#[test]
fn foreground_widget_reads_xfa_display_and_syncs_pdf_and_javascript_writes() {
    let fixture = Fixture::new();
    let script = fixture.context.xfa_doc().unwrap().script_context();
    assert_eq!(fixture.widget.field().unwrap().value(), wide("pdf"));
    assert_eq!(fixture.widget.value(), wide("old"));
    assert!(fixture.widget.get_mix_xfa_widget().is_some());
    assert!(fixture.widget.set_value(&wide("new")));
    assert_eq!(
        script
            .eval("xfa.form.root.amount.rawValue")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "new"
    );
    let sdk = fixture.env.get_or_create_runtime().unwrap();
    sdk.engine()
        .eval(
            &"this.getField('root.amount').value='js'"
                .encode_utf16()
                .collect::<Vec<_>>(),
        )
        .unwrap();
    assert_eq!(
        script
            .eval("xfa.form.root.amount.rawValue")
            .unwrap()
            .as_string()
            .unwrap()
            .to_std_string_escaped(),
        "js"
    );
    assert_eq!(fixture.widget.value(), wide("js"));
    assert!(fixture.widget.has_xfa_action(XfaAActionType::Click));
    assert!(fixture.widget.on_xfa_action(
        XfaAActionType::Click,
        &mut CfflFieldAction::default(),
        &fixture.view
    ));
    assert_eq!(script.eval("clicks").unwrap().as_number(), Some(1.));
    // The same AcroForm dictionary remains attached; the adapter never rewrites
    // font mappings or replaces the PDF widget with a synthetic XFA annotation.
    assert_eq!(
        fixture.widget.annot().rect(),
        CfxFloatRect::new(0., 0., 100., 20.)
    );
    assert_eq!(fixture.doc.borrow().page_count(), 1);
}
#[test]
fn foreground_full_event_uses_the_actual_pwl_text_limit_and_xfa_handler() {
    let fixture = Fixture::new();
    assert!(fixture.widget.set_value(&wide("old")));
    let annot = fixture.widget.annotation().unwrap();
    assert!(fixture.env.set_focus_annot(&annot));
    assert!(fixture.widget.has_xfa_action(XfaAActionType::Full));
    annot.on_char(u32::from(b'x'), 0);
    let script = fixture.context.xfa_doc().unwrap().script_context();
    assert_eq!(script.eval("fulls").unwrap().as_number(), Some(1.));
    assert_eq!(fixture.widget.get_text(), wide("old"));
}
