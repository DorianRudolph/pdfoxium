// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Public ABI behavior for interpreter-free and XFA-free builds.
#[cfg(not(feature = "js"))]
use super::unimplemented_abi::{FPDF_FORMFILLINFO, IPDF_JSPLATFORM};
use super::*;

#[repr(C)]
struct Sink {
    writer: FPDF_FILEWRITE,
    bytes: Vec<u8>,
}
impl Sink {
    fn new() -> Self {
        Self {
            writer: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(Self::write),
            },
            bytes: Vec::new(),
        }
    }
    unsafe extern "C" fn write(
        writer: *mut FPDF_FILEWRITE,
        data: *const c_void,
        size: c_ulong,
    ) -> c_int {
        let sink = unsafe { &mut *writer.cast::<Self>() };
        sink.bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(data.cast::<u8>(), size as usize)
        });
        1
    }
}
fn fixture() -> Vec<u8> {
    // A valid static PDF with source-queryable JavaScript and a non-script Next
    // action. The disabled interpreter must not swallow the named action.
    let content = "1 0 0 rg 5 5 20 20 re f";
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R /OpenAction 4 0 R /Names << /JavaScript << /Names [(script) 5 0 R] >> >> >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Resources << >> /Contents 6 0 R >>".into(),
        r#"<< /S /JavaScript /JS (app.alert\("unexpected"\)) /Next << /S /Named /N /Print >> >>"#.into(),
        r#"<< /S /JavaScript /JS (app.alert\("document script"\)) >>"#.into(),
        format!("<< /Length {} >>\nstream\n{}\nendstream", content.len(), content),
    ];
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0usize];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{}\nendobj\n", i + 1, object).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    bytes
}
unsafe fn load(bytes: &[u8]) -> *mut c_void {
    let document =
        unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) };
    assert!(!document.is_null());
    document
}
unsafe fn pixels(document: *mut c_void) -> Vec<u8> {
    let page = unsafe { FPDF_LoadPage(document, 0) };
    assert!(!page.is_null());
    let bitmap = FPDFBitmap_Create(40, 40, 1);
    unsafe {
        FPDFBitmap_FillRect(bitmap, 0, 0, 40, 40, 0xffffffff);
        FPDF_RenderPageBitmap(bitmap, page, 0, 0, 40, 40, 0, 0);
        let bytes =
            std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), 40 * 40 * 4)
                .to_vec();
        assert_eq!(
            &bytes[(20 * 40 + 10) * 4..(20 * 40 + 10) * 4 + 4],
            &[0, 0, 255, 255]
        );
        FPDFBitmap_Destroy(bitmap);
        FPDF_ClosePage(page);
        bytes
    }
}

#[test]
fn static_render_and_action_queries_survive_save_without_xfa() {
    FPDF_InitLibrary();
    let bytes = fixture();
    unsafe {
        let document = load(&bytes);
        assert_eq!(FPDF_GetPageCount(document), 1);
        assert_eq!(FPDF_LoadXFA(document), 0);
        assert_eq!(FPDFDoc_GetJavaScriptActionCount(document), 1);
        let action = FPDFDoc_GetJavaScriptAction(document, 0);
        assert!(!action.is_null());
        let count = FPDFJavaScriptAction_GetScript(action, std::ptr::null_mut(), 0);
        let mut script = vec![0u16; count as usize / 2];
        assert_eq!(
            FPDFJavaScriptAction_GetScript(action, script.as_mut_ptr(), count),
            count
        );
        assert_eq!(
            String::from_utf16(&script[..script.len() - 1]).unwrap(),
            "app.alert(\"document script\")"
        );
        FPDFDoc_CloseJavaScriptAction(action);
        let before = pixels(document);
        let mut sink = Sink::new();
        assert_ne!(FPDF_SaveAsCopy(document, &mut sink.writer, 0), 0);
        let reopened = load(&sink.bytes);
        assert_eq!(FPDFDoc_GetJavaScriptActionCount(reopened), 1);
        assert_eq!(pixels(reopened), before);
        FPDF_CloseDocument(reopened);
        FPDF_CloseDocument(document);
    }
    FPDF_DestroyLibrary();
}

#[cfg(not(feature = "js"))]
#[test]
fn js_disabled_preserves_named_next_action_with_opted_in_host_and_version_one_forms() {
    #[repr(C)]
    struct Host {
        info: FPDF_FORMFILLINFO,
        platform: IPDF_JSPLATFORM,
        named: RefCell<Vec<Vec<u8>>>,
        alerts: Cell<usize>,
    }
    unsafe extern "C" fn named(info: *mut FPDF_FORMFILLINFO, name: *const c_char) {
        let host = unsafe { &*info.cast::<Host>() };
        host.named
            .borrow_mut()
            .push(unsafe { CStr::from_ptr(name) }.to_bytes().to_vec());
    }
    unsafe extern "C" fn alert(
        platform: *mut IPDF_JSPLATFORM,
        _: *const u16,
        _: *const u16,
        _: c_int,
        _: c_int,
    ) -> c_int {
        let host = unsafe { &*(*platform).m_pFormfillinfo.cast::<Host>() };
        host.alerts.set(host.alerts.get() + 1);
        0
    }
    FPDF_InitLibrary();
    let bytes = fixture();
    unsafe {
        let document = load(&bytes);
        let mut host = Box::new(Host {
            info: std::mem::zeroed(),
            platform: std::mem::zeroed(),
            named: RefCell::new(Vec::new()),
            alerts: Cell::new(0),
        });
        host.info.version = 1;
        host.platform.version = 3;
        host.platform.app_alert = Some(alert);
        host.platform.m_pFormfillinfo = (&mut host.info as *mut FPDF_FORMFILLINFO).cast();
        host.info.m_pJsPlatform = &mut host.platform;
        host.info.FFI_ExecuteNamedAction = Some(named);
        let form = FPDFDOC_InitFormFillEnvironment(document, &mut host.info);
        assert!(!form.is_null());
        FORM_DoDocumentJSAction(form);
        FORM_DoDocumentOpenAction(form);
        assert_eq!(host.alerts.get(), 0);
        assert_eq!(host.named.borrow().as_slice(), &[b"Print".to_vec()]);
        FPDFDOC_ExitFormFillEnvironment(form);
        FPDF_CloseDocument(document);
    }
    FPDF_DestroyLibrary();
}

#[cfg(not(feature = "js"))]
#[test]
fn metadata_clock_without_js_supplies_real_utc_fields() {
    use pdfoxium_core::fxcrt::fx_date_helpers as date;
    use std::time::{SystemTime, UNIX_EPOCH};
    let before = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (local, utc) = document_date_fields(None).expect("system clock metadata");
    let after = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let observed = date::make_date(
        date::make_day(utc.year, utc.month as i32 - 1, utc.day as i32),
        date::make_time(utc.hour as i32, utc.minute as i32, utc.second as i32, 0),
    ) / 1000.0;
    assert!(observed >= before as f64 && observed <= after as f64);
    let text = fpdf_editpage::format_pdf_date(local, Some(utc));
    assert_eq!(text.len(), 23);
    assert!(text.ends_with(b"+00'00'"));
}
