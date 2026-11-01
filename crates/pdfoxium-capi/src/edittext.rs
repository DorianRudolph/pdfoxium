// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Text/font C ABI from `fpdfsdk/fpdf_edittext.cpp`. Callers supply live opaque
//! handles, valid buffers and the ownership/lifetimes documented by PDFium.

use super::*;
use pdfoxium_core::fpdfapi::font::cpdf_font::CpdfFont;
use pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use pdfoxium_core::fpdfapi::page::cpdf_textstate::set_text_rendering_mode_from_int;
use pdfoxium_core::fpdfsdk::fpdf_edittext as sdk;
use pdfoxium_core::fxge::cfx_path::CfxPath;
use std::collections::HashMap;

pub(super) struct FontHandle {
    font: CpdfFont,
    // Caller-owned handles also retain the document's weak resource targets.
    _document: Option<Rc<RefCell<CpdfDocument>>>,
    glyphs: HashMap<usize, Box<GlyphPathHandle>>,
}
struct GlyphPathHandle {
    path: Rc<CfxPath>,
    segments: HashMap<usize, Box<super::editpath::PathSegmentHandle>>,
}
impl FontHandle {
    fn new(font: CpdfFont, document: Option<Rc<RefCell<CpdfDocument>>>) -> Self {
        Self {
            font,
            _document: document,
            glyphs: HashMap::new(),
        }
    }
}
unsafe fn with_text<R>(handle: *mut c_void, f: impl FnOnce(&mut CpdfTextObject) -> R) -> Option<R> {
    unsafe {
        with_page_object(handle, |object| {
            object.as_any_mut().downcast_mut::<CpdfTextObject>().map(f)
        })
    }
    .flatten()
}
fn owned_font(font: Option<CpdfFont>, document: Rc<RefCell<CpdfDocument>>) -> *mut c_void {
    font.map_or(std::ptr::null_mut(), |font| {
        Box::into_raw(Box::new(FontHandle::new(font, Some(document)))).cast()
    })
}

#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_NewTextObj(
    document: *mut c_void,
    font: *const c_char,
    font_size: f32,
) -> *mut c_void {
    if document.is_null() || font.is_null() {
        return std::ptr::null_mut();
    }
    let document = unsafe { &*document.cast::<DocumentHandle>() };
    let name = unsafe { CStr::from_ptr(font) }.to_bytes();
    let font = CpdfFont::get_stock_font(&document.document.borrow(), name);
    font.map_or(std::ptr::null_mut(), |font| {
        new_owned_page_object(Box::new(sdk::new_text_object(font, font_size)))
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_SetText(text_object: *mut c_void, text: *const u16) -> c_int {
    if text.is_null() {
        return 0;
    }
    let mut length = 0;
    // SAFETY: caller supplies a readable NUL-terminated UTF-16 string.
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    let text = unsafe { std::slice::from_raw_parts(text, length) };
    unsafe { with_text(text_object, |object| sdk::set_text(object, text)) }.unwrap_or(false)
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_SetCharcodes(
    text_object: *mut c_void,
    charcodes: *const u32,
    count: usize,
) -> c_int {
    if charcodes.is_null() && count != 0 {
        return 0;
    }
    let codes = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(charcodes, count) }
    };
    unsafe { with_text(text_object, |object| sdk::set_charcodes(object, codes)) }.unwrap_or(false)
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_SetPositions(
    text_object: *mut c_void,
    positions: *const f32,
    count: usize,
) -> c_int {
    if positions.is_null() && count != 0 {
        return 0;
    }
    let positions = if count == 0 {
        &[]
    } else {
        unsafe { std::slice::from_raw_parts(positions, count) }
    };
    unsafe {
        with_text(text_object, |object| {
            if !object.set_absolute_positions(positions) {
                return false;
            }
            object.base_mut().set_dirty(true);
            true
        })
    }
    .unwrap_or(false) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_LoadFont(
    document: *mut c_void,
    data: *const u8,
    size: u32,
    font_type: c_int,
    cid: c_int,
) -> *mut c_void {
    if document.is_null() || data.is_null() || size == 0 {
        return std::ptr::null_mut();
    }
    let document = Rc::clone(&unsafe { &*document.cast::<DocumentHandle>() }.document);
    let bytes = unsafe { std::slice::from_raw_parts(data, size as usize) };
    let font = sdk::load_font(&document.borrow(), bytes, font_type, cid != 0);
    owned_font(font, document)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_LoadStandardFont(
    document: *mut c_void,
    font: *const c_char,
) -> *mut c_void {
    if document.is_null() || font.is_null() {
        return std::ptr::null_mut();
    }
    let document = Rc::clone(&unsafe { &*document.cast::<DocumentHandle>() }.document);
    let name = unsafe { CStr::from_ptr(font) }.to_bytes();
    let font = CpdfFont::get_stock_font(&document.borrow(), name);
    owned_font(font, document)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_LoadCidType2Font(
    document: *mut c_void,
    font_data: *const u8,
    font_data_size: u32,
    to_unicode_cmap: *const c_char,
    cid_to_gid_map_data: *const u8,
    cid_to_gid_map_data_size: u32,
) -> *mut c_void {
    if document.is_null()
        || font_data.is_null()
        || font_data_size == 0
        || to_unicode_cmap.is_null()
        || cid_to_gid_map_data.is_null()
        || cid_to_gid_map_data_size == 0
    {
        return std::ptr::null_mut();
    }
    let document = Rc::clone(&unsafe { &*document.cast::<DocumentHandle>() }.document);
    let data = unsafe { std::slice::from_raw_parts(font_data, font_data_size as usize) };
    let cmap = unsafe { CStr::from_ptr(to_unicode_cmap) }.to_bytes();
    let mapping = unsafe {
        std::slice::from_raw_parts(cid_to_gid_map_data, cid_to_gid_map_data_size as usize)
    };
    let font = sdk::load_cid_type2_font(&document.borrow(), data, cmap, mapping);
    owned_font(font, document)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_Close(font: *mut c_void) {
    if !font.is_null() {
        drop(unsafe { Box::from_raw(font.cast::<FontHandle>()) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_CreateTextObj(
    document: *mut c_void,
    font: *mut c_void,
    font_size: f32,
) -> *mut c_void {
    if document.is_null() || font.is_null() {
        return std::ptr::null_mut();
    }
    let document = unsafe { &*document.cast::<DocumentHandle>() };
    let font = unsafe { &*font.cast::<FontHandle>() };
    sdk::create_text_object(&document.document.borrow(), &font.font, font_size)
        .map_or(std::ptr::null_mut(), |object| {
            new_owned_page_object(Box::new(object))
        })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_GetFontSize(text: *mut c_void, size: *mut f32) -> c_int {
    if size.is_null() {
        return 0;
    }
    unsafe {
        with_text(text, |object| {
            *size = object.font_size();
        })
    }
    .is_some() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_SetFontSize(text: *mut c_void, size: f32) -> c_int {
    if size < 0.0 {
        return 0;
    }
    unsafe {
        with_text(text, |object| {
            object.base_mut().mutable_text_state().set_font_size(size);
            object.base_mut().set_dirty(true);
        })
    }
    .is_some() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_GetTextRenderMode(text: *mut c_void) -> c_int {
    unsafe { with_text(text, |object| object.text_render_mode() as c_int) }.unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_SetTextRenderMode(
    text: *mut c_void,
    render_mode: c_int,
) -> c_int {
    let Some(mode) = set_text_rendering_mode_from_int(render_mode) else {
        return 0;
    };
    unsafe { with_text(text, |object| object.set_text_render_mode(mode)) }.is_some() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_GetFont(text: *mut c_void) -> *mut c_void {
    let Some(font) = (unsafe { with_text(text, |object| object.font()) }).flatten() else {
        return std::ptr::null_mut();
    };
    // Borrow the alias cache only after releasing the engine object borrow.
    let handle = unsafe { &mut *text.cast::<PageObjectHandle>() };
    let font_handle = handle
        .font_handle
        .get_or_insert_with(|| Box::new(FontHandle::new(font, None)));
    (&mut **font_handle as *mut FontHandle).cast()
}
unsafe fn copy_bytes(source: &[u8], buffer: *mut u8, length: usize) {
    if !buffer.is_null() && length >= source.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(source.as_ptr(), buffer, source.len());
        }
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetBaseFontName(
    font: *mut c_void,
    buffer: *mut c_char,
    length: usize,
) -> usize {
    if font.is_null() {
        return 0;
    }
    let font = unsafe { &*font.cast::<FontHandle>() };
    let mut name = font.font.base_font_name().to_vec();
    name.push(0);
    unsafe {
        copy_bytes(&name, buffer.cast(), length);
    }
    name.len()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetFamilyName(
    font: *mut c_void,
    buffer: *mut c_char,
    length: usize,
) -> usize {
    if font.is_null() {
        return 0;
    }
    let font = unsafe { &*font.cast::<FontHandle>() };
    let Some(face) = font.font.font() else {
        return 0;
    };
    let mut name = face.family_name().into_bytes();
    name.push(0);
    unsafe {
        copy_bytes(&name, buffer.cast(), length);
    }
    name.len()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetFontData(
    font: *mut c_void,
    buffer: *mut u8,
    buflen: usize,
    out_buflen: *mut usize,
) -> c_int {
    if font.is_null() || out_buflen.is_null() {
        return 0;
    }
    let font = unsafe { &*font.cast::<FontHandle>() };
    let Some(face) = font.font.font() else {
        return 0;
    };
    let data = face.font_span();
    unsafe {
        copy_bytes(data, buffer, buflen);
        *out_buflen = data.len();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetIsEmbedded(font: *mut c_void) -> c_int {
    if font.is_null() {
        return -1;
    }
    unsafe { &*font.cast::<FontHandle>() }.font.is_embedded() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetFlags(font: *mut c_void) -> c_int {
    if font.is_null() {
        return -1;
    }
    (unsafe { &*font.cast::<FontHandle>() }.font.font_flags() & 0x7ffff) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetWeight(font: *mut c_void) -> c_int {
    if font.is_null() {
        return -1;
    }
    unsafe { &*font.cast::<FontHandle>() }
        .font
        .font_weight()
        .unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetItalicAngle(font: *mut c_void, angle: *mut c_int) -> c_int {
    if font.is_null() || angle.is_null() {
        return 0;
    }
    unsafe {
        *angle = (&*font.cast::<FontHandle>()).font.italic_angle();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetAscent(
    font: *mut c_void,
    font_size: f32,
    ascent: *mut f32,
) -> c_int {
    if font.is_null() || ascent.is_null() {
        return 0;
    }
    unsafe {
        *ascent = (&*font.cast::<FontHandle>()).font.type_ascent() as f32 * font_size / 1000.0;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetDescent(
    font: *mut c_void,
    font_size: f32,
    descent: *mut f32,
) -> c_int {
    if font.is_null() || descent.is_null() {
        return 0;
    }
    unsafe {
        *descent = (&*font.cast::<FontHandle>()).font.type_descent() as f32 * font_size / 1000.0;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetGlyphWidth(
    font: *mut c_void,
    glyph: u32,
    font_size: f32,
    width: *mut f32,
) -> c_int {
    if font.is_null() || width.is_null() {
        return 0;
    }
    unsafe {
        *width = sdk::glyph_width(&(&*font.cast::<FontHandle>()).font, glyph, font_size);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFFont_GetGlyphPath(
    font: *mut c_void,
    glyph: u32,
    font_size: f32,
) -> *mut c_void {
    if font.is_null() {
        return std::ptr::null_mut();
    }
    let font = unsafe { &mut *font.cast::<FontHandle>() };
    let Some(path) = sdk::glyph_path(&font.font, glyph, font_size) else {
        return std::ptr::null_mut();
    };
    let identity = Rc::as_ptr(&path) as usize;
    let glyph = font.glyphs.entry(identity).or_insert_with(|| {
        Box::new(GlyphPathHandle {
            path,
            segments: HashMap::new(),
        })
    });
    (&mut **glyph as *mut GlyphPathHandle).cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFGlyphPath_CountGlyphSegments(glyphpath: *mut c_void) -> c_int {
    if glyphpath.is_null() {
        return -1;
    }
    unsafe { &*glyphpath.cast::<GlyphPathHandle>() }
        .path
        .points()
        .len() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFGlyphPath_GetGlyphPathSegment(
    glyphpath: *mut c_void,
    index: c_int,
) -> *mut c_void {
    if glyphpath.is_null() || index < 0 {
        return std::ptr::null_mut();
    }
    let glyph = unsafe { &mut *glyphpath.cast::<GlyphPathHandle>() };
    let Some(&point) = glyph.path.points().get(index as usize) else {
        return std::ptr::null_mut();
    };
    let segment = glyph
        .segments
        .entry(index as usize)
        .or_insert_with(|| Box::new(super::editpath::PathSegmentHandle::Glyph(point)));
    (&mut **segment as *mut super::editpath::PathSegmentHandle).cast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_font_handles_and_glyph_segments_follow_public_lifetimes() {
        unsafe {
            FPDF_InitLibrary();
            let document = FPDF_CreateNewDocument();
            let font = FPDFText_LoadStandardFont(document, c"Helvetica".as_ptr());
            assert!(!font.is_null());
            let text = FPDFPageObj_CreateTextObj(document, font, 12.0);
            assert!(!text.is_null());
            assert_eq!(FPDFText_SetText(text, [65u16, 66, 67, 0].as_ptr()), 1);
            assert_eq!(FPDFText_SetPositions(text, [20.0f32, 40.0].as_ptr(), 2), 1);
            let borrowed = FPDFTextObj_GetFont(text);
            assert!(!borrowed.is_null());
            assert_eq!(FPDFTextObj_GetFont(text), borrowed);
            let mut size = 0.0;
            assert_eq!(FPDFTextObj_GetFontSize(text, &mut size), 1);
            assert_eq!(size, 12.0);
            assert_eq!(FPDFTextObj_SetFontSize(text, -1.0), 0);
            assert_eq!(FPDFTextObj_SetTextRenderMode(text, 8), 0);
            assert_eq!(FPDFTextObj_SetTextRenderMode(text, 1), 1);
            assert_eq!(FPDFTextObj_GetTextRenderMode(text), 1);
            let mut name = [0i8; 32];
            assert_eq!(
                FPDFFont_GetBaseFontName(borrowed, name.as_mut_ptr(), name.len()),
                10
            );
            assert_eq!(CStr::from_ptr(name.as_ptr()).to_bytes(), b"Helvetica");
            let glyph = FPDFFont_GetGlyphPath(borrowed, u32::from(b'A'), 12.0);
            assert!(!glyph.is_null());
            assert!(FPDFGlyphPath_CountGlyphSegments(glyph) > 0);
            assert!(FPDFGlyphPath_GetGlyphPathSegment(glyph, -1).is_null());
            let segment = FPDFGlyphPath_GetGlyphPathSegment(glyph, 0);
            assert!(!segment.is_null());
            let (mut x, mut y) = (0.0, 0.0);
            assert_eq!(
                super::super::editpath::FPDFPathSegment_GetPoint(segment, &mut x, &mut y),
                1
            );
            assert!(x.is_finite() && y.is_finite());
            FPDFFont_Close(font);
            // Closing the caller's font does not release the text object's font.
            assert!(FPDFFont_GetBaseFontName(borrowed, std::ptr::null_mut(), 0) > 0);
            FPDFPageObj_Destroy(text);
            FPDF_CloseDocument(document);
            FPDF_DestroyLibrary();
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDFTextObj_GetRenderedBitmap(
    document: *mut c_void,
    page: *mut c_void,
    text_object: *mut c_void,
    scale: f32,
) -> *mut c_void {
    if document.is_null() {
        return std::ptr::null_mut();
    }
    let document = unsafe { &*document.cast::<DocumentHandle>() };
    // Borrow the page field separately from its alias registry, since text may
    // itself be attached to this page and resolved through with_page_object.
    let page = if page.is_null() {
        None
    } else {
        let owner = unsafe { &(*page.cast::<PageHandle>())._document };
        if !Rc::ptr_eq(owner, &document.document) {
            return std::ptr::null_mut();
        }
        unsafe { &*page.cast::<PageHandle>() }.pdf_page()
    };
    // Engine access is read-only, including when this object belongs to the
    // optional page supplying resources. Shared borrows can safely overlap.
    let bitmap = unsafe {
        with_page_object_ref(text_object, |object| {
            let text = object.as_any().downcast_ref::<CpdfTextObject>()?;
            sdk::rendered_bitmap(&document.document.borrow(), page, text, scale)
        })
    }
    .flatten();
    bitmap.map_or(std::ptr::null_mut(), |bitmap| {
        Box::into_raw(Box::new(BitmapHandle {
            storage: Rc::new(RefCell::new(BitmapStorage::Owned(bitmap))),
        }))
        .cast()
    })
}
