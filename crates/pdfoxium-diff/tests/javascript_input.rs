//! Connected document/field JavaScript, actual C callback reentry, and saved
//! appearances are exercised through the public interfaces of both libraries.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::any::Any;
use std::cell::Cell;
use std::ffi::{c_int, c_ulong, c_void};
use std::marker::PhantomData;
use std::ptr;
fn js(source: &str) -> String {
    // Hex strings keep JavaScript parentheses and escapes out of PDF syntax.
    let hex: String = source.bytes().map(|byte| format!("{byte:02x}")).collect();
    format!("<< /S /JavaScript /JS <{hex}> >>")
}

fn fixture(detach: bool) -> Vec<u8> {
    let document_script = if detach {
        "this.getField('input').value='before'; this.gotoNamedDest('target'); this.getField('input').value='after';"
    } else {
        "var phase2Counter=1; this.getField('input').value='3'; this.gotoNamedDest('target'); if(this.getField('nested').valueAsString==='7') this.getField('input').value='4'; else this.getField('input').value='failed';"
    };
    fixture_with_actions(Actions {
        document: document_script,
        open: "phase2Counter++; if(phase2Counter===2) this.getField('input').value='5';",
        page_open: "this.getField('nested').value='7';",
        keystroke: None,
        validate: "if(event.value==='bad') event.rc=false;",
        format: "event.value='['+event.value+']'; this.getField('nested').value='F'+event.value;",
        calculate: "event.value=Number(this.getField('input').value)*2;",
    })
}

struct Actions<'a> {
    document: &'a str,
    open: &'a str,
    page_open: &'a str,
    keystroke: Option<&'a str>,
    validate: &'a str,
    format: &'a str,
    calculate: &'a str,
}

fn fixture_with_actions(actions: Actions<'_>) -> Vec<u8> {
    let open = js(actions.open);
    let page_open = js(actions.page_open);
    let keystroke = actions
        .keystroke
        .map(|source| format!("/K {}", js(source)))
        .unwrap_or_default();
    let validate = js(actions.validate);
    let format = js(actions.format);
    let calculate = js(actions.calculate);
    let objects = [
        format!("<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(phase2) 9 0 R] >> /Dests << /Names [(target) [3 0 R /XYZ 0 300 1]] >> >> /OpenAction {open} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << >> /Annots [6 0 R 7 0 R 8 0 R] /AA << /O {page_open} >> >>"),
        "<< /Fields [6 0 R 7 0 R 8 0 R] /CO [7 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
        format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 220 180 250] /FT /Tx /T (input) /V () /AA << {keystroke} /V {validate} /F {format} >> >>"),
        format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 160 180 190] /FT /Tx /Ff 1 /T (calc) /V () /AA << /C {calculate} >> >>"),
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 100 180 130] /FT /Tx /T (nested) /V () >>".to_owned(),
        js(actions.document),
    ];
    pdf_with_objects(&objects)
}

fn pdf_with_objects(objects: &[String]) -> Vec<u8> {
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    bytes
}

// The C table is first and the entire host remains boxed until all callbacks
// have returned. Shared callbacks use Cells, never a mutable host borrow.
#[repr(C)]
struct Host<'a> {
    info: FPDF_FORMFILLINFO,
    // pdfium-render does not export the platform type, so retain the
    // correctly inferred bindgen allocation behind Any without copying ABI.
    _platform: Box<dyn Any>,
    bindings: &'a dyn PdfiumLibraryBindings,
    document: FPDF_DOCUMENT,
    page: FPDF_PAGE,
    form: Cell<FPDF_FORMHANDLE>,
    gotos: Cell<usize>,
    goto_page: Cell<c_int>,
    releases: Cell<usize>,
    next_timer: Cell<c_int>,
    detach: bool,
}
impl<'a> Host<'a> {
    fn new(
        bindings: &'a dyn PdfiumLibraryBindings,
        document: FPDF_DOCUMENT,
        page: FPDF_PAGE,
        detach: bool,
    ) -> Box<Self> {
        // These bindgen structures contain C scalars and nullable pointers.
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
        info.FFI_DoGoToAction = Some(goto);
        let mut platform = Box::new(unsafe { std::mem::zeroed() });
        info.m_pJsPlatform = &mut *platform;
        // Version 2 asks native PDFium to create its managed V8 isolate.
        // A non-null JS platform is also the public API's JS enable switch.
        platform.version = 2;
        Box::new(Self {
            info,
            _platform: platform,
            bindings,
            document,
            page,
            form: Cell::new(ptr::null_mut()),
            gotos: Cell::new(0),
            goto_page: Cell::new(-1),
            releases: Cell::new(0),
            next_timer: Cell::new(0),
            detach,
        })
    }
}
unsafe fn host<'a>(info: *mut FPDF_FORMFILLINFO) -> &'a Host<'a> {
    unsafe { &*info.cast::<Host<'a>>() }
}
unsafe extern "C" fn release(info: *mut FPDF_FORMFILLINFO) {
    let h = unsafe { host(info) };
    h.releases.set(h.releases.get() + 1);
}
unsafe extern "C" fn goto(
    info: *mut FPDF_FORMFILLINFO,
    page_index: c_int,
    _: c_int,
    _: *mut f32,
    _: c_int,
) {
    let h = unsafe { host(info) };
    h.goto_page.set(page_index);
    let count = h.gotos.get();
    h.gotos.set(count + 1);
    if count != 0 {
        return;
    }
    // Snapshot before crossing the public ABI again. No RefCell or exclusive
    // borrow of any C host state is held while the nested script runs.
    let bindings = h.bindings;
    let page = h.page;
    let form = h.form.get();
    if form.is_null() || page_index != 0 {
        return;
    }
    if h.detach {
        h.form.set(ptr::null_mut());
        unsafe { bindings.FPDFDOC_ExitFormFillEnvironment(form) };
    } else {
        unsafe { bindings.FORM_DoPageAAction(page, form, 0) };
    }
}
unsafe extern "C" fn invalidate(
    _: *mut FPDF_FORMFILLINFO,
    _: FPDF_PAGE,
    _: f64,
    _: f64,
    _: f64,
    _: f64,
) {
}
unsafe extern "C" fn on_change(_: *mut FPDF_FORMFILLINFO) {}
unsafe extern "C" fn text_focus(
    _: *mut FPDF_FORMFILLINFO,
    _: FPDF_WIDESTRING,
    _: FPDF_DWORD,
    _: FPDF_BOOL,
) {
}
unsafe extern "C" fn set_cursor(_: *mut FPDF_FORMFILLINFO, _: c_int) {}
unsafe extern "C" fn set_timer(
    info: *mut FPDF_FORMFILLINFO,
    _: c_int,
    _: Option<unsafe extern "C" fn(c_int)>,
) -> c_int {
    let h = unsafe { host(info) };
    let id = h.next_timer.get() + 1;
    h.next_timer.set(id);
    id
}
unsafe extern "C" fn kill_timer(_: *mut FPDF_FORMFILLINFO, _: c_int) {}
unsafe extern "C" fn get_page(
    info: *mut FPDF_FORMFILLINFO,
    document: FPDF_DOCUMENT,
    index: c_int,
) -> FPDF_PAGE {
    let h = unsafe { host(info) };
    if document == h.document && index == 0 {
        h.page
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
    host: Box<Host<'a>>,
    _bytes: PhantomData<&'a [u8]>,
}
impl<'a> Session<'a> {
    fn open(bindings: &'a dyn PdfiumLibraryBindings, bytes: &'a [u8], detach: bool) -> Self {
        unsafe {
            let document = bindings.FPDF_LoadMemDocument64(bytes, None);
            assert!(!document.is_null());
            let page = bindings.FPDF_LoadPage(document, 0);
            assert!(!page.is_null());
            let mut host = Host::new(bindings, document, page, detach);
            let form = bindings.FPDFDOC_InitFormFillEnvironment(document, &mut host.info);
            assert!(!form.is_null());
            host.form.set(form);
            bindings.FORM_OnAfterLoadPage(page, form);
            bindings.FPDF_SetFormFieldHighlightAlpha(form, 0);
            Self {
                bindings,
                document,
                page,
                host,
                _bytes: PhantomData,
            }
        }
    }
    fn form(&self) -> FPDF_FORMHANDLE {
        let form = self.host.form.get();
        assert!(
            !form.is_null(),
            "caller must not use a detached form handle"
        );
        form
    }
    fn value(&self, index: c_int) -> String {
        unsafe {
            let annotation = self.bindings.FPDFPage_GetAnnot(self.page, index);
            assert!(!annotation.is_null());
            let form = self.form();
            let length =
                self.bindings
                    .FPDFAnnot_GetFormFieldValue(form, annotation, ptr::null_mut(), 0);
            assert!(length >= 2 && length % 2 == 0);
            let mut words = vec![0u16; length as usize / 2];
            assert_eq!(
                self.bindings.FPDFAnnot_GetFormFieldValue(
                    form,
                    annotation,
                    words.as_mut_ptr(),
                    length
                ),
                length
            );
            self.bindings.FPDFPage_CloseAnnot(annotation);
            assert_eq!(words.pop(), Some(0));
            String::from_utf16(&words).unwrap()
        }
    }
    fn assert_values(&self, input: &str, calc: &str, nested: &str) {
        assert_eq!(self.value(0), input, "input value");
        assert_eq!(self.value(1), calc, "calculation order/action");
        assert_eq!(self.value(2), nested, "format action side effect");
    }
    fn commit(&self, value: &str) {
        unsafe {
            let form = self.form();
            assert_ne!(
                self.bindings.FORM_OnFocus(form, self.page, 0, 30.0, 235.0),
                0,
                "focus input before editing {value:?}"
            );
            assert_ne!(
                self.bindings.FORM_SelectAllText(form, self.page),
                0,
                "select input before editing {value:?}"
            );
            let words: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
            self.bindings
                .FORM_ReplaceSelection(form, self.page, words.as_ptr());
            assert_ne!(
                self.bindings.FORM_ForceToKillFocus(form),
                0,
                "release focus after replacing input with {value:?}"
            );
        }
    }
    fn ink(&self) -> [usize; 3] {
        unsafe {
            let bitmap = self.bindings.FPDFBitmap_Create(300, 300, 1);
            assert!(!bitmap.is_null());
            assert_ne!(
                self.bindings
                    .FPDFBitmap_FillRect(bitmap, 0, 0, 300, 300, 0xffffffff),
                0
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
                self.form(),
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
            [50..80, 110..140, 170..200].map(|rows| {
                rows.flat_map(|y| (20..180).map(move |x| (y * 300 + x) * 4))
                    .filter(|&offset| pixels[offset..offset + 3] != [255, 255, 255])
                    .count()
            })
        }
    }
    fn save(&self) -> Vec<u8> {
        let mut sink = Sink {
            interface: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(append),
            },
            bytes: Vec::new(),
        };
        assert_ne!(
            unsafe {
                self.bindings
                    .FPDF_SaveAsCopy(self.document, &mut sink.interface, 0)
            },
            0
        );
        sink.bytes
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        unsafe {
            let form = self.host.form.replace(ptr::null_mut());
            if !form.is_null() {
                self.bindings.FORM_OnBeforeClosePage(self.page, form);
                self.bindings.FPDFDOC_ExitFormFillEnvironment(form);
            }
            self.bindings.FPDF_ClosePage(self.page);
            self.bindings.FPDF_CloseDocument(self.document);
        }
        if !std::thread::panicking() {
            assert_eq!(self.host.releases.get(), 1);
        }
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

fn run_and_save(bindings: &dyn PdfiumLibraryBindings, bytes: &[u8]) -> Vec<u8> {
    let session = Session::open(bindings, bytes, false);
    unsafe { bindings.FORM_DoDocumentJSAction(session.form()) };
    assert_eq!(
        session.host.gotos.get(),
        1,
        "named destination must call the actual C host"
    );
    assert_eq!(session.host.goto_page.get(), 0);
    session.assert_values("4", "8", "F[4]");
    unsafe { bindings.FORM_DoDocumentOpenAction(session.form()) };
    session.assert_values("5", "10", "F[5]");
    session.commit("12");
    session.assert_values("12", "24", "F[12]");
    session.commit("bad");
    session.assert_values("12", "24", "F[12]");
    assert!(
        session.ink().iter().all(|&count| count > 10),
        "all three generated appearances must draw text"
    );
    session.save()
}

#[test]
fn javascript_input_reentry_calculation_format_save_reload() {
    common::with_libraries(|native, rust| {
        let bytes = fixture(false);
        let saved = [
            run_and_save(native.as_ref(), &bytes),
            run_and_save(rust.as_ref(), &bytes),
        ];
        for reader in [native.as_ref(), rust.as_ref()] {
            for bytes in &saved {
                let session = Session::open(reader, bytes, false);
                // Opening the PDF does not execute document scripts. Saved raw
                // values and generated format appearances must survive together.
                session.assert_values("12", "24", "F[12]");
                assert!(session.ink().iter().all(|&count| count > 10));
            }
        }
    });
}

fn run_standard_actions_and_save(
    bindings: &dyn PdfiumLibraryBindings,
    bytes: &[u8],
    backend: &str,
) -> Vec<u8> {
    eprintln!("standard AF/util actions: {backend}");
    let session = Session::open(bindings, bytes, false);
    unsafe { bindings.FORM_DoDocumentJSAction(session.form()) };
    session.assert_values("1234", "2468", "2024-02-29|$1,234.00|1234.00");
    session.commit("12.75");
    session.assert_values("12.75", "25.5", "2024-02-29|$12.75|12.75");
    // This numeric replacement passes AFNumber_Keystroke but the validation
    // event vetoes it. Neither calculation nor formatting may commit it.
    session.commit("6000");
    session.assert_values("12.75", "25.5", "2024-02-29|$12.75|12.75");
    assert!(session.ink().iter().all(|&count| count > 10));
    session.save()
}

#[test]
fn standard_af_and_util_actions_validate_calculate_format_save_reload() {
    common::with_libraries(|native, rust| {
        let bytes = fixture_with_actions(Actions {
            document: "var field=this.getField('input'); if(field.type!=='text' || field.readonly || !this.getField('calc').readonly) throw 'field flags'; this.calculate=false; field.value='1234'; this.calculate=true; this.calculateNow();",
            open: "",
            page_open: "",
            keystroke: Some("AFNumber_Keystroke(2,0,0,0,'$',true);"),
            validate: "if(Number(event.value)>5000) event.rc=false;",
            format: "AFNumber_Format(2,0,0,0,'$',true); this.getField('nested').value=util.printf('%s|%s|%.2f',util.printd('yyyy-mm-dd',util.scand('yyyy-mm-dd','2024-02-29')),event.value,Number(this.getField('input').value));",
            calculate: "AFSimple_Calculate('SUM',['input','input']);",
        });
        let saved = [
            run_standard_actions_and_save(native.as_ref(), &bytes, "native"),
            run_standard_actions_and_save(rust.as_ref(), &bytes, "rust"),
        ];
        for reader in [native.as_ref(), rust.as_ref()] {
            for bytes in &saved {
                let session = Session::open(reader, bytes, false);
                // Keep the numeric raw value distinct from its dollar-formatted
                // appearance; both engines must read either writer's result.
                session.assert_values("12.75", "25.5", "2024-02-29|$12.75|12.75");
                assert!(session.ink().iter().all(|&count| count > 10));
            }
        }
    });
}

fn cosmetic_fixture() -> Vec<u8> {
    let configure = js(r#"
        function check(ok,message) { if(!ok) throw message; }
        var field=this.getField('input');
        field.delay=true;
        field.borderStyle=border.d;
        field.lineWidth=2;
        field.rect=[30,250,190,220];
        field.display=display.noPrint;
        field.value='cosmetic';
        check(field.valueAsString==='' && field.borderStyle===border.s,'deferred properties');
        field.delay=false;
        check(field.borderStyle===border.d && field.lineWidth===2,'border mutation');
        check(field.rect.join(',')==='30,250,190,220','rect mutation');
        field.hidden=true;
        check(field.hidden && field.display===display.hidden,'hidden field');
        field.hidden=false;
        field.display=display.noPrint;
        field.print=true;
        check(field.print && field.display===display.visible,'print enabled');
        field.print=false;
        check(!field.print && !field.hidden && field.display===display.noPrint,'print disabled');
        field.alignment='center'; field.textFont=font.Cour; field.textSize=20;
        field.fillColor=color.red; field.strokeColor=color.green; field.rotation=90;
        check(field.alignment==='right' && field.textFont==='Helvetica' && field.textSize===12,'source font/alignment setters');
        check(color.equal(field.fillColor,color.yellow) && color.equal(field.strokeColor,color.blue) && field.rotation===0,'source color/rotation setters');
        var button=this.getField('button');
        check(button.type==='button' && button.buttonGetCaption()==='Launch' && button.buttonGetCaption(1)==='Pressed' && button.buttonGetCaption(2)==='Hover','button captions');
        check(button.buttonAlignX===1 && button.buttonAlignY===0 && button.buttonFitBounds===true && button.buttonPosition===position.iconTextV,'button layout');
        check(button.buttonScaleHow===true && button.buttonScaleWhen===1,'button scale');
        var icon=button.buttonGetIcon();
        check(icon.name==='' && this.icons===undefined,'initial icon');
        var readonly=false; try { icon.name='changed'; } catch(e) { readonly=true; }
        check(readonly && icon.name==='','readonly icon name');
        this.addIcon('sample',icon);
        check(this.getIcon('sample').name==='sample' && this.icons.length===1 && this.icons[0].name==='sample','document icons');
        var annots=this.getAnnots();
        check(annots.length===1 && annots[0].type==='Square' && annots[0].name==='note','annotation iteration');
        var annot=this.getAnnot(0,'note');
        annot.name='renamed-note'; annot.hidden=true;
        check(annot.hidden && annot.name==='renamed-note' && annot.type==='Square','annotation mutation');
        readonly=false; try { annot.type='Text'; } catch(e) { readonly=true; }
        check(readonly && annot.type==='Square','readonly annotation type');
        this.getField('status').value='configured';
    "#);
    let verify = js(r#"
        function check(ok,message) { if(!ok) throw message; }
        var field=this.getField('input');
        check(field.valueAsString==='cosmetic' && field.borderStyle===border.d && field.lineWidth===2,'saved field');
        check(field.rect.join(',')==='30,250,190,220' && field.display===display.noPrint && !field.hidden && !field.print,'saved geometry/display');
        check(field.textFont==='Helvetica' && field.textSize===12 && field.alignment==='right','saved typography');
        check(color.equal(field.fillColor,color.yellow) && color.equal(field.strokeColor,color.blue),'saved colors');
        var button=this.getField('button');
        check(button.buttonGetCaption()==='Launch' && button.buttonGetCaption(1)==='Pressed' && button.buttonGetCaption(2)==='Hover','saved captions');
        check(button.buttonFitBounds && button.buttonPosition===position.iconTextV && button.buttonScaleHow===true && button.buttonScaleWhen===1,'saved button layout');
        var annot=this.getAnnot(0,'renamed-note');
        check(annot.name==='renamed-note' && annot.hidden && annot.type==='Square' && this.getAnnots().length===1,'saved annotation');
        check(this.icons===undefined,'icons are runtime state');
        this.getField('status').value='verified';
    "#);
    let objects = [
        format!("<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R /Names << /JavaScript << /Names [(cosmetics) 9 0 R] >> >> /OpenAction {verify} >>"),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 300] /Resources << >> /Annots [6 0 R 7 0 R 8 0 R 10 0 R] >>".to_owned(),
        "<< /Fields [6 0 R 7 0 R 8 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 220 180 250] /FT /Tx /T (input) /V () /Q 2 /BS << /W 1 /S /S >> /MK << /BG [1 1 0] /BC [0 0 1] /R 0 >> >>".to_owned(),
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 160 180 190] /FT /Tx /T (status) /V () >>".to_owned(),
        "<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [20 100 180 130] /FT /Btn /Ff 65536 /T (button) /MK << /CA (Launch) /AC (Pressed) /RC (Hover) /TP 2 /IF << /SW /B /S /A /A [1 0] /FB true >> >> >>".to_owned(),
        configure,
        "<< /Type /Annot /Subtype /Square /P 3 0 R /F 4 /NM (note) /Rect [220 220 260 260] /C [1 0 0] >>".to_owned(),
    ];
    pdf_with_objects(&objects)
}

fn assert_cosmetic_state(session: &Session<'_>, status: &str) {
    assert_eq!(session.value(0), "cosmetic");
    assert_eq!(session.value(1), status);
    unsafe {
        let input = session.bindings.FPDFPage_GetAnnot(session.page, 0);
        assert!(!input.is_null());
        assert_eq!(session.bindings.FPDFAnnot_GetFlags(input), 0);
        let mut rect: FS_RECTF = std::mem::zeroed();
        assert_ne!(session.bindings.FPDFAnnot_GetRect(input, &mut rect), 0);
        assert_eq!(
            [rect.left, rect.top, rect.right, rect.bottom],
            [30., 250., 190., 220.]
        );
        session.bindings.FPDFPage_CloseAnnot(input);
        let square = session.bindings.FPDFPage_GetAnnot(session.page, 3);
        assert!(!square.is_null());
        assert_eq!(session.bindings.FPDFAnnot_GetFlags(square), 35);
        let length = session
            .bindings
            .FPDFAnnot_GetStringValue(square, "NM", ptr::null_mut(), 0);
        assert!(length >= 2 && length % 2 == 0);
        let mut name = vec![0u16; length as usize / 2];
        assert_eq!(
            session
                .bindings
                .FPDFAnnot_GetStringValue(square, "NM", name.as_mut_ptr(), length),
            length
        );
        assert_eq!(name.pop(), Some(0));
        assert_eq!(String::from_utf16(&name).unwrap(), "renamed-note");
        session.bindings.FPDFPage_CloseAnnot(square);
    }
    assert!(
        session.ink()[..2].iter().all(|&count| count > 10),
        "configured text appearances must draw"
    );
}

#[test]
fn javascript_field_cosmetics_annotations_icons_save_reload() {
    common::with_libraries(|native, rust| {
        let bytes = cosmetic_fixture();
        let mut saved = Vec::new();
        for (backend, bindings) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
            eprintln!("field cosmetics/annotations/icons: {backend}");
            let session = Session::open(bindings, &bytes, false);
            unsafe { bindings.FORM_DoDocumentJSAction(session.form()) };
            assert_cosmetic_state(&session, "configured");
            saved.push(session.save());
        }
        for (backend, reader) in [("native", native.as_ref()), ("rust", rust.as_ref())] {
            for (writer, bytes) in saved.iter().enumerate() {
                eprintln!("field cosmetics reload: reader={backend}, writer={writer}");
                let session = Session::open(reader, bytes, false);
                assert_cosmetic_state(&session, "configured");
                unsafe { reader.FORM_DoDocumentOpenAction(session.form()) };
                assert_cosmetic_state(&session, "verified");
            }
        }
    });
}

#[test]
fn javascript_host_can_detach_environment_during_callback() {
    common::with_libraries(|_native, rust| {
        // This exercises the safe Rust environment's documented invalidation
        // boundary; the native engine is not asked to destroy its active runtime.
        let bytes = fixture(true);
        {
            let session = Session::open(rust.as_ref(), &bytes, true);
            let form = session.form();
            unsafe { rust.FORM_DoDocumentJSAction(form) };
            assert_eq!(session.host.gotos.get(), 1);
            assert_eq!(session.host.goto_page.get(), 0);
            assert!(session.host.form.get().is_null());
            assert_eq!(session.host.releases.get(), 1);
            // Only document/page handles remain valid. Saving after the active
            // callback unwinds checks their lifetime without touching old |form|.
            let saved = session.save();
            let reloaded = Session::open(rust.as_ref(), &saved, false);
            assert_eq!(reloaded.value(0), "before");
        }
    });
}
