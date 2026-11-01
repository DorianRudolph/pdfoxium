// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Native packet replacement/ordering and a real SDK XFA edit/save/reload.
use super::*;
use crate::{
    fpdfapi::{
        edit::cpdf_creator::CpdfCreator,
        parser::{
            cpdf_document::CpdfDocument, cpdf_parser::ParserError,
            cpdf_scalar_object::CpdfScalarObject,
        },
    },
    fpdfsdk::cpdfsdk_formfillenvironment::{CpdfSdkFormFillEnvironment, FormFillHost},
    fxcrt::fx_stream::WriteStream,
};
use std::cell::RefCell;
fn memory(bytes: &[u8]) -> CfxMemoryStream {
    let mut stream = CfxMemoryStream::new();
    assert!(stream.write_block(bytes));
    stream
}
// Valid PDF arrays store streams through indirect references. The native
// direct-stream compatibility branch is unreachable through this safe API.
fn append_stream(array: &mut CpdfArray, holder: &HolderRef, data: &[u8]) -> ObjectRef {
    let stream = retain_object(CpdfStream::from_bytes(data));
    let number = holder.borrow_mut().add_indirect_object(stream.clone());
    array.append_new(CpdfReference::new(holder, number));
    stream
}
fn bytes(array: &ObjectRef, name: &[u8]) -> Vec<u8> {
    let object = array.borrow();
    let array = object.as_any().downcast_ref::<CpdfArray>().unwrap();
    let index = (0..array.size().saturating_sub(1))
        .find(|&index| array.get_byte_string_at(index) == name)
        .unwrap();
    let stream = array.get_stream_at(index + 1).unwrap();
    let object = stream.borrow();
    object
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .read_all_raw_data()
}
#[test]
fn inserts_before_postamble_and_preserves_existing_indirect_form_identity() {
    let document = CpdfDocument::new();
    let holder = document.holder();
    let form = retain_object(CpdfStream::from_bytes(b"old-form"));
    let number = holder.borrow_mut().add_indirect_object(form.clone());
    let mut packets = CpdfArray::new();
    packets.append_new_string(b"template");
    append_stream(&mut packets, &holder, b"template");
    packets.append_new_string(b"form");
    packets.append_new(CpdfReference::new(&holder, number));
    packets.append_new_string(b"postamble");
    append_stream(&mut packets, &holder, b"end");
    let packets = retain_object(packets);
    replace_package(&packets, &holder, b"datasets", memory(b"new-data"));
    replace_package(&packets, &holder, b"form", memory(b"new-form"));
    assert_eq!(bytes(&packets, b"datasets"), b"new-data");
    assert_eq!(bytes(&packets, b"form"), b"new-form");
    assert_eq!(form.borrow().obj_num(), number);
    let object = packets.borrow();
    let array = object.as_any().downcast_ref::<CpdfArray>().unwrap();
    assert_eq!(array.size(), 8);
    assert_eq!(array.get_byte_string_at(6), b"postamble");
    assert!(Rc::ptr_eq(&array.get_stream_at(3).unwrap(), &form));
}
#[test]
fn resolved_stream_identity_is_preserved_and_malformed_existing_packet_is_unchanged() {
    let document = CpdfDocument::new();
    let holder = document.holder();
    let malformed = retain_object(CpdfScalarObject::number_from_signed(17));
    let mut array = CpdfArray::new();
    array.append_new_string(b"datasets");
    let stream = append_stream(&mut array, &holder, b"old");
    array.append_new_string(b"form");
    array.append(malformed.clone());
    let array = retain_object(array);
    replace_package(&array, &document.holder(), b"datasets", memory(b"new"));
    replace_package(&array, &document.holder(), b"form", memory(b"unused"));
    assert_eq!(bytes(&array, b"datasets"), b"new");
    let object = array.borrow();
    let array = object.as_any().downcast_ref::<CpdfArray>().unwrap();
    assert!(Rc::ptr_eq(&array.get_stream_at(1).unwrap(), &stream));
    assert!(Rc::ptr_eq(&array.get_object_at(3).unwrap(), &malformed));
    assert_eq!(array.size(), 4);
    assert!(!save_xfa_document_data(None));
}
struct Host;
impl FormFillHost for Host {
    fn javascript_enabled(&self) -> bool {
        true
    }
}
fn load(
    document: &Rc<RefCell<CpdfDocument>>,
) -> (
    Rc<CpdfXfaContext>,
    Rc<CpdfSdkFormFillEnvironment>,
    Rc<dyn FormFillHost>,
) {
    let host: Rc<dyn FormFillHost> = Rc::new(Host);
    let env = CpdfSdkFormFillEnvironment::new(document, Some(&host));
    let context = CpdfXfaContext::new(document);
    context.set_form_fill_env(Some(&env));
    context.load_xfa_doc().unwrap();
    (context, env, host)
}
#[test]
fn actual_xfa_edit_and_presave_effect_survive_pdf_save_and_sdk_reload() {
    let mut pdf = CpdfDocument::new();
    pdf.create_new_doc();
    pdf.create_new_page(0).unwrap();
    let holder = pdf.holder();
    let mut packets = CpdfArray::new();
    for (name, data) in [
        (b"preamble".as_slice(), br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">"#.as_slice()),
        (b"template".as_slice(), br#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root" layout="tb"><pageSet><pageArea name="page"><contentArea w="200pt" h="200pt"/></pageArea></pageSet><field name="amount" w="100pt" h="20pt"><ui><textEdit/></ui><bind match="dataRef" ref="$record.amount"/><event activity="preSave"><script contentType="application/x-javascript">this.rawValue=this.rawValue+'-pre';</script></event><event activity="postSave"><script contentType="application/x-javascript">globalThis.postSaved=true;</script></event></field></subform></template>"#.as_slice()),
        (b"datasets".as_slice(), br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><amount>old</amount></root></xfa:data></xfa:datasets>"#.as_slice()),
        (b"postamble".as_slice(), b"</xdp:xdp>".as_slice()),
    ] {
        packets.append_new_string(name);
        append_stream(&mut packets,&holder,data);
    }
    let packets = retain_object(packets);
    let mut acro = CpdfDictionary::new();
    acro.set_for(b"XFA", Some(packets.clone()));
    pdf.root()
        .unwrap()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_for(b"AcroForm", Some(retain_object(acro)));
    let pdf = Rc::new(RefCell::new(pdf));
    let (context, _env, _host) = load(&pdf);
    let engine = context.xfa_doc().unwrap().script_context();
    engine
        .eval("xfa.form.root.amount.rawValue='changed'")
        .unwrap();
    context.send_pre_save_to_xfa_doc();
    assert!(save_xfa_document_data(Some(&context)));
    assert!(String::from_utf8(bytes(&packets, b"datasets"))
        .unwrap()
        .contains("changed-pre"));
    assert!(!bytes(&packets, b"form").is_empty());
    let mut saved = CfxMemoryStream::new();
    assert!(CpdfCreator::new(pdf.clone(), &mut saved).create(0, 0));
    context.send_post_save_to_xfa_doc();
    assert_eq!(engine.eval("postSaved").unwrap().as_boolean(), Some(true));
    let mut reloaded = CpdfDocument::new();
    assert_eq!(reloaded.load_doc(Rc::new(saved), b""), ParserError::Success);
    let reloaded = Rc::new(RefCell::new(reloaded));
    let (context, _reload_env, _reload_host) = load(&reloaded);
    let value = context
        .xfa_doc()
        .unwrap()
        .script_context()
        .eval("xfa.form.root.amount.rawValue")
        .unwrap();
    assert_eq!(
        value.as_string().unwrap().to_std_string_escaped(),
        "changed-pre"
    );
    assert!(context.get_page_count() > 0);
}
