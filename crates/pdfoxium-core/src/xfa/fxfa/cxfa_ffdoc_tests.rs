// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Real FFDoc/App/model/runtime/embedding callbacks; no alternate document host.
use super::super::test_support::{new_document, HostCall};
use super::*;
use crate::{
    fpdfapi::parser::{
        cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object, cpdf_reference::CpdfReference,
        cpdf_stream::CpdfStream,
    },
    fxcrt::{
        cfx_memorystream::CfxMemoryStream,
        cfx_read_only_container_stream::CfxReadOnlyContainerStream,
    },
};
const XDP: &str = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record"><field name="amount"><ui><textEdit/></ui><value><text>original</text></value></field></subform></template><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><record><amount>bound</amount></record></xfa:data></xfa:datasets></xdp:xdp>"#;
fn wide(value: &str) -> Vec<u32> {
    value.chars().map(u32::from).collect()
}
#[test]
fn open_doc_uses_configuration_builds_actual_xfa_global_and_exports_package() {
    let (doc, _) = new_document(XDP);
    assert_eq!(doc.form_type(), FormType::kXFAForeground);
    assert!(doc
        .script_context()
        .eval("xfa.className==='xfa' && xfa.resolveNode('template')===xfa.template")
        .unwrap()
        .to_boolean());
    let mut output = CfxMemoryStream::new();
    assert!(doc.save_package(None, &mut output));
    let (reloaded, _) = new_document(output.get_span());
    assert_eq!(reloaded.form_type(), FormType::kXFAForeground);
    assert!(reloaded
        .model_document()
        .get_xfa_node(XfaHashCode::Template as u32)
        .is_some());
    let required = r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><config xmlns="http://www.xfa.org/schema/xci/3.3/"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat></config><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform/></template></xdp:xdp>"#;
    let (full, _) = new_document(required);
    assert_eq!(full.form_type(), FormType::kXFAFull);
    let (optional, _) = new_document(required.replace("required", "optional"));
    assert_eq!(optional.form_type(), FormType::kXFAForeground);
}
#[test]
fn actual_environment_and_app_callbacks_allow_same_engine_reentry() {
    let (doc, host) = new_document(XDP);
    let weak = Rc::downgrade(&doc);
    *host.on_callback.borrow_mut() = Some(Rc::new(move |call| {
        if matches!(call,HostCall::Title(value)if value==&wide("outer")) {
            weak.upgrade()
                .unwrap()
                .script_context()
                .eval("xfa.host.title='inner';globalThis.reentered=true")
                .unwrap();
        }
    }));
    assert!(doc.script_context().eval(r#"
    if(xfa.host.language!=='en_US'||xfa.host.platform!=='UNIX'||xfa.host.name!=='PDFOxium test'||xfa.host.numPages!==1)throw Error('actual app');
    xfa.host.title={toString(){return 'outer'}};if(xfa.host.title!=='inner'||!reentered)throw Error('environment reentry');
    xfa.host.currentPage=4;xfa.host.calculationsEnabled=false;xfa.host.validationsEnabled=false;
    xfa.host.beep(3);if(xfa.host.messageBox('message','title',2,1)!==1)throw Error('message callback');
    if(xfa.host.response('question','title','answer',true)!=='Test response')throw Error('response callback');
    xfa.host.gotoURL('https://example.invalid/');xfa.host.exportData('saved.xdp',true);
    true
    "#).unwrap().to_boolean());
    assert_eq!(host.title.borrow().as_slice(), wide("inner"));
    assert_eq!(host.current_page.get(), 4);
    assert!(!host.calculations.get());
    assert!(!host.validations.get());
    assert!(host.calls.borrow().contains(&HostCall::Beep(3)));
    assert!(host
        .calls
        .borrow()
        .contains(&HostCall::Goto(wide("https://example.invalid/"))));
    doc.print(2, 5, XfaPrintOpt::kShowDialog);
    assert!(host
        .calls
        .borrow()
        .contains(&HostCall::Print(2, 5, XfaPrintOpt::kShowDialog)));
    assert!(Rc::ptr_eq(&doc.get_xml_document().unwrap(), &host.xml));
    assert!(Rc::ptr_eq(
        &doc.get_ijs_runtime(),
        doc.script_context().javascript()
    ));
}
fn bmp(red: u8, green: u8, blue: u8) -> Vec<u8> {
    let mut bytes = vec![0; 58];
    bytes[..2].copy_from_slice(b"BM");
    bytes[2..6].copy_from_slice(&58u32.to_le_bytes());
    bytes[10..14].copy_from_slice(&54u32.to_le_bytes());
    bytes[14..18].copy_from_slice(&40u32.to_le_bytes());
    bytes[18..22].copy_from_slice(&1u32.to_le_bytes());
    bytes[22..26].copy_from_slice(&1u32.to_le_bytes());
    bytes[26..28].copy_from_slice(&1u16.to_le_bytes());
    bytes[28..30].copy_from_slice(&24u16.to_le_bytes());
    bytes[38..42].copy_from_slice(&2835u32.to_le_bytes());
    bytes[42..46].copy_from_slice(&5669u32.to_le_bytes());
    bytes[54..58].copy_from_slice(&[blue, green, red, 0]);
    bytes
}
#[test]
fn named_pdf_images_decode_cache_identity_dpi_and_distinct_colliding_names() {
    let (doc, host) = new_document(XDP);
    let tree = {
        let pdf = host.pdf.borrow();
        CpdfNameTree::create_with_root_name_array(&pdf, b"XFAImages").unwrap()
    };
    let first = vec![65, 1378];
    let second = vec![66, 65];
    assert_eq!(first[0] * 1313 + first[1], second[0] * 1313 + second[1]);
    let holder = host.pdf.borrow().holder();
    for (name, bytes) in [(&first, bmp(200, 10, 20)), (&second, bmp(30, 180, 40))] {
        let stream = retain_object(CpdfStream::from_data_and_dict(
            bytes,
            retain_object(CpdfDictionary::new()),
        ));
        let object_number = holder.borrow_mut().add_indirect_object(stream.clone());
        assert!(tree.add_value_and_name(
            retain_object(CpdfReference::new(&holder, object_number)),
            name
        ));
        assert!(Rc::ptr_eq(&stream, &tree.lookup_value(name).unwrap()));
    }
    let a = doc.get_pdf_named_image(&first).unwrap();
    let b = doc.get_pdf_named_image(&second).unwrap();
    assert!(a.named && b.named);
    assert_eq!(a.dpi, (72, 143));
    assert!(!Rc::ptr_eq(&a.bitmap, &b.bitmap));
    assert_eq!(&a.bitmap.scanline(0).unwrap()[..3], &[20, 10, 200]);
    assert_eq!(&b.bitmap.scanline(0).unwrap()[..3], &[40, 180, 30]);
    assert!(Rc::ptr_eq(
        &a.bitmap,
        &doc.get_pdf_named_image(&first).unwrap().bitmap
    ));
    assert!(doc.get_pdf_named_image(&wide("missing")).is_none());
    let file: Rc<dyn SeekableReadStream> = Rc::new(CfxReadOnlyContainerStream::new(
        b"actual external bytes".to_vec(),
    ));
    host.files.borrow_mut().push((wide("linked"), file.clone()));
    assert!(Rc::ptr_eq(
        &file,
        &doc.open_linked_file(&wide("linked")).unwrap()
    ));
}
#[test]
fn weak_runtime_hosts_and_explicit_close_do_not_retain_document_cycles() {
    let (doc, _host) = new_document(XDP);
    let theme = doc.app().theme();
    assert!(doc.app().load_fwl_theme(&doc));
    assert!(!Rc::ptr_eq(&theme, &doc.app().theme()));
    let view = doc.create_doc_view();
    assert!(Rc::ptr_eq(&view, &doc.create_doc_view()));
    assert!(doc
        .get_doc_view_for_layout(&doc.layout_processor())
        .is_some());
    let engine = doc.script_context();
    let weak = Rc::downgrade(&doc);
    let app = Rc::downgrade(&doc.app());
    doc.pre_finalize();
    doc.pre_finalize();
    assert!(doc.try_script_context().is_none());
    assert!(engine.host().is_none());
    assert!(engine.network_host().is_none());
    drop(doc);
    assert!(weak.upgrade().is_none());
    assert!(app.upgrade().is_none());
    // A surviving view retains the actual FWL capabilities, not the FF owner.
    use super::super::cxfa_ffwidget::FfDocViewHost;
    let _fwl = view.fwl_app();
}
