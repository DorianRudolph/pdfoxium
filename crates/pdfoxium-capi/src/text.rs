// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Text/search/link ABI. Immutable extraction snapshots are retained by searches
//! and links; page-object aliases are resolved only while the original page lives.
use super::*;
use pdfoxium_core::{
    fpdftext::{
        cpdf_linkextract::CpdfLinkExtract,
        cpdf_textpage::{CharInfo, CharType, CpdfTextPage},
        cpdf_textpagefind::{CpdfTextPageFind, Options},
    },
    fxcrt::fx_coordinates::CfxSizeF,
};
use std::rc::Weak;
struct TextPageHandle {
    text: Rc<CpdfTextPage>,
    rects: RefCell<Vec<CfxFloatRect>>,
    original: NonNull<PageHandle>,
    owner: Weak<pdfoxium_core::fpdfsdk::cpdfsdk_pageview::SdkPage>,
}
unsafe fn text_page<'a>(page: *mut c_void) -> Option<&'a TextPageHandle> {
    unsafe { page.cast::<TextPageHandle>().as_ref() }
}
unsafe fn char_info<'a>(
    page: *mut c_void,
    index: c_int,
) -> Option<(&'a CpdfTextPage, &'a CharInfo)> {
    let page = unsafe { text_page(page) }?;
    let index = usize::try_from(index).ok()?;
    (index < page.text.size()).then(|| (page.text.as_ref(), page.text.get_char_info(index)))
}
unsafe fn copy_rect(
    rect: CfxFloatRect,
    left: *mut f64,
    top: *mut f64,
    right: *mut f64,
    bottom: *mut f64,
) -> bool {
    if left.is_null() || top.is_null() || right.is_null() || bottom.is_null() {
        return false;
    }
    unsafe {
        *left = rect.left.into();
        *top = rect.top.into();
        *right = rect.right.into();
        *bottom = rect.bottom.into();
    }
    true
}
fn words(text: &[u32], terminated: bool) -> Vec<u16> {
    pdfoxium_core::fxcrt::fx_string::utf16_encode(text)
        .into_iter()
        .chain(terminated.then_some(0))
        .collect()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_LoadPage(page: *mut c_void) -> *mut c_void {
    let Some(original) = NonNull::new(page.cast::<PageHandle>()) else {
        return std::ptr::null_mut();
    };
    let page = unsafe { original.as_ref() };
    let Some(engine_page) = page.pdf_page() else {
        return std::ptr::null_mut();
    };
    let rtl = pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
        &page._document.borrow(),
    )
    .is_direction_r2l();
    Box::into_raw(Box::new(TextPageHandle {
        text: Rc::new(CpdfTextPage::new(engine_page, rtl)),
        rects: Default::default(),
        original,
        owner: Rc::downgrade(&page.sdk_page),
    }))
    .cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_ClosePage(page: *mut c_void) {
    if !page.is_null() {
        drop(unsafe { Box::from_raw(page.cast::<TextPageHandle>()) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_CountChars(page: *mut c_void) -> c_int {
    unsafe { text_page(page) }.map_or(-1, |page| page.text.count_chars())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetUnicode(page: *mut c_void, index: c_int) -> u32 {
    unsafe { char_info(page, index) }.map_or(0, |(_, info)| info.unicode())
}
macro_rules! char_kind {
    ($name:ident,$kind:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(page: *mut c_void, index: c_int) -> c_int {
            unsafe { char_info(page, index) }.map_or(-1, |(_, info)| {
                c_int::from(info.char_type() == CharType::$kind)
            })
        }
    };
}
char_kind!(FPDFText_IsGenerated, Generated);
char_kind!(FPDFText_IsHyphen, Hyphen);
char_kind!(FPDFText_HasUnicodeMapError, NotUnicode);
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetFontSize(page: *mut c_void, index: c_int) -> f64 {
    unsafe { char_info(page, index) }.map_or(0., |(page, _)| {
        page.get_char_font_size(index as usize).into()
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetFontInfo(
    page: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    length: c_ulong,
    flags: *mut c_int,
) -> c_ulong {
    let Some(font) = (unsafe { char_info(page, index) })
        .and_then(|(page, _)| page.get_char_font(index as usize))
    else {
        return 0;
    };
    if !flags.is_null() {
        unsafe {
            *flags = font.font_flags() as c_int;
        }
    }
    unsafe { doc::copy_terminated(font.base_font_name(), buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetFontWeight(page: *mut c_void, index: c_int) -> c_int {
    unsafe { char_info(page, index) }
        .and_then(|(page, _)| page.get_char_font(index as usize))
        .and_then(|font| font.font_weight())
        .unwrap_or(-1)
}
unsafe fn color(
    page: *mut c_void,
    index: c_int,
    red: *mut u32,
    green: *mut u32,
    blue: *mut u32,
    alpha: *mut u32,
    stroke: bool,
) -> c_int {
    if red.is_null() || green.is_null() || blue.is_null() || alpha.is_null() {
        return 0;
    }
    let Some((page, info)) = (unsafe { char_info(page, index) }) else {
        return 0;
    };
    let Some(object) = info.text_object().and_then(|id| page.text_object_info(id)) else {
        return 0;
    };
    let color = if stroke {
        object.stroke_color
    } else {
        object.fill_color
    };
    unsafe {
        *red = color[0];
        *green = color[1];
        *blue = color[2];
        *alpha = color[3];
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetFillColor(
    page: *mut c_void,
    index: c_int,
    red: *mut u32,
    green: *mut u32,
    blue: *mut u32,
    alpha: *mut u32,
) -> c_int {
    unsafe { color(page, index, red, green, blue, alpha, false) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetStrokeColor(
    page: *mut c_void,
    index: c_int,
    red: *mut u32,
    green: *mut u32,
    blue: *mut u32,
    alpha: *mut u32,
) -> c_int {
    unsafe { color(page, index, red, green, blue, alpha, true) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetCharAngle(page: *mut c_void, index: c_int) -> f32 {
    unsafe { char_info(page, index) }.map_or(-1., |(_, info)| {
        let matrix = info.matrix();
        let angle = matrix.c.atan2(matrix.a);
        if angle < 0. {
            angle + std::f32::consts::TAU
        } else {
            angle
        }
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetCharBox(
    page: *mut c_void,
    index: c_int,
    left: *mut f64,
    right: *mut f64,
    bottom: *mut f64,
    top: *mut f64,
) -> c_int {
    unsafe { char_info(page, index) }
        .is_some_and(|(_, info)| unsafe { copy_rect(info.char_box(), left, top, right, bottom) })
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetLooseCharBox(
    page: *mut c_void,
    index: c_int,
    rect: *mut FS_RECTF,
) -> c_int {
    let Some((page, _)) = (unsafe { char_info(page, index) }) else {
        return 0;
    };
    let Some(rect) = (unsafe { rect.as_mut() }) else {
        return 0;
    };
    let bounds = page.get_char_loose_bounds(index as usize);
    *rect = FS_RECTF {
        left: bounds.left,
        right: bounds.right,
        top: bounds.top,
        bottom: bounds.bottom,
    };
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetMatrix(
    page: *mut c_void,
    index: c_int,
    matrix: *mut FS_MATRIX,
) -> c_int {
    let Some((_, info)) = (unsafe { char_info(page, index) }) else {
        return 0;
    };
    let Some(output) = (unsafe { matrix.as_mut() }) else {
        return 0;
    };
    let matrix = info.matrix();
    *output = FS_MATRIX {
        a: matrix.a,
        b: matrix.b,
        c: matrix.c,
        d: matrix.d,
        e: matrix.e,
        f: matrix.f,
    };
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetCharOrigin(
    page: *mut c_void,
    index: c_int,
    x: *mut f64,
    y: *mut f64,
) -> c_int {
    if x.is_null() || y.is_null() {
        return 0;
    }
    let Some((_, info)) = (unsafe { char_info(page, index) }) else {
        return 0;
    };
    unsafe {
        *x = info.origin().x.into();
        *y = info.origin().y.into();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetCharIndexAtPos(
    page: *mut c_void,
    x: f64,
    y: f64,
    x_tolerance: f64,
    y_tolerance: f64,
) -> c_int {
    unsafe { text_page(page) }.map_or(-3, |page| {
        page.text.get_index_at_pos(
            CfxPointF::new(x as f32, y as f32),
            CfxSizeF::new(x_tolerance as f32, y_tolerance as f32),
        )
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetText(
    page: *mut c_void,
    start: c_int,
    count: c_int,
    result: *mut u16,
) -> c_int {
    let Some(page) = (unsafe { text_page(page) }) else {
        return 0;
    };
    if start < 0 || count < 0 || result.is_null() {
        return 0;
    }
    let available = page.text.count_chars().saturating_sub(start);
    if available <= 0 {
        return 0;
    }
    let count = count.min(available) as usize;
    // GetText deliberately uses UCS-2 (WideString::ToUCS2LE upstream), unlike
    // GetBoundedText's UTF-16. Skip non-BMP scalars rather than expanding them
    // and truncating unrelated trailing text to meet the count+1 contract.
    // Already-stored surrogate units pass through, including partial ranges.
    let output: Vec<u16> = page
        .text
        .get_page_text(start, count as i32)
        .into_iter()
        .filter_map(|unit| u16::try_from(unit).ok())
        // Also retain the hard output bound for malformed character maps.
        .take(count)
        .chain(Some(0))
        .collect();
    unsafe {
        std::ptr::copy_nonoverlapping(output.as_ptr(), result, output.len());
    }
    output.len() as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_CountRects(
    page: *mut c_void,
    start: c_int,
    count: c_int,
) -> c_int {
    let Some(page) = (unsafe { text_page(page) }) else {
        return 0;
    };
    if start < 0 {
        return -1;
    }
    let rects = page.text.get_rect_array(start, count);
    let count = rects.len().min(c_int::MAX as usize) as c_int;
    *page.rects.borrow_mut() = rects;
    count
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetRect(
    page: *mut c_void,
    index: c_int,
    left: *mut f64,
    top: *mut f64,
    right: *mut f64,
    bottom: *mut f64,
) -> c_int {
    let Some(page) = (unsafe { text_page(page) }) else {
        return 0;
    };
    let rect = usize::try_from(index)
        .ok()
        .and_then(|index| page.rects.borrow().get(index).copied());
    (unsafe { copy_rect(rect.unwrap_or_default(), left, top, right, bottom) } && rect.is_some())
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetBoundedText(
    page: *mut c_void,
    left: f64,
    top: f64,
    right: f64,
    bottom: f64,
    buffer: *mut u16,
    length: c_int,
) -> c_int {
    let Some(page) = (unsafe { text_page(page) }) else {
        return 0;
    };
    let text = page.text.get_text_by_rect(CfxFloatRect::new(
        left as f32,
        bottom as f32,
        right as f32,
        top as f32,
    ));
    if buffer.is_null() || length <= 0 {
        return text.len().min(c_int::MAX as usize) as c_int;
    }
    let output = words(&text, true);
    let count = output.len().min(length as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(output.as_ptr(), buffer, count);
    }
    count as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetCharIndexFromTextIndex(
    page: *mut c_void,
    index: c_int,
) -> c_int {
    unsafe { text_page(page) }.map_or(-1, |page| page.text.char_index_from_text_index(index))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetTextIndexFromCharIndex(
    page: *mut c_void,
    index: c_int,
) -> c_int {
    unsafe { text_page(page) }.map_or(-1, |page| page.text.text_index_from_char_index(index))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_FindStart(
    page: *mut c_void,
    query: *const u16,
    flags: c_ulong,
    start: c_int,
) -> *mut c_void {
    if query.is_null() {
        return std::ptr::null_mut();
    }
    let Some(page) = (unsafe { text_page(page) }) else {
        return std::ptr::null_mut();
    };
    let options = Options {
        match_case: flags & 1 != 0,
        match_whole_word: flags & 2 != 0,
        consecutive: flags & 4 != 0,
    };
    Box::into_raw(Box::new(CpdfTextPageFind::new(
        page.text.clone(),
        &unsafe { doc::wide_string(query) },
        options,
        usize::try_from(start).ok(),
    )))
    .cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_FindNext(handle: *mut c_void) -> c_int {
    unsafe { handle.cast::<CpdfTextPageFind>().as_mut() }.is_some_and(CpdfTextPageFind::find_next)
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_FindPrev(handle: *mut c_void) -> c_int {
    unsafe { handle.cast::<CpdfTextPageFind>().as_mut() }.is_some_and(CpdfTextPageFind::find_prev)
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetSchResultIndex(handle: *mut c_void) -> c_int {
    unsafe { handle.cast::<CpdfTextPageFind>().as_ref() }.map_or(0, CpdfTextPageFind::get_cur_order)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetSchCount(handle: *mut c_void) -> c_int {
    unsafe { handle.cast::<CpdfTextPageFind>().as_ref() }
        .map_or(0, CpdfTextPageFind::get_matched_count)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_FindClose(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<CpdfTextPageFind>()) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_LoadWebLinks(page: *mut c_void) -> *mut c_void {
    let Some(page) = (unsafe { text_page(page) }) else {
        return std::ptr::null_mut();
    };
    let mut links = CpdfLinkExtract::new(page.text.clone());
    links.extract_links();
    Box::into_raw(Box::new(links)).cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_CountWebLinks(handle: *mut c_void) -> c_int {
    unsafe { handle.cast::<CpdfLinkExtract>().as_ref() }.map_or(0, |links| {
        links.count_links().min(c_int::MAX as usize) as c_int
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetURL(
    handle: *mut c_void,
    index: c_int,
    buffer: *mut u16,
    length: c_int,
) -> c_int {
    let text = unsafe { handle.cast::<CpdfLinkExtract>().as_ref() }
        .and_then(|links| {
            usize::try_from(index)
                .ok()
                .map(|index| links.get_url(index))
        })
        .unwrap_or_default();
    let output = words(&text, true);
    if buffer.is_null() || length <= 0 {
        return output.len().min(c_int::MAX as usize) as c_int;
    }
    let count = output.len().min(length as usize);
    unsafe {
        std::ptr::copy_nonoverlapping(output.as_ptr(), buffer, count);
    }
    count as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_CountRects(handle: *mut c_void, index: c_int) -> c_int {
    unsafe { handle.cast::<CpdfLinkExtract>().as_ref() }
        .and_then(|links| {
            usize::try_from(index)
                .ok()
                .map(|index| links.get_rects(index).len().min(c_int::MAX as usize) as c_int)
        })
        .unwrap_or(0)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetRect(
    handle: *mut c_void,
    index: c_int,
    rect: c_int,
    left: *mut f64,
    top: *mut f64,
    right: *mut f64,
    bottom: *mut f64,
) -> c_int {
    let bounds = unsafe { handle.cast::<CpdfLinkExtract>().as_ref() }.and_then(|links| {
        let index = usize::try_from(index).ok()?;
        let rect = usize::try_from(rect).ok()?;
        links.get_rects(index).get(rect).copied()
    });
    bounds.is_some_and(|bounds| unsafe { copy_rect(bounds, left, top, right, bottom) }) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetTextRange(
    handle: *mut c_void,
    index: c_int,
    start: *mut c_int,
    count: *mut c_int,
) -> c_int {
    if start.is_null() || count.is_null() {
        return 0;
    }
    let range = unsafe { handle.cast::<CpdfLinkExtract>().as_ref() }.and_then(|links| {
        usize::try_from(index)
            .ok()
            .and_then(|index| links.get_text_range(index))
    });
    let Some(range) = range else {
        return 0;
    };
    let (Ok(first), Ok(length)) = (c_int::try_from(range.start), c_int::try_from(range.count))
    else {
        return 0;
    };
    unsafe {
        *start = first;
        *count = length;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_CloseWebLinks(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<CpdfLinkExtract>()) });
    }
}

fn object_path(
    holder: &pdfoxium_core::fpdfapi::page::cpdf_pageobjectholder::CpdfPageObjectHolder,
    target: PageObjectId,
) -> Option<Vec<usize>> {
    let mut stack = vec![(holder, Vec::new())];
    while let Some((holder, prefix)) = stack.pop() {
        for index in 0..holder.page_object_count() {
            let mut path = prefix.clone();
            path.push(index);
            if holder.page_object_id_by_index(index) == Some(target) {
                return Some(path);
            }
            if let Some(form) = holder.page_object_by_index(index).and_then(|object| {
                object
                    .as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject>()
            }) {
                stack.push((form.form().holder(), path));
            }
        }
    }
    None
}
#[no_mangle]
pub unsafe extern "C" fn FPDFText_GetTextObject(page: *mut c_void, index: c_int) -> *mut c_void {
    let Some(handle) = (unsafe { text_page(page) }) else {
        return std::ptr::null_mut();
    };
    let Some(owner) = handle.owner.upgrade().filter(|owner| owner.is_alive()) else {
        return std::ptr::null_mut();
    };
    let _owner = owner;
    let Some((_, info)) = (unsafe { char_info(page, index) }) else {
        return std::ptr::null_mut();
    };
    let Some(target) = info.text_object() else {
        return std::ptr::null_mut();
    };
    // Complete the shared engine traversal before mutating either handle map.
    let Some(path) = object_path(
        unsafe { handle.original.as_ref() }
            .pdf_page()
            .unwrap()
            .holder(),
        target,
    ) else {
        return std::ptr::null_mut();
    };
    let mut indices = path.into_iter();
    let Some(first) = indices.next() else {
        return std::ptr::null_mut();
    };
    let Ok(first) = c_int::try_from(first) else {
        return std::ptr::null_mut();
    };
    let mut object = unsafe { FPDFPage_GetObject(handle.original.as_ptr().cast(), first) };
    for index in indices {
        if object.is_null() {
            return object;
        }
        object = unsafe { FPDFFormObj_GetObject(object, index as c_ulong) };
    }
    object
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn text_search_links_and_object_aliases_use_saved_page_content() {
        unsafe {
            assert_eq!(FPDFText_CountChars(std::ptr::null_mut()), -1);
            assert_eq!(
                FPDFText_GetCharIndexAtPos(std::ptr::null_mut(), 0., 0., 0., 0.),
                -3
            );
            assert_eq!(FPDFText_IsGenerated(std::ptr::null_mut(), 0), -1);
            let document = FPDF_CreateNewDocument();
            let page = FPDFPage_New(document, 0, 400., 400.);
            let object = FPDFPageObj_NewTextObj(document, c"Helvetica".as_ptr(), 12.);
            let source = "banana https://example.org/path";
            let input: Vec<u16> = source.encode_utf16().chain(Some(0)).collect();
            assert_eq!(FPDFText_SetText(object, input.as_ptr()), 1);
            FPDFPageObj_Transform(object, 1., 0., 0., 1., 20., 300.);
            FPDFPage_InsertObject(page, object);
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            let text = FPDFText_LoadPage(page);
            assert!(!text.is_null());
            assert_eq!(FPDFText_CountChars(text), source.len() as i32);
            assert_eq!(FPDFText_GetTextObject(text, 0), object);
            assert_eq!(FPDFText_GetUnicode(text, 0), b'b' as u32);
            assert_eq!(FPDFText_GetUnicode(text, -1), 0);
            assert_eq!(FPDFText_GetFontSize(text, 0), 12.);
            let mut flags = 0;
            let mut name = [0u8; 32];
            assert_eq!(
                FPDFText_GetFontInfo(text, 0, name.as_mut_ptr().cast(), 32, &mut flags),
                10
            );
            assert_eq!(&name[..10], b"Helvetica\0");
            let mut text_out = vec![0xffffu16; input.len() + 1];
            assert_eq!(
                FPDFText_GetText(text, 0, i32::MAX, text_out.as_mut_ptr()),
                input.len() as i32
            );
            assert_eq!(&text_out[..input.len()], input);
            assert_eq!(text_out[input.len()], 0xffff);
            let mut empty = 0xffff;
            assert_eq!(FPDFText_GetText(text, 0, 0, &mut empty), 1);
            assert_eq!(empty, 0);
            assert!(FPDFText_CountRects(text, 0, -1) > 0);
            let (mut left, mut top, mut right, mut bottom) = (0., 0., 0., 0.);
            assert_eq!(
                FPDFText_GetRect(text, 0, &mut left, &mut top, &mut right, &mut bottom),
                1
            );
            assert!(right > left && top > bottom);
            let pattern: Vec<u16> = "ana".encode_utf16().chain(Some(0)).collect();
            let search = FPDFText_FindStart(text, pattern.as_ptr(), 4, 0);
            assert_eq!(FPDFText_FindNext(search), 1);
            assert_eq!(FPDFText_GetSchResultIndex(search), 1);
            assert_eq!(FPDFText_GetSchCount(search), 3);
            assert_eq!(FPDFText_FindNext(search), 1);
            assert_eq!(FPDFText_GetSchResultIndex(search), 3);
            assert_eq!(FPDFText_FindPrev(search), 1);
            assert_eq!(FPDFText_GetSchResultIndex(search), 1);
            let links = FPDFLink_LoadWebLinks(text);
            assert_eq!(FPDFLink_CountWebLinks(links), 1);
            let (mut start, mut count) = (0, 0);
            assert_eq!(FPDFLink_GetTextRange(links, 0, &mut start, &mut count), 1);
            assert_eq!((start, count), (7, 24));
            let mut url = [0xffff; 64];
            let length = FPDFLink_GetURL(links, 0, url.as_mut_ptr(), 64);
            assert_eq!(
                String::from_utf16(&url[..length as usize - 1]).unwrap(),
                "https://example.org/path"
            );
            FPDF_ClosePage(page);
            assert!(FPDFText_GetTextObject(text, 0).is_null());
            assert_eq!(FPDFText_GetUnicode(text, 0), b'b' as u32);
            FPDFText_ClosePage(text);
            // Searches/links retain their immutable extracted data safely.
            assert_eq!(FPDFText_FindNext(search), 1);
            assert_eq!(FPDFLink_CountWebLinks(links), 1);
            FPDFText_FindClose(search);
            FPDFLink_CloseWebLinks(links);
            FPDF_CloseDocument(document);
        }
    }
}

#[path = "text_object_query.rs"]
mod object_query;
pub use object_query::*;
