//! Ordinary FORM input follows the upstream SelectAllText / UndoRedo caller
//! sequence and survives saving through either implementation.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::cell::Cell;
use std::ffi::{c_int, c_ulong, c_void};
use std::marker::PhantomData;
use std::ptr;

const FIXTURE: &[u8] = include_bytes!("../../../upstream/pdfium/testing/resources/text_form.pdf");
const VALUE: &str = "Ported PDF";

// The interface must stay at a stable address for the entire FORM lifetime.
#[repr(C)]
struct Host {
    info: FPDF_FORMFILLINFO,
    document: FPDF_DOCUMENT,
    page: FPDF_PAGE,
    invalidations: Cell<usize>,
    changes: Cell<usize>,
    focus_notifications: Cell<usize>,
    releases: Cell<usize>,
    next_timer: Cell<c_int>,
}

impl Host {
    fn new(document: FPDF_DOCUMENT, page: FPDF_PAGE) -> Box<Self> {
        // All fields are C scalar values or nullable function/data pointers.
        let mut info: FPDF_FORMFILLINFO = unsafe { std::mem::zeroed() };
        info.version = 2;
        info.xfa_disabled = 1;
        info.Release = Some(release);
        info.FFI_Invalidate = Some(invalidate);
        info.FFI_SetCursor = Some(set_cursor);
        info.FFI_SetTimer = Some(set_timer);
        info.FFI_KillTimer = Some(kill_timer);
        info.FFI_GetPage = Some(get_page);
        info.FFI_GetCurrentPage = Some(get_current_page);
        info.FFI_GetRotation = Some(get_rotation);
        info.FFI_OnChange = Some(on_change);
        info.FFI_SetTextFieldFocus = Some(text_focus);
        Box::new(Self {
            info,
            document,
            page,
            invalidations: Cell::new(0),
            changes: Cell::new(0),
            focus_notifications: Cell::new(0),
            releases: Cell::new(0),
            next_timer: Cell::new(0),
        })
    }
}

unsafe fn host<'a>(info: *mut FPDF_FORMFILLINFO) -> &'a Host {
    unsafe { &*info.cast::<Host>() }
}
unsafe extern "C" fn release(info: *mut FPDF_FORMFILLINFO) {
    let host = unsafe { host(info) };
    host.releases.set(host.releases.get() + 1);
}
unsafe extern "C" fn invalidate(
    info: *mut FPDF_FORMFILLINFO,
    _: FPDF_PAGE,
    _: f64,
    _: f64,
    _: f64,
    _: f64,
) {
    let host = unsafe { host(info) };
    host.invalidations.set(host.invalidations.get() + 1);
}
unsafe extern "C" fn on_change(info: *mut FPDF_FORMFILLINFO) {
    let host = unsafe { host(info) };
    host.changes.set(host.changes.get() + 1);
}
unsafe extern "C" fn text_focus(
    info: *mut FPDF_FORMFILLINFO,
    _: FPDF_WIDESTRING,
    _: FPDF_DWORD,
    _: FPDF_BOOL,
) {
    let host = unsafe { host(info) };
    host.focus_notifications
        .set(host.focus_notifications.get() + 1);
}
unsafe extern "C" fn set_cursor(_: *mut FPDF_FORMFILLINFO, _: c_int) {}
unsafe extern "C" fn set_timer(
    info: *mut FPDF_FORMFILLINFO,
    _: c_int,
    _: Option<unsafe extern "C" fn(c_int)>,
) -> c_int {
    let host = unsafe { host(info) };
    let id = host.next_timer.get() + 1;
    host.next_timer.set(id);
    // This synchronous test does not advance the host's timer clock.
    id
}
unsafe extern "C" fn kill_timer(_: *mut FPDF_FORMFILLINFO, _: c_int) {}
unsafe extern "C" fn get_page(
    info: *mut FPDF_FORMFILLINFO,
    document: FPDF_DOCUMENT,
    index: c_int,
) -> FPDF_PAGE {
    let host = unsafe { host(info) };
    if document == host.document && index == 0 {
        host.page
    } else {
        ptr::null_mut()
    }
}
unsafe extern "C" fn get_current_page(
    info: *mut FPDF_FORMFILLINFO,
    document: FPDF_DOCUMENT,
) -> FPDF_PAGE {
    unsafe { get_page(info, document, 0) }
}
unsafe extern "C" fn get_rotation(_: *mut FPDF_FORMFILLINFO, _: FPDF_PAGE) -> c_int {
    0
}

struct Session<'a> {
    bindings: &'a dyn PdfiumLibraryBindings,
    document: FPDF_DOCUMENT,
    page: FPDF_PAGE,
    form: FPDF_FORMHANDLE,
    host: Box<Host>,
    _bytes: PhantomData<&'a [u8]>,
}
impl<'a> Session<'a> {
    fn open(bindings: &'a dyn PdfiumLibraryBindings, bytes: &'a [u8]) -> Self {
        unsafe {
            let document = bindings.FPDF_LoadMemDocument64(bytes, None);
            assert!(!document.is_null());
            let page = bindings.FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let mut host = Host::new(document, page);
            let form = bindings.FPDFDOC_InitFormFillEnvironment(document, &mut host.info);
            assert!(!form.is_null());
            bindings.FORM_OnAfterLoadPage(page, form);
            bindings.FPDF_SetFormFieldHighlightAlpha(form, 0);
            Self {
                bindings,
                document,
                page,
                form,
                host,
                _bytes: PhantomData,
            }
        }
    }
    fn text(&self, selected: bool) -> String {
        let get = |buffer, len| unsafe {
            if selected {
                self.bindings
                    .FORM_GetSelectedText(self.form, self.page, buffer, len)
            } else {
                self.bindings
                    .FORM_GetFocusedText(self.form, self.page, buffer, len)
            }
        };
        let bytes = get(ptr::null_mut(), 0);
        assert!(bytes >= 2 && bytes % 2 == 0);
        let mut words = vec![0u16; bytes as usize / 2];
        assert_eq!(get(words.as_mut_ptr().cast(), bytes), bytes);
        assert_eq!(words.pop(), Some(0));
        String::from_utf16(&words).unwrap()
    }
    fn render(&self) -> Vec<u8> {
        unsafe {
            let bitmap = self.bindings.FPDFBitmap_Create(300, 300, 1);
            assert!(!bitmap.is_null());
            assert_eq!(
                self.bindings
                    .FPDFBitmap_FillRect(bitmap, 0, 0, 300, 300, 0xffffffff),
                1
            );
            self.bindings.FPDF_RenderPageBitmap(
                bitmap,
                self.page,
                0,
                0,
                300,
                300,
                0,
                FPDF_NO_NATIVETEXT,
            );
            self.bindings.FPDF_FFLDraw(
                self.form,
                bitmap,
                self.page,
                0,
                0,
                300,
                300,
                0,
                FPDF_NO_NATIVETEXT,
            );
            let pixels = self.bindings.FPDFBitmap_GetBuffer_as_vec(bitmap);
            self.bindings.FPDFBitmap_Destroy(bitmap);
            pixels
        }
    }
    fn shutdown(&mut self) {
        if self.form.is_null() {
            return;
        }
        unsafe {
            self.bindings.FORM_OnBeforeClosePage(self.page, self.form);
            self.bindings.FPDF_ClosePage(self.page);
            self.bindings.FPDFDOC_ExitFormFillEnvironment(self.form);
            self.bindings.FPDF_CloseDocument(self.document);
        }
        self.form = ptr::null_mut();
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[repr(C)]
struct Sink {
    interface: FPDF_FILEWRITE,
    bytes: Vec<u8>,
}
unsafe extern "C" fn append(
    this: *mut FPDF_FILEWRITE,
    data: *const c_void,
    size: c_ulong,
) -> c_int {
    let sink = unsafe { &mut *this.cast::<Sink>() };
    if size != 0 {
        sink.bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(data.cast::<u8>(), size as usize)
        });
    }
    1
}

fn input_and_save(bindings: &dyn PdfiumLibraryBindings) -> Vec<u8> {
    let mut session = Session::open(bindings, FIXTURE);
    unsafe {
        // These are the same page-space coordinates as the source embedder test.
        bindings.FORM_OnMouseMove(session.form, session.page, 0, 115.0, 115.0);
        bindings.FORM_OnLButtonDown(session.form, session.page, 0, 115.0, 115.0);
        bindings.FORM_OnLButtonUp(session.form, session.page, 0, 115.0, 115.0);
        assert_ne!(
            bindings.FORM_OnFocus(session.form, session.page, 0, 115.0, 115.0),
            0
        );
        assert_eq!(session.text(false), "");
        assert_eq!(session.text(true), "");
        assert_eq!(bindings.FORM_CanUndo(session.form, session.page), 0);
        assert_eq!(bindings.FORM_CanRedo(session.form, session.page), 0);
        for ch in "ABCDE".chars() {
            assert_ne!(
                bindings.FORM_OnChar(session.form, session.page, ch as c_int, 0),
                0
            );
        }
        assert_eq!(session.text(false), "ABCDE");
        assert_ne!(bindings.FORM_CanUndo(session.form, session.page), 0);
        assert_ne!(bindings.FORM_Undo(session.form, session.page), 0);
        assert_eq!(session.text(false), "ABCD");
        assert_ne!(bindings.FORM_CanRedo(session.form, session.page), 0);
        assert_ne!(bindings.FORM_Redo(session.form, session.page), 0);
        assert_eq!(session.text(false), "ABCDE");
        assert_ne!(bindings.FORM_SelectAllText(session.form, session.page), 0);
        assert_eq!(session.text(true), "ABCDE");
        let replacement: Vec<u16> = VALUE.encode_utf16().chain(Some(0)).collect();
        bindings.FORM_ReplaceSelection(session.form, session.page, replacement.as_ptr());
        assert_eq!(session.text(false), VALUE);
        assert_eq!(session.text(true), "");
        assert_ne!(bindings.FORM_Undo(session.form, session.page), 0);
        assert_eq!(session.text(false), "ABCDE");
        assert_ne!(bindings.FORM_Redo(session.form, session.page), 0);
        assert_eq!(session.text(false), VALUE);
        assert!(
            field_ink(&session.render()) > 20,
            "live edit must draw text"
        );
        assert_ne!(bindings.FORM_ForceToKillFocus(session.form), 0);
        assert!(session.host.invalidations.get() > 0);
        assert!(session.host.focus_notifications.get() > 0);
        assert!(session.host.changes.get() > 0);
        let mut sink = Sink {
            interface: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(append),
            },
            bytes: Vec::new(),
        };
        assert_ne!(
            bindings.FPDF_SaveAsCopy(session.document, &mut sink.interface, 0),
            0
        );
        session.shutdown();
        assert_eq!(session.host.releases.get(), 1);
        sink.bytes
    }
}

fn field_ink(pixels: &[u8]) -> usize {
    // Page-space field [100,100,200,130] at 72 dpi, excluding its edges.
    (172..198)
        .flat_map(|y| (102..198).map(move |x| (y * 300 + x) * 4))
        .filter(|&offset| pixels[offset..offset + 3] != [255, 255, 255])
        .count()
}

#[test]
fn public_form_input_commit_save_reload_in_both_libraries() {
    common::with_libraries(|native, rust| {
        let saved = [
            input_and_save(native.as_ref()),
            input_and_save(rust.as_ref()),
        ];
        for reader in [native.as_ref(), rust.as_ref()] {
            let baseline = Session::open(reader, FIXTURE).render();
            assert_eq!(field_ink(&baseline), 0);
            for bytes in &saved {
                let session = Session::open(reader, bytes);
                let pixels = session.render();
                assert_ne!(pixels, baseline, "saved text must change the rendering");
                assert!(
                    field_ink(&pixels) > 20,
                    "saved appearance must contain text"
                );
                unsafe {
                    assert_ne!(
                        reader.FORM_OnFocus(session.form, session.page, 0, 115.0, 115.0),
                        0
                    );
                    assert_eq!(session.text(false), VALUE);
                    assert_ne!(reader.FORM_ForceToKillFocus(session.form), 0);
                }
            }
        }
    });
}
