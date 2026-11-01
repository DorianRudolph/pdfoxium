// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected script-to-embedder tests for fpdf_formfill.h's JavaScript platform.
//! All external callbacks record their inputs; assertions stay on the Rust test
//! stack. Userdata and callback tables remain boxed until after environment exit.
use super::*;
use pdfoxium_core::fpdfapi::parser::cfdf_document::CfdfDocument;
use pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use pdfoxium_core::fpdfapi::parser::cpdf_string::CpdfString;
use pdfoxium_core::fpdfdoc::cpdf_formfield::CpdfFormField;
use pdfoxium_core::fxjs::cjs_runtime::CjsRuntime;

type ExternalTimer = unsafe extern "C" fn(c_int);
#[derive(Debug, PartialEq, Eq)]
struct Alert {
    message: Vec<u16>,
    title: Vec<u16>,
    kind: c_int,
    icon: c_int,
}
#[derive(Debug, PartialEq, Eq)]
struct Response {
    question: Vec<u16>,
    title: Vec<u16>,
    default: Vec<u16>,
    label: Vec<u16>,
    password: c_int,
    capacity: c_int,
}
#[derive(Debug)]
struct Mail {
    packet: Vec<u8>,
    ui: c_int,
    to: Vec<u16>,
    subject: Vec<u16>,
    cc: Vec<u16>,
    bcc: Vec<u16>,
    message: Vec<u16>,
}
#[repr(C)]
struct Host {
    info: FPDF_FORMFILLINFO,
    platform: IPDF_JSPLATFORM,
    alerts: RefCell<Vec<Alert>>,
    responses: RefCell<Vec<Response>>,
    beeps: RefCell<Vec<c_int>>,
    pages: RefCell<Vec<c_int>>,
    prints: RefCell<Vec<[c_int; 8]>>,
    mail: RefCell<Vec<Mail>>,
    timers: RefCell<Vec<(c_int, c_int, Option<ExternalTimer>)>>,
    killed: RefCell<Vec<c_int>>,
    next_timer: Cell<c_int>,
    releases: Cell<usize>,
    active_form: Cell<*mut c_void>,
    browse_calls: RefCell<Vec<(bool, c_int)>>,
    browse_bytes: RefCell<Vec<u8>>,
    browse_required: Cell<c_int>,
    browse_actual: Cell<c_int>,
    browse_reenter: Cell<bool>,
    browse_detach_call: Cell<usize>,
}
unsafe fn platform_host<'a>(platform: *mut IPDF_JSPLATFORM) -> &'a Host {
    // Every callback receives the live platform installed by Fixture::open.
    unsafe { &*(*platform).m_pFormfillinfo.cast::<Host>() }
}
unsafe fn wide_text(value: *const u16) -> Vec<u16> {
    if value.is_null() {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut offset = 0;
    loop {
        let unit = unsafe { *value.add(offset) };
        if unit == 0 {
            return result;
        }
        result.push(unit);
        offset += 1;
    }
}
unsafe extern "C" fn release(info: *mut FPDF_FORMFILLINFO) {
    let host = unsafe { &*info.cast::<Host>() };
    host.releases.set(host.releases.get() + 1);
}
unsafe extern "C" fn alert(
    platform: *mut IPDF_JSPLATFORM,
    message: *const u16,
    title: *const u16,
    kind: c_int,
    icon: c_int,
) -> c_int {
    let host = unsafe { platform_host(platform) };
    host.alerts.borrow_mut().push(Alert {
        message: unsafe { wide_text(message) },
        title: unsafe { wide_text(title) },
        kind,
        icon,
    });
    4
}
unsafe extern "C" fn beep(platform: *mut IPDF_JSPLATFORM, kind: c_int) {
    unsafe { platform_host(platform) }
        .beeps
        .borrow_mut()
        .push(kind);
}
unsafe extern "C" fn response(
    platform: *mut IPDF_JSPLATFORM,
    question: *const u16,
    title: *const u16,
    default: *const u16,
    label: *const u16,
    password: c_int,
    buffer: *mut c_void,
    capacity: c_int,
) -> c_int {
    let host = unsafe { platform_host(platform) };
    host.responses.borrow_mut().push(Response {
        question: unsafe { wide_text(question) },
        title: unsafe { wide_text(title) },
        default: unsafe { wide_text(default) },
        label: unsafe { wide_text(label) },
        password,
        capacity,
    });
    // Response is explicitly UTF-16LE bytes, with neither a required terminator
    // nor scalar-value restrictions. Include a NUL and an unpaired surrogate.
    let bytes: Vec<_> = [0x00e9u16, 0xd800, 0, 0xd83d, 0xde00]
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect();
    if !buffer.is_null() && capacity > 0 {
        let count = bytes.len().min(capacity as usize);
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), count);
        }
    }
    bytes.len() as c_int
}
unsafe extern "C" fn browse(
    platform: *mut IPDF_JSPLATFORM,
    buffer: *mut c_void,
    capacity: c_int,
) -> c_int {
    let host = unsafe { platform_host(platform) };
    let query = buffer.is_null();
    let call = {
        let mut calls = host.browse_calls.borrow_mut();
        calls.push((query, capacity));
        calls.len()
    };
    if query && host.browse_reenter.get() {
        let form = host.active_form.get();
        if !form.is_null() {
            // Real C ABI reentry while the outer browse binding lends its
            // context to the host. WillSave is distinct from outer DocOpen.
            unsafe { FORM_DoDocumentAAction(form, 0x11) };
        }
    }
    if !query && capacity > 0 {
        let bytes = host.browse_bytes.borrow();
        let count = bytes.len().min(capacity as usize);
        unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), count) };
    }
    if host.browse_detach_call.get() == call {
        let form = host.active_form.replace(std::ptr::null_mut());
        if !form.is_null() {
            unsafe { FPDFDOC_ExitFormFillEnvironment(form) };
        }
    }
    if query {
        host.browse_required.get()
    } else {
        host.browse_actual.get()
    }
}
unsafe extern "C" fn goto_page(platform: *mut IPDF_JSPLATFORM, page: c_int) {
    unsafe { platform_host(platform) }
        .pages
        .borrow_mut()
        .push(page);
}
unsafe extern "C" fn print(
    platform: *mut IPDF_JSPLATFORM,
    ui: c_int,
    start: c_int,
    end: c_int,
    silent: c_int,
    shrink: c_int,
    image: c_int,
    reverse: c_int,
    annotations: c_int,
) {
    unsafe { platform_host(platform) }
        .prints
        .borrow_mut()
        .push([ui, start, end, silent, shrink, image, reverse, annotations]);
}
unsafe extern "C" fn mail(
    platform: *mut IPDF_JSPLATFORM,
    data: *mut c_void,
    length: c_int,
    ui: c_int,
    to: *const u16,
    subject: *const u16,
    cc: *const u16,
    bcc: *const u16,
    message: *const u16,
) {
    let packet = if data.is_null() || length <= 0 {
        Vec::new()
    } else {
        unsafe { std::slice::from_raw_parts(data.cast::<u8>(), length as usize) }.to_vec()
    };
    unsafe { platform_host(platform) }
        .mail
        .borrow_mut()
        .push(Mail {
            packet,
            ui,
            to: unsafe { wide_text(to) },
            subject: unsafe { wide_text(subject) },
            cc: unsafe { wide_text(cc) },
            bcc: unsafe { wide_text(bcc) },
            message: unsafe { wide_text(message) },
        });
}
unsafe extern "C" fn set_timer(
    info: *mut FPDF_FORMFILLINFO,
    interval: c_int,
    callback: Option<ExternalTimer>,
) -> c_int {
    let host = unsafe { &*info.cast::<Host>() };
    let id = host.next_timer.get() + 1;
    host.next_timer.set(id);
    host.timers.borrow_mut().push((id, interval, callback));
    id
}
unsafe extern "C" fn kill_timer(info: *mut FPDF_FORMFILLINFO, id: c_int) {
    unsafe { &*info.cast::<Host>() }
        .killed
        .borrow_mut()
        .push(id);
}
struct Fixture {
    document: *mut c_void,
    form: *mut c_void,
    host: Box<Host>,
}
impl Fixture {
    fn open() -> Self {
        FPDF_InitLibrary();
        let bytes = include_bytes!("../../../upstream/pdfium/testing/resources/text_form.pdf");
        let document =
            unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null()) };
        assert!(!document.is_null());
        let mut host = Box::new(Host {
            info: unsafe { std::mem::zeroed() },
            platform: unsafe { std::mem::zeroed() },
            alerts: Default::default(),
            responses: Default::default(),
            beeps: Default::default(),
            pages: Default::default(),
            prints: Default::default(),
            mail: Default::default(),
            timers: Default::default(),
            killed: Default::default(),
            next_timer: Cell::new(40),
            releases: Cell::new(0),
            active_form: Cell::new(std::ptr::null_mut()),
            browse_calls: Default::default(),
            browse_bytes: RefCell::new(b"chosen.pdf\0".to_vec()),
            browse_required: Cell::new(11),
            browse_actual: Cell::new(11),
            browse_reenter: Cell::new(false),
            browse_detach_call: Cell::new(0),
        });
        host.info.version = 2;
        host.info.Release = Some(release);
        host.info.FFI_SetTimer = Some(set_timer);
        host.info.FFI_KillTimer = Some(kill_timer);
        host.platform.version = 2;
        host.platform.app_alert = Some(alert);
        host.platform.app_response = Some(response);
        host.platform.app_beep = Some(beep);
        host.platform.Doc_gotoPage = Some(goto_page);
        host.platform.Doc_print = Some(print);
        host.platform.Doc_mail = Some(mail);
        host.platform.Field_browse = Some(browse);
        host.platform.m_pFormfillinfo = (&mut *host as *mut Host).cast();
        host.info.m_pJsPlatform = &mut host.platform;
        let form = unsafe { FPDFDOC_InitFormFillEnvironment(document, &mut host.info) };
        assert!(!form.is_null());
        host.active_form.set(form);
        Self {
            document,
            form,
            host,
        }
    }
    fn runtime(&self) -> Rc<CjsRuntime> {
        unsafe { form_environment(self.host.active_form.get()) }
            .unwrap()
            .get_or_create_runtime()
            .unwrap()
    }
    fn script(&self, source: &str) {
        self.runtime()
            .run_document_script(&[], &source.encode_utf16().collect::<Vec<_>>())
            .unwrap();
    }
    fn close_environment(&mut self) {
        self.form = std::ptr::null_mut();
        let form = self.host.active_form.replace(std::ptr::null_mut());
        if !form.is_null() {
            unsafe {
                FPDFDOC_ExitFormFillEnvironment(form);
            }
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.close_environment();
        unsafe {
            FPDF_CloseDocument(self.document);
        }
        FPDF_DestroyLibrary();
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().collect()
}

#[test]
fn scripts_bridge_utf16_dialogs_document_flags_and_real_fdf_mail_packets() {
    let fixture = Fixture::open();
    let runtime = fixture.runtime();
    let environment = unsafe { form_environment(fixture.form) }.unwrap();
    let field = environment
        .interactive_form()
        .core_form()
        .get_field(0, &[])
        .unwrap();
    let event = runtime.new_event_context();
    event.on_field_mouse_up(false, false, &field);
    event.run_script(&wide(r#"
        var selected=app.alert({cMsg:'é\ud800😀',nIcon:2,nType:3,cTitle:'題'});
        if(selected!==4) throw 'alert result';
        var answer=app.response({cQuestion:'¿?',cTitle:'題',cDefault:'déf',bPassword:true,cLabel:'名'});
        if(JSON.stringify(Array.from({length:answer.length},(_,i)=>answer.charCodeAt(i)))!=='[233,55296,0,55357,56832]') throw 'response bytes';
        app.beep(3);
        this.pageNum=999; this.pageNum=-1;
        this.print({bUI:false,nStart:2,nEnd:5,bSilent:true,bShrinkToFit:true,bPrintAsImage:false,bReverse:true,bAnnotations:true});
        this.mailDoc({bUI:false,cTo:'to@example.invalid',cCc:'cc',cBcc:'bcc',cSubject:'題',cMsg:'café'});
        this.mailForm({bUI:true,cTo:'form@example.invalid',cSubject:'form'});
    "#)).unwrap();
    assert_eq!(
        &*fixture.host.alerts.borrow(),
        &[Alert {
            message: vec![0x00e9, 0xd800, 0xd83d, 0xde00],
            title: wide("題"),
            kind: 3,
            icon: 2
        }]
    );
    assert_eq!(
        &*fixture.host.responses.borrow(),
        &[Response {
            question: wide("¿?"),
            title: wide("題"),
            default: wide("déf"),
            label: wide("名"),
            password: 1,
            capacity: 2048
        }]
    );
    assert_eq!(&*fixture.host.beeps.borrow(), &[3]);
    let last_page = unsafe { FPDF_GetPageCount(fixture.document) } - 1;
    assert_eq!(&*fixture.host.pages.borrow(), &[last_page, 0]);
    assert_eq!(&*fixture.host.prints.borrow(), &[[0, 2, 5, 1, 1, 0, 1, 1]]);
    let mail = fixture.host.mail.borrow();
    assert_eq!(mail.len(), 2);
    assert!(mail[0].packet.is_empty());
    assert_eq!(mail[0].ui, 0);
    assert_eq!(mail[0].to, wide("to@example.invalid"));
    assert_eq!(mail[0].subject, wide("題"));
    assert_eq!(mail[0].cc, wide("cc"));
    assert_eq!(mail[0].bcc, wide("bcc"));
    assert_eq!(mail[0].message, wide("café"));
    assert_eq!(mail[1].ui, 1);
    assert_eq!(mail[1].to, wide("form@example.invalid"));
    assert_eq!(mail[1].subject, wide("form"));
    let packet = CfdfDocument::parse_memory(&mail[1].packet).expect("mailForm supplies valid FDF");
    assert!(packet.root().is_some());
    assert!(mail[1]
        .packet
        .windows(b"/Fields".len())
        .any(|s| s == b"/Fields"));
}

#[test]
fn ffi_timers_execute_scripts_cancel_and_ignore_notifications_after_exit() {
    let mut fixture = Fixture::open();
    fixture.script(
        r#"
        var ticks=0;
        var repeating=app.setInterval('ticks++;app.beep(ticks);',25);
        var once=app.setTimeOut('ticks+=10;app.beep(ticks);',50);
    "#,
    );
    // Copy callback pointers out of the record before firing: callbacks can
    // synchronously register/cancel timers and mutate the host records.
    let timers = fixture.host.timers.borrow().clone();
    assert_eq!(
        timers
            .iter()
            .map(|(id, interval, _)| (*id, *interval))
            .collect::<Vec<_>>(),
        vec![(41, 25), (42, 50)]
    );
    let callback = timers[0].2.expect("host timer callback");
    unsafe {
        callback(41);
        callback(42);
        callback(42);
    }
    assert_eq!(&*fixture.host.beeps.borrow(), &[1, 11]);
    assert_eq!(&*fixture.host.killed.borrow(), &[42]);
    fixture.script("app.clearInterval(repeating); var pending=app.setInterval('app.beep(99)',15);");
    assert_eq!(&*fixture.host.killed.borrow(), &[42, 41]);
    unsafe {
        callback(41);
    }
    assert_eq!(&*fixture.host.beeps.borrow(), &[1, 11]);
    fixture.close_environment();
    assert_eq!(fixture.host.releases.get(), 1);
    assert_eq!(&*fixture.host.killed.borrow(), &[42, 41, 43]);
    // These function pointers are embedder-owned notifications, not form
    // handles. Calling them after Exit must not retain or access the dead VM.
    unsafe {
        callback(43);
        callback(41);
        callback(42);
    }
    assert_eq!(&*fixture.host.beeps.borrow(), &[1, 11]);
    assert_eq!(fixture.host.releases.get(), 1);
}

fn file_field(fixture: &Fixture) -> Rc<CpdfFormField> {
    let environment = unsafe { form_environment(fixture.host.active_form.get()) }.unwrap();
    let field = environment
        .interactive_form()
        .core_form()
        .get_field(0, &[])
        .unwrap();
    field.set_field_flags(field.field_flags() | (1 << 20));
    field
}
fn field_value(field: &CpdfFormField) -> String {
    String::from_utf16(&pdfoxium_core::fxcrt::fx_string::utf16_encode(
        &field.value(),
    ))
    .unwrap()
}
fn configure_browse(fixture: &Fixture, required: c_int, actual: c_int, bytes: &[u8]) {
    fixture.host.browse_calls.borrow_mut().clear();
    fixture.host.browse_required.set(required);
    fixture.host.browse_actual.set(actual);
    *fixture.host.browse_bytes.borrow_mut() = bytes.to_vec();
}

#[test]
fn field_browse_uses_two_pass_local_bytes_and_actual_length() {
    let fixture = Fixture::open();
    let field = file_field(&fixture);
    // The required size is an upper bound. Only the actual returned bytes,
    // excluding their trailing terminator, form the selected field value.
    configure_browse(&fixture, 32, 11, b"chosen.pdf\0ignored allocation tail");
    fixture.script("this.getField('Text Box').browseForFileToSubmit(); if(this.getField('Text Box').value!=='chosen.pdf') throw 'actual size';");
    assert_eq!(field_value(&field), "chosen.pdf");
    assert_eq!(
        &*fixture.host.browse_calls.borrow(),
        &[(true, 0), (false, 32)]
    );
    configure_browse(&fixture, 6, 6, b"a.pdf\0");
    fixture.script("this.getField('Text Box').browseForFileToSubmit('ignored argument');");
    assert_eq!(field_value(&field), "a.pdf");
    assert_eq!(
        &*fixture.host.browse_calls.borrow(),
        &[(true, 0), (false, 6)]
    );
    // This is the local byte encoding boundary, not UTF-16: no zero byte is
    // interleaved between ordinary ASCII path characters.
    configure_browse(&fixture, 9, 9, b"C:\\f.txt\0");
    fixture.script("this.getField('Text Box').browseForFileToSubmit();");
    assert_eq!(field_value(&field), "C:\\f.txt");
    // The portable pinned FromDefANSI path widens each local byte. A high
    // byte therefore remains U+00E9 rather than requiring UTF-8 input.
    configure_browse(&fixture, 9, 9, b"caf\xe9.pdf\0");
    fixture.script("this.getField('Text Box').browseForFileToSubmit();");
    assert_eq!(field_value(&field), "café.pdf");
}

#[test]
fn field_browse_rejects_bad_lengths_missing_service_and_wrong_field_type() {
    let mut fixture = Fixture::open();
    let field = file_field(&fixture);
    fixture.script("this.getField('Text Box').value='keep';");
    for required in [0, -1] {
        configure_browse(&fixture, required, 6, b"a.pdf\0");
        fixture.script("this.getField('Text Box').browseForFileToSubmit();");
        assert_eq!(&*fixture.host.browse_calls.borrow(), &[(true, 0)]);
        assert_eq!(field_value(&field), "keep");
    }
    for actual in [0, -1, 7] {
        configure_browse(&fixture, 6, actual, b"a.pdf\0");
        fixture.script("this.getField('Text Box').browseForFileToSubmit();");
        assert_eq!(
            &*fixture.host.browse_calls.borrow(),
            &[(true, 0), (false, 6)]
        );
        assert_eq!(field_value(&field), "keep");
    }
    // A successful empty path (terminator alone) also preserves the value.
    configure_browse(&fixture, 1, 1, b"\0");
    fixture.script("this.getField('Text Box').browseForFileToSubmit();");
    assert_eq!(field_value(&field), "keep");
    fixture.host.browse_calls.borrow_mut().clear();
    fixture.host.platform.Field_browse = None;
    fixture.script("this.getField('Text Box').browseForFileToSubmit();");
    assert!(fixture.host.browse_calls.borrow().is_empty());
    assert_eq!(field_value(&field), "keep");
    fixture.host.platform.Field_browse = Some(browse);
    field.set_field_flags(field.field_flags() & !(1 << 20));
    fixture.script("var rejected=false; try{this.getField('Text Box').browseForFileToSubmit()}catch(e){rejected=e.includes('wrong type')} if(!rejected) throw 'file-select flag';");
    assert!(fixture.host.browse_calls.borrow().is_empty());
    assert_eq!(field_value(&field), "keep");
}

#[test]
fn field_browse_query_reenters_document_action_before_outer_script_continues() {
    let fixture = Fixture::open();
    let field = file_field(&fixture);
    let environment = unsafe { form_environment(fixture.form) }.unwrap();
    let document = environment.document().unwrap();
    let root = document.borrow().root().unwrap();
    let mut action = CpdfDictionary::new();
    action.set_new_name_for(b"S", b"JavaScript");
    action.set_new_for(
        b"JS",
        CpdfString::from_bytes(None, b"browseNested++; app.beep(73);"),
    );
    let mut actions = CpdfDictionary::new();
    actions.set_new_for(b"WS", action);
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(b"AA", actions);
    fixture.host.browse_reenter.set(true);
    configure_browse(&fixture, 11, 11, b"chosen.pdf\0");
    fixture.script("var browseNested=0; this.getField('Text Box').browseForFileToSubmit(); if(browseNested!==1) throw 'nested script did not finish';");
    assert_eq!(&*fixture.host.beeps.borrow(), &[73]);
    assert_eq!(
        &*fixture.host.browse_calls.borrow(),
        &[(true, 0), (false, 11)]
    );
    assert_eq!(field_value(&field), "chosen.pdf");
}

#[test]
fn field_browse_detachment_during_either_pass_does_not_reuse_handle_or_mutate_field() {
    for detach_call in [1, 2] {
        let fixture = Fixture::open();
        let field = file_field(&fixture);
        fixture.script("this.getField('Text Box').value='before';");
        configure_browse(&fixture, 11, 11, b"chosen.pdf\0");
        fixture.host.browse_detach_call.set(detach_call);
        let runtime = fixture.runtime();
        // Keep only safe owners for assertions after the C callback exits the
        // form environment. No caller uses the old raw form handle afterward.
        let _result = runtime.run_document_script(
            &[],
            &wide("this.getField('Text Box').browseForFileToSubmit();"),
        );
        assert!(fixture.host.active_form.get().is_null());
        assert_eq!(fixture.host.releases.get(), 1);
        assert_eq!(fixture.host.browse_calls.borrow().len(), detach_call);
        assert_eq!(field_value(&field), "before");
    }
}
