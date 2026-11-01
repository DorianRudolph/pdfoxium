//! Regenerated widget appearances survive saving and flattening with both
//! libraries. The safe SDK model is used until FORM event dispatch is ported.
mod common;

use pdfium_render::prelude::*;
use pdfoxium_core::fpdfapi::edit::cpdf_creator::CpdfCreator;
use pdfoxium_core::fpdfapi::parser::cpdf_document::CpdfDocument;
use pdfoxium_core::fpdfapi::parser::cpdf_parser::ParserError;
use pdfoxium_core::fpdfdoc::cpdf_bafontmap::CpdfBaFontMap;
use pdfoxium_core::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm;
use pdfoxium_core::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use pdfoxium_core::fpdfsdk::fpdf_fwlevent::*;
use pdfoxium_core::fpdfsdk::pwl::cpwl_combo_box::CpwlComboBox;
use pdfoxium_core::fpdfsdk::pwl::cpwl_edit::CpwlEdit;
use pdfoxium_core::fpdfsdk::pwl::cpwl_list_box::CpwlListBox;
use pdfoxium_core::fpdfsdk::pwl::cpwl_wnd::{
    CreateParams, Window, EDIT_AUTO_SCROLL, EDIT_UNDO, WINDOW_VISIBLE,
};
use pdfoxium_core::fpdfsdk::pwl::ipwl_fillernotify::{
    BeforeKeystrokeResult, CursorStyle, FillerNotify, PerWindowData,
};
use pdfoxium_core::fpdfsdk::pwl::pwl_draw::AggPwlDraw;
use pdfoxium_core::fxcrt::cfx_memorystream::CfxMemoryStream;
use pdfoxium_core::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use pdfoxium_core::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix};
use pdfoxium_core::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use pdfoxium_core::fxge::cfx_color::CfxColor;
use pdfoxium_core::fxge::dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat};
use pdfoxium_diff::FPDF_NO_NATIVETEXT;
use std::cell::RefCell;
use std::ffi::{c_int, c_ulong, c_void};
use std::rc::Rc;

fn fixture() -> Vec<u8> {
    let fields = [
        "/FT /Btn /T (check) /V /Yes /AS /Yes /MK << /CA (4) >>",
        "/FT /Btn /Ff 32768 /T (radio) /V /Yes /AS /Yes /MK << /CA (l) >>",
        "/FT /Btn /Ff 65536 /T (push) /MK << /CA (Press) /RC (Hover) /AC (Down) >>",
        "/FT /Tx /T (text) /V (First value)",
        "/FT /Ch /Ff 131072 /T (combo) /Opt [(Red) (Green) (Blue)] /V (Green)",
        "/FT /Ch /T (list) /Opt [(One) (Two) (Three)] /I [1] /V (Two)",
    ];
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /AcroForm 4 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Resources << >> /Annots [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] >>".to_owned(),
        "<< /Fields [6 0 R 7 0 R 8 0 R 9 0 R 10 0 R 11 0 R] /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 5 0 R >> >> >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_owned(),
    ];
    for (index, field) in fields.iter().enumerate() {
        let x = (index % 3) * 100 + 10;
        let y = 110 - (index / 3) * 100;
        objects.push(format!("<< /Type /Annot /Subtype /Widget /P 3 0 R /F 4 /Rect [{x} {y} {} {}] /BS << /W 1 /S /S >> {field} >>", x + 80, y + 70));
    }
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

fn regenerate(edited: bool) -> Vec<u8> {
    let document = Rc::new(RefCell::new(CpdfDocument::new()));
    assert_eq!(
        document
            .borrow_mut()
            .load_doc(Rc::new(CfxReadOnlyContainerStream::new(fixture())), b""),
        ParserError::Success
    );
    let form = CpdfInteractiveForm::new(&document);
    assert_eq!(form.count_fields(&[]), 6);
    for index in 0..6 {
        let field = form.get_field(index, &[]).unwrap();
        let widget = CpdfSdkWidget::new(&document, field.get_control(0).unwrap());
        assert!(widget.reset_appearance(None, false));
        if index < 2 {
            assert!(widget.is_checked());
        }
        if edited {
            match index {
                0 | 1 => {
                    assert!(widget.set_check(false));
                }
                3 => edit_text_window(&document, &widget),
                4 | 5 => edit_choice_window(&document, &widget, index == 4),
                _ => {}
            }
        }
        assert!(widget.reset_appearance(None, edited));
    }
    let mut output = CfxMemoryStream::new();
    assert!(CpdfCreator::new(document, &mut output).create(0, 17));
    output.get_span().to_vec()
}

fn window_params(document: &Rc<RefCell<CpdfDocument>>, widget: &CpdfSdkWidget) -> CreateParams {
    CreateParams {
        font_map: Some(Rc::new(CpdfBaFontMap::new(document, widget.dict(), b"N"))),
        rect: CfxFloatRect::new(0.0, 0.0, 80.0, 70.0),
        flags: WINDOW_VISIBLE | EDIT_UNDO | EDIT_AUTO_SCROLL,
        font_size: 12.0,
        text_color: CfxColor::make_gray(0.0),
        ..Default::default()
    }
}
fn edit_text_window(document: &Rc<RefCell<CpdfDocument>>, widget: &CpdfSdkWidget) {
    let edit = CpwlEdit::new(window_params(document, widget), None);
    edit.wnd().realize();
    edit.set_text(&widget.value());
    edit.set_focus();
    edit.set_selection(0, -1);
    assert!(edit.on_key_down(FWL_VKEY_Delete, 0));
    for word in "Changed text".encode_utf16() {
        assert!(edit.on_char(word, 0));
    }
    assert!(edit.on_char(b'!' as u16, 0));
    assert!(edit.undo());
    assert!(edit.redo());
    assert!(edit.undo());
    assert_eq!(
        edit.text(),
        "Changed text".chars().map(u32::from).collect::<Vec<_>>()
    );
    edit.kill_focus();
    // Draw the actual live control as well as its saved appearance. Hiding the
    // caret keeps this check about text rather than focus decoration.
    let mut bitmap = CfxDibitmap::create(80, 70, FxdibFormat::Bgra, 0).unwrap();
    {
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        driver.clear(0xffffffff);
        edit.draw_appearance(&mut AggPwlDraw::new(&mut driver), CfxMatrix::default());
    }
    assert!(bitmap
        .buffer()
        .chunks_exact(4)
        .any(|p| p[..3] != [255, 255, 255]));
    assert!(widget.set_value(&edit.text()));
    edit.wnd().destroy();
}
fn edit_choice_window(document: &Rc<RefCell<CpdfDocument>>, widget: &CpdfSdkWidget, combo: bool) {
    let mut params = window_params(document, widget);
    params.flags = WINDOW_VISIBLE;
    let host = Rc::new(ChoiceHost::default());
    let callbacks: Rc<dyn FillerNotify> = host.clone();
    params.filler_notify = Some(Rc::downgrade(&callbacks));
    let index = if combo {
        let window = CpwlComboBox::new(params, None);
        window.wnd().realize();
        for index in 0..widget.count_options() {
            window.add_string(&widget.option_label(index as i32));
        }
        assert!(window.set_select(widget.selected_index(0)));
        // ComboBox forwards the selection callback's `exit` flag here; false
        // is the ordinary completed selection, unlike Edit's handled flag.
        assert!(!window.on_char(b'R' as u16, 0));
        assert_eq!(
            window.text(),
            "Red".chars().map(u32::from).collect::<Vec<_>>()
        );
        let selected = window.selected_index();
        window.wnd().destroy();
        selected
    } else {
        let window = CpwlListBox::new(params, None);
        window.wnd().realize();
        for index in 0..widget.count_options() {
            window.add_string(&widget.option_label(index as i32));
        }
        window.select(widget.selected_index(0));
        assert!(window.on_key_down(FWL_VKEY_Home, 0));
        let selected = window.cur_sel();
        window.wnd().destroy();
        selected
    };
    if combo {
        assert_eq!(*host.events.borrow(), ["pre", "post"]);
    }
    assert_eq!(index, 0);
    assert!(widget.set_option_selection(index));
}

#[derive(Default)]
struct ChoiceHost {
    events: RefCell<Vec<&'static str>>,
}
impl FillerNotify for ChoiceHost {
    fn invalidate_rect(&self, _: Option<&dyn PerWindowData>, _: CfxFloatRect) {}
    fn output_selected_rect(&self, _: Option<&dyn PerWindowData>, _: CfxFloatRect) {}
    fn is_selection_implemented(&self) -> bool {
        false
    }
    fn set_cursor(&self, _: CursorStyle) {}
    fn query_where_popup(&self, _: Option<&dyn PerWindowData>, min: f32, _: f32) -> (bool, f32) {
        (true, min)
    }
    fn on_before_key_stroke(
        &self,
        _: Option<&dyn PerWindowData>,
        _: &mut Vec<u32>,
        _: &[u32],
        _: i32,
        _: i32,
        _: bool,
        _: u32,
    ) -> BeforeKeystrokeResult {
        BeforeKeystrokeResult {
            rc: true,
            exit: false,
        }
    }
    fn on_popup_pre_open(&self, _: Option<&dyn PerWindowData>, _: u32) -> bool {
        self.events.borrow_mut().push("pre");
        false
    }
    fn on_popup_post_open(&self, _: Option<&dyn PerWindowData>, _: u32) -> bool {
        self.events.borrow_mut().push("post");
        false
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
fn flatten(b: &dyn PdfiumLibraryBindings, bytes: &[u8]) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_LoadMemDocument64(bytes, None);
        assert!(!doc.is_null());
        let page = b.FPDF_LoadPage(doc, 0);
        assert!(!page.is_null());
        assert_eq!(b.FPDFPage_GetAnnotCount(page), 6);
        assert_eq!(b.FPDFPage_Flatten(page, 0), 1);
        assert_eq!(b.FPDFPage_GetAnnotCount(page), 0);
        let mut sink = Sink {
            interface: FPDF_FILEWRITE {
                version: 1,
                WriteBlock: Some(append),
            },
            bytes: Vec::new(),
        };
        assert_eq!(b.FPDF_SaveAsCopy(doc, &mut sink.interface, 0), 1);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        sink.bytes
    }
}
fn render(b: &dyn PdfiumLibraryBindings, bytes: &[u8]) -> Vec<u8> {
    unsafe {
        let doc = b.FPDF_LoadMemDocument64(bytes, None);
        assert!(!doc.is_null());
        let page = b.FPDF_LoadPage(doc, 0);
        assert!(!page.is_null());
        let bitmap = b.FPDFBitmap_Create(300, 200, 1);
        assert!(!bitmap.is_null());
        assert_eq!(b.FPDFBitmap_FillRect(bitmap, 0, 0, 300, 200, 0xffffffff), 1);
        b.FPDF_RenderPageBitmap(bitmap, page, 0, 0, 300, 200, 0, FPDF_NO_NATIVETEXT);
        let pixels = b.FPDFBitmap_GetBuffer_as_vec(bitmap);
        b.FPDFBitmap_Destroy(bitmap);
        b.FPDF_ClosePage(page);
        b.FPDF_CloseDocument(doc);
        pixels
    }
}
#[test]
fn regenerated_widget_appearances_save_reload_and_flatten_in_both_libraries() {
    common::with_libraries(|native, rust| {
        let originals = [regenerate(false), regenerate(true)];
        let flattened: Vec<_> = originals
            .iter()
            .map(|pdf| [flatten(native.as_ref(), pdf), flatten(rust.as_ref(), pdf)])
            .collect();
        for reader in [native.as_ref(), rust.as_ref()] {
            let before = render(reader, &flattened[0][0]);
            let after = render(reader, &flattened[1][0]);
            assert_ne!(before, after, "value changes must change appearance");
            for output in &flattened {
                let n = render(reader, &output[0]);
                let r = render(reader, &output[1]);
                assert_eq!(
                    n, r,
                    "flatteners must preserve the same generated appearance"
                );
                for row in 0..2 {
                    for col in 0..3 {
                        // Inspect interiors rather than counting borders as text.
                        let count = (row * 100 + 23..row * 100 + 87)
                            .flat_map(|y| {
                                (col * 100 + 13..col * 100 + 87).map(move |x| (y * 300 + x) * 4)
                            })
                            .filter(|&p| n[p..p + 3] != [255, 255, 255])
                            .count();
                        if row == 1 || col == 2 {
                            assert!(count > 5, "empty widget interior at {row},{col}");
                        }
                    }
                }
            }
        }
    });
}
