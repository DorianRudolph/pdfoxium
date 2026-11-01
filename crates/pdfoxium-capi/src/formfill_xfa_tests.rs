// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Public SDK full-XFA load, real shared Boa VM, FF rendering and edit/reload.
use super::*;
use pdfoxium_core::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_object::retain_object, cpdf_reference::CpdfReference,
        cpdf_stream::CpdfStream,
    },
    xfa::fxfa::fxfa_basic::XfaHashCode,
};
const TEMPLATE:&[u8]=br#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root" layout="tb"><pageSet><pageArea name="page"><occur min="1" max="-1"/><medium short="200pt" long="300pt"/><contentArea w="200pt" h="300pt"/></pageArea></pageSet><draw w="40pt" h="20pt"><value><rectangle><fill><color value="255,0,0"/></fill><edge presence="hidden"/></rectangle></value></draw><field name="amount" w="100pt" h="20pt"><ui><textEdit/></ui><bind match="dataRef" ref="$record.amount"/></field></subform></template>"#;
struct Fixture {
    document: *mut c_void,
    form: *mut c_void,
    info: Box<FPDF_FORMFILLINFO>,
    _js: Box<IPDF_JSPLATFORM>,
}
impl Fixture {
    unsafe fn new(disabled: bool, template: &[u8]) -> Self {
        FPDF_InitLibrary();
        let document = FPDF_CreateNewDocument();
        let pdf = unsafe { &*document.cast::<DocumentHandle>() }
            .document
            .clone();
        pdf.borrow_mut().create_new_page(0).unwrap();
        pdf.borrow_mut().create_new_page(1).unwrap();
        let mut packets = CpdfArray::new();
        let holder = pdf.borrow().holder();
        for(name,bytes)in[
   (b"preamble".as_slice(),br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">"#.as_slice()),
   (b"config".as_slice(),br#"<config xmlns="http://www.xfa.org/schema/xci/3.0/"><acrobat><acrobat7><dynamicRender>required</dynamicRender></acrobat7></acrobat><present><pdf><interactive>1</interactive></pdf></present></config>"#.as_slice()),
   (b"template".as_slice(),template),
   (b"datasets".as_slice(),br#"<xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data><root><amount>old</amount></root></xfa:data></xfa:datasets>"#.as_slice()),
   (b"postamble".as_slice(),b"</xdp:xdp>".as_slice()),
  ]{packets.append_new_string(name);let number = holder.borrow_mut().add_indirect_object(retain_object(CpdfStream::from_bytes(bytes)));packets.append_new(CpdfReference::new(&holder,number));}
        let mut acro = CpdfDictionary::new();
        acro.set_for(b"XFA", Some(retain_object(packets)));
        let root = pdf.borrow().root().unwrap();
        let mut object = root.borrow_mut();
        let root = object
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        root.set_for(b"AcroForm", Some(retain_object(acro)));
        root.set_new_for(
            b"NeedsRendering",
            pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject::boolean(true),
        );
        drop(object);
        let mut js: Box<IPDF_JSPLATFORM> = Box::new(unsafe { std::mem::zeroed() });
        js.version = 3;
        let mut info: Box<FPDF_FORMFILLINFO> = Box::new(unsafe { std::mem::zeroed() });
        info.version = 2;
        info.xfa_disabled = disabled as c_int;
        info.m_pJsPlatform = &mut *js;
        let form = unsafe { FPDFDOC_InitFormFillEnvironment(document, &mut *info) };
        assert!(!form.is_null());
        Self {
            document,
            form,
            info,
            _js: js,
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        unsafe {
            FPDFDOC_ExitFormFillEnvironment(self.form);
            FPDF_CloseDocument(self.document);
        }
    }
}
#[repr(C)]
struct Saved {
    writer: FPDF_FILEWRITE,
    data: Vec<u8>,
}
unsafe extern "C" fn save(
    writer: *mut FPDF_FILEWRITE,
    data: *const c_void,
    size: c_ulong,
) -> c_int {
    let writer = unsafe { &mut *writer.cast::<Saved>() };
    writer
        .data
        .extend_from_slice(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) });
    1
}
fn as_wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}
#[test]
fn full_xfa_uses_existing_vm_and_real_ff_page_input_render_save_reload() {
    unsafe {
        let mut fixture = Fixture::new(false, TEMPLATE);
        let env = form_environment(fixture.form).unwrap();
        let sdk = env.get_or_create_runtime().unwrap();
        sdk.engine()
            .eval(&as_wide("globalThis.sdkOnly=73")[..as_wide("globalThis.sdkOnly=73").len() - 1])
            .unwrap();
        assert_eq!(
            FPDFDOC_InitFormFillEnvironment(fixture.document, &mut *fixture.info),
            fixture.form
        );
        assert_eq!(FPDF_GetFormType(fixture.document), 2);
        assert_eq!(
            FPDF_LoadXFA(fixture.document),
            1,
            "load error {}",
            FPDF_GetLastError()
        );
        let context = (&*fixture.document.cast::<DocumentHandle>())
            .xfa_context
            .clone()
            .unwrap();
        let ff = context.xfa_doc().unwrap();
        let script = ff.script_context();
        assert!(Rc::ptr_eq(script.javascript(), sdk.engine()));
        // Native named XFA lookup falls back to the actual SDK global. The
        // realms retain their own properties while sharing this host lookup.
        assert!(script
            .eval("sdkOnly === 73 && !Object.hasOwn(globalThis, 'sdkOnly')")
            .unwrap()
            .to_boolean());
        assert_eq!(
            sdk.engine()
                .eval(&"sdkOnly".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .as_number(),
            Some(73.)
        );
        assert!(script.eval("throw new Error('xfa only')").is_err());
        assert_eq!(
            sdk.engine()
                .eval(&"sdkOnly".encode_utf16().collect::<Vec<_>>())
                .unwrap()
                .as_number(),
            Some(73.)
        );
        let timezone = script
            .eval("new Date(0).getTimezoneOffset()")
            .unwrap()
            .as_number();
        assert_eq!(
            timezone,
            sdk.engine()
                .eval(
                    &"new Date(0).getTimezoneOffset()"
                        .encode_utf16()
                        .collect::<Vec<_>>()
                )
                .unwrap()
                .as_number()
        );
        assert_eq!(FPDF_GetPageCount(fixture.document), 1);
        let page = FPDF_LoadPage(fixture.document, 0);
        assert!(!page.is_null());
        assert!((&*page.cast::<PageHandle>()).pdf_page().is_none());
        assert_eq!(
            (FPDF_GetPageWidthF(page), FPDF_GetPageHeightF(page)),
            (200., 300.)
        );
        let mut size = FS_SIZEF {
            width: 0.,
            height: 0.,
        };
        assert_eq!(FPDF_GetPageSizeByIndexF(fixture.document, 0, &mut size), 1);
        assert_eq!((size.width, size.height), (200., 300.));
        let (mut device_x, mut device_y) = (0, 0);
        assert_eq!(
            FPDF_PageToDevice(
                page,
                0,
                0,
                400,
                600,
                0,
                15.,
                25.,
                &mut device_x,
                &mut device_y
            ),
            1
        );
        let (mut x, mut y) = (0., 0.);
        assert_eq!(
            FPDF_DeviceToPage(page, 0, 0, 400, 600, 0, device_x, device_y, &mut x, &mut y),
            1
        );
        assert!((x - 15.).abs() < 0.01 && (y - 25.).abs() < 0.01);
        FORM_OnAfterLoadPage(page, fixture.form);
        // PDF additional-action APIs must safely ignore a layout-only page.
        FORM_DoPageAAction(page, fixture.form, 0);
        FORM_DoPageAAction(page, fixture.form, 1);
        let field = ff
            .model_document()
            .get_xfa_node(XfaHashCode::Form as u32)
            .unwrap()
            .get_first_child_by_name(&"root".chars().map(u32::from).collect::<Vec<_>>())
            .unwrap()
            .get_first_child_by_name(&"amount".chars().map(u32::from).collect::<Vec<_>>())
            .unwrap();
        let owner = context
            .doc_view()
            .unwrap()
            .get_widget_for_node(&field)
            .unwrap();
        let rect = owner.as_ff_widget().unwrap().widget().get_widget_rect();
        assert!(rect.width > 0. && rect.height > 0.);
        let point = CfxPointF::new(rect.left + 5., rect.top + 5.);
        assert_eq!(
            FPDFPage_HasFormFieldAtPoint(fixture.form, page, point.x.into(), point.y.into()),
            FormFieldType::XfaTextField as c_int
        );
        assert_eq!(
            FORM_OnFocus(fixture.form, page, 0, point.x.into(), point.y.into()),
            1
        );
        let (mut focused_index, mut focused) = (-1, std::ptr::null_mut());
        assert_eq!(
            FORM_GetFocusedAnnot(fixture.form, &mut focused_index, &mut focused),
            1
        );
        assert_eq!(focused_index, 0);
        assert!(!focused.is_null());
        assert_eq!(
            FPDFAnnot_GetSubtype(focused),
            pdfoxium_core::fpdfdoc::cpdf_annot::Subtype::XfaWidget as c_int
        );
        let mut focused_rect: FS_RECTF = std::mem::zeroed();
        assert_eq!(FPDFAnnot_GetRect(focused, &mut focused_rect), 1);
        assert_eq!(FPDFAnnot_GetObjectCount(focused), 0);
        assert_eq!(
            FPDFAnnot_GetFormFieldType(fixture.form, focused),
            FormFieldType::XfaTextField as c_int
        );
        assert_eq!(FORM_SetFocusedAnnot(fixture.form, focused), 1);
        FPDFPage_CloseAnnot(focused);
        assert_eq!(FORM_SelectAllText(fixture.form, page), 1);
        FORM_ReplaceSelection(fixture.form, page, as_wide("changed").as_ptr());
        assert_eq!(FORM_ForceToKillFocus(fixture.form), 1);
        assert_eq!(
            script
                .eval("xfa.form.root.amount.rawValue")
                .unwrap()
                .as_string()
                .unwrap()
                .to_std_string_escaped(),
            "changed"
        );
        let bitmap = FPDFBitmap_Create(200, 300, 1);
        FPDFBitmap_FillRect(bitmap, 0, 0, 200, 300, 0xffffffff);
        FPDF_FFLDraw(fixture.form, bitmap, page, 0, 0, 200, 300, 0, 0);
        let bytes = std::slice::from_raw_parts(
            FPDFBitmap_GetBuffer(bitmap).cast::<u8>(),
            FPDFBitmap_GetStride(bitmap) as usize * 300,
        );
        assert!(bytes
            .chunks_exact(4)
            .any(|p| p[0] < 240 || p[1] < 240 || p[2] < 240));
        FPDFBitmap_Destroy(bitmap);
        // PDF-only entry points reject a full-XFA page instead of fabricating a
        // dictionary, page-object graph or PDF text extraction result.
        let mut bbox: FS_RECTF = std::mem::zeroed();
        assert_eq!(FPDF_GetPageBoundingBox(page, &mut bbox), 0);
        assert_eq!(FPDFPage_GetRotation(page), -1);
        assert_eq!(FPDFPage_CountObjects(page), -1);
        assert_eq!(FPDFPage_GetAnnotCount(page), 0);
        assert!(FPDFPage_CreateAnnot(page, 1).is_null());
        assert!(FPDFText_LoadPage(page).is_null());
        assert!(FPDFPage_GetThumbnailAsBitmap(page).is_null());
        assert!(FPDF_StructTree_GetForPage(page).is_null());
        let mut saved = Saved {
            writer: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(save),
            },
            data: Vec::new(),
        };
        assert_eq!(FPDF_SaveAsCopy(fixture.document, &mut saved.writer, 0), 1);
        FORM_OnBeforeClosePage(page, fixture.form);
        FPDF_ClosePage(page);
        let reloaded = FPDF_LoadMemDocument64(
            saved.data.as_ptr().cast(),
            saved.data.len(),
            std::ptr::null(),
        );
        assert!(!reloaded.is_null());
        let mut info: Box<FPDF_FORMFILLINFO> = Box::new(std::mem::zeroed());
        info.version = 2;
        info.m_pJsPlatform = &mut *fixture._js;
        let form = FPDFDOC_InitFormFillEnvironment(reloaded, &mut *info);
        assert!(!form.is_null());
        assert_eq!(FPDF_LoadXFA(reloaded), 1);
        let context = (&*reloaded.cast::<DocumentHandle>())
            .xfa_context
            .clone()
            .unwrap();
        assert_eq!(
            context
                .xfa_doc()
                .unwrap()
                .script_context()
                .eval("xfa.form.root.amount.rawValue")
                .unwrap()
                .as_string()
                .unwrap()
                .to_std_string_escaped(),
            "changed"
        );
        FPDFDOC_ExitFormFillEnvironment(form);
        FPDF_CloseDocument(reloaded);
    }
}
#[test]
fn disabled_xfa_and_invalid_packets_fail_without_poisoning_sdk_environment() {
    unsafe {
        let fixture = Fixture::new(true, TEMPLATE);
        assert!((&*fixture.document.cast::<DocumentHandle>())
            .xfa_context
            .is_none());
        assert_eq!(FPDF_LoadXFA(fixture.document), 0);
        assert_eq!(FPDF_GetPageCount(fixture.document), 2);
        let page = FPDF_LoadPage(fixture.document, 0);
        assert!(!page.is_null());
        assert!((&*page.cast::<PageHandle>()).pdf_page().is_some());
        FPDF_ClosePage(page);
        drop(fixture);
        let fixture = Fixture::new(false, b"<not-a-template/>");
        assert_eq!(FPDF_LoadXFA(fixture.document), 0);
        assert_eq!(FPDF_GetLastError(), 7);
        assert!(form_environment(fixture.form).unwrap().is_alive());
        let context = (&*fixture.document.cast::<DocumentHandle>())
            .xfa_context
            .clone()
            .unwrap();
        assert!(context.xfa_doc().is_none());
        assert!(context.doc_view().is_none());
    }
}

#[test]
fn newly_created_extension_pages_retain_pdf_backing_and_route_actual_xfa_geometry() {
    unsafe {
        let fixture = Fixture::new(false, TEMPLATE);
        // Form-fill setup creates the extension before loading an XFA model.
        // A clamped insertion still reports real PDF dimensions and identity.
        let page = FPDFPage_New(fixture.document, c_int::MAX, 123., 87.);
        assert!(!page.is_null());
        let handle = &*page.cast::<PageHandle>();
        let xfa = handle.xfa_page.as_ref().unwrap();
        assert_eq!(xfa.page_index(), 2);
        assert!(handle.sdk_page.xfa_page().is_some());
        assert!(Rc::ptr_eq(
            &xfa.pdf_page().unwrap(),
            handle.page.as_ref().unwrap()
        ));
        assert_eq!(
            (FPDF_GetPageWidthF(page), FPDF_GetPageHeightF(page)),
            (123., 87.)
        );
        let identity = Rc::as_ptr(handle.page.as_ref().unwrap());
        assert_eq!(FPDFPage_GenerateContent(page), 1);
        let handle = &*page.cast::<PageHandle>();
        assert_eq!(
            Rc::as_ptr(&handle.xfa_page.as_ref().unwrap().pdf_page().unwrap()),
            identity
        );
        FPDF_ClosePage(page);
        assert_eq!(FPDF_LoadXFA(fixture.document), 1);
        // Full XFA uses the real layout page geometry, while this new page
        // keeps its actual editable PDF dictionary/content as native does.
        let page = FPDFPage_New(fixture.document, -1, 321., 123.);
        assert!(!page.is_null());
        let handle = &*page.cast::<PageHandle>();
        assert_eq!(handle.xfa_page.as_ref().unwrap().page_index(), 0);
        assert_eq!(
            (
                handle.pdf_page().unwrap().width(),
                handle.pdf_page().unwrap().height()
            ),
            (321., 123.)
        );
        assert_eq!(
            (FPDF_GetPageWidthF(page), FPDF_GetPageHeightF(page)),
            (200., 300.)
        );
        assert_eq!(FPDFPage_GenerateContent(page), 1);
        assert_eq!(FPDF_GetPageCount(fixture.document), 1);
        FPDF_ClosePage(page);
    }
}
