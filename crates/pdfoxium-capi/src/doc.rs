// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Document navigation and metadata from `fpdfsdk/fpdf_doc.cpp`. Borrowed
//! dictionary/array identities are held in the document's stable handle cache.
use super::*;
use pdfoxium_core::fpdfapi::parser::cpdf_array::CpdfArray;
use pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef;
use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use pdfoxium_core::fpdfdoc::cpdf_aaction::{AActionType, CpdfAAction};
use pdfoxium_core::fpdfdoc::cpdf_action::{ActionType, CpdfAction};
use pdfoxium_core::fpdfdoc::cpdf_bookmark::CpdfBookmark;
use pdfoxium_core::fpdfdoc::cpdf_bookmarktree::CpdfBookmarkTree;
use pdfoxium_core::fpdfdoc::cpdf_dest::CpdfDest;
use pdfoxium_core::fpdfdoc::cpdf_link::CpdfLink;
use pdfoxium_core::fpdfdoc::cpdf_pagelabel::CpdfPageLabel;
use pdfoxium_core::fxcrt::fx_string::{utf16_decode, utf16_encode, utf8_encode};
use std::collections::{HashMap, HashSet};
use std::rc::Weak;

pub(super) struct BorrowedObjectHandle {
    object: ObjectRef,
    cache: Weak<RefCell<HashMap<usize, Box<BorrowedObjectHandle>>>>,
}
pub(super) type BorrowedObjectCache = Rc<RefCell<HashMap<usize, Box<BorrowedObjectHandle>>>>;
pub(super) fn borrow_object(cache: &BorrowedObjectCache, object: ObjectRef) -> *mut c_void {
    let key = Rc::as_ptr(&object) as *const () as usize;
    let mut cache_map = cache.borrow_mut();
    let handle = cache_map.entry(key).or_insert_with(|| {
        Box::new(BorrowedObjectHandle {
            object,
            cache: Rc::downgrade(cache),
        })
    });
    (&mut **handle as *mut BorrowedObjectHandle).cast()
}
pub(super) unsafe fn borrowed(handle: *mut c_void) -> Option<ObjectRef> {
    Some(
        unsafe { handle.cast::<BorrowedObjectHandle>().as_ref()? }
            .object
            .clone(),
    )
}
unsafe fn borrow_child(handle: *mut c_void, object: Option<ObjectRef>) -> *mut c_void {
    let Some(object) = object else {
        return std::ptr::null_mut();
    };
    let Some(owner) = (unsafe { handle.cast::<BorrowedObjectHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(cache) = owner.cache.upgrade() else {
        return std::ptr::null_mut();
    };
    borrow_object(&cache, object)
}
fn borrow_optional(cache: &BorrowedObjectCache, object: Option<ObjectRef>) -> *mut c_void {
    object.map_or(std::ptr::null_mut(), |object| borrow_object(cache, object))
}
pub(super) unsafe fn copy_bytes(bytes: &[u8], buffer: *mut c_void, length: c_ulong) -> c_ulong {
    if !buffer.is_null() && length as usize >= bytes.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
        }
    }
    bytes.len() as c_ulong
}
pub(super) unsafe fn copy_utf16(text: &[u32], buffer: *mut c_void, length: c_ulong) -> c_ulong {
    let bytes: Vec<u8> = utf16_encode(text)
        .into_iter()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    unsafe { copy_bytes(&bytes, buffer, length) }
}
pub(super) unsafe fn copy_terminated(text: &[u8], buffer: *mut c_void, length: c_ulong) -> c_ulong {
    let required = text.len() + 1;
    let output = if !buffer.is_null() && length as usize >= required {
        unsafe { std::slice::from_raw_parts_mut(buffer.cast::<u8>(), required) }
    } else {
        &mut []
    };
    pdfoxium_core::fpdfsdk::cpdfsdk_helpers::nul_terminate_maybe_copy_and_return_length(
        text, output,
    ) as c_ulong
}
pub(super) unsafe fn wide_string(text: *const u16) -> Vec<u32> {
    if text.is_null() {
        return Vec::new();
    }
    let mut length = 0;
    while unsafe { *text.add(length) } != 0 {
        length += 1;
    }
    let bytes: Vec<u8> = unsafe { std::slice::from_raw_parts(text, length) }
        .iter()
        .flat_map(|unit| unit.to_le_bytes())
        .collect();
    utf16_decode(&bytes, false)
}
fn title_equal(a: &[u32], b: &[u32]) -> bool {
    // Use deterministic Unicode lowercase instead of the process C locale.
    let lower =
        |point| char::from_u32(point).map_or(point, |ch| ch.to_lowercase().next().unwrap() as u32);
    a.len() == b.len() && a.iter().zip(b).all(|(&a, &b)| lower(a) == lower(b))
}
fn find_bookmark(tree: &CpdfBookmarkTree<'_>, title: &[u32]) -> CpdfBookmark {
    // Preserve the native depth-first first-match order without using the host
    // stack for arbitrarily deep untrusted outline trees.
    let mut stack = vec![tree.first_child(&CpdfBookmark::new(None))];
    let mut visited = HashSet::new();
    while let Some(bookmark) = stack.pop() {
        let Some(dict) = bookmark.dict() else {
            continue;
        };
        if !visited.insert(Rc::as_ptr(&dict) as *const () as usize) {
            continue;
        }
        if title_equal(&bookmark.title(), title) {
            return bookmark;
        }
        stack.push(tree.next_sibling(&bookmark));
        stack.push(tree.first_child(&bookmark));
    }
    CpdfBookmark::new(None)
}

#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetFirstChild(
    document: *mut c_void,
    bookmark: *mut c_void,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let child = CpdfBookmarkTree::new(&document.document.borrow())
        .first_child(&CpdfBookmark::new(unsafe { borrowed(bookmark) }));
    borrow_optional(&document.borrowed_objects, child.dict())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetNextSibling(
    document: *mut c_void,
    bookmark: *mut c_void,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = (unsafe { borrowed(bookmark) }) else {
        return std::ptr::null_mut();
    };
    let sibling = CpdfBookmarkTree::new(&document.document.borrow())
        .next_sibling(&CpdfBookmark::new(Some(dict)));
    borrow_optional(&document.borrowed_objects, sibling.dict())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetTitle(
    bookmark: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(dict) = (unsafe { borrowed(bookmark) }) else {
        return 0;
    };
    unsafe { copy_utf16(&CpdfBookmark::new(Some(dict)).title(), buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetCount(bookmark: *mut c_void) -> c_int {
    CpdfBookmark::new(unsafe { borrowed(bookmark) }).count()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_Find(
    document: *mut c_void,
    title: *const u16,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let title = unsafe { wide_string(title) };
    if title.is_empty() {
        return std::ptr::null_mut();
    }
    let result = find_bookmark(&CpdfBookmarkTree::new(&document.document.borrow()), &title);
    borrow_optional(&document.borrowed_objects, result.dict())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetDest(
    document: *mut c_void,
    bookmark: *mut c_void,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = (unsafe { borrowed(bookmark) }) else {
        return std::ptr::null_mut();
    };
    let bookmark = CpdfBookmark::new(Some(dict));
    let doc = document.document.borrow();
    let dest = bookmark
        .dest(&doc)
        .array()
        .or_else(|| bookmark.action().dest(&doc).array());
    borrow_optional(&document.borrowed_objects, dest)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetAction(bookmark: *mut c_void) -> *mut c_void {
    unsafe {
        borrow_child(
            bookmark,
            CpdfBookmark::new(borrowed(bookmark)).action().dict(),
        )
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetColor(
    bookmark: *mut c_void,
    red: *mut f32,
    green: *mut f32,
    blue: *mut f32,
) -> c_int {
    if red.is_null() || green.is_null() || blue.is_null() {
        return 0;
    }
    let Some(color) = CpdfBookmark::new(unsafe { borrowed(bookmark) }).color() else {
        return 0;
    };
    if color
        .iter()
        .any(|&c| !c.is_finite() || !(0.0..=1.0).contains(&c))
    {
        return 0;
    }
    unsafe {
        *red = color[0];
        *green = color[1];
        *blue = color[2];
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFBookmark_GetStyle(bookmark: *mut c_void) -> c_int {
    CpdfBookmark::new(unsafe { borrowed(bookmark) }).style()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAction_GetType(action: *mut c_void) -> c_ulong {
    match CpdfAction::new(unsafe { borrowed(action) }).action_type() {
        ActionType::GoTo => 1,
        ActionType::GoToR => 2,
        ActionType::Uri => 3,
        ActionType::Launch => 4,
        ActionType::GoToE => 5,
        _ => 0,
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAction_GetDest(
    document: *mut c_void,
    action: *mut c_void,
) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    if !matches!(unsafe { FPDFAction_GetType(action) }, 1 | 2 | 5) {
        return std::ptr::null_mut();
    }
    let dest = CpdfAction::new(unsafe { borrowed(action) }).dest(&document.document.borrow());
    borrow_optional(&document.borrowed_objects, dest.array())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAction_GetFilePath(
    action: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    if !matches!(unsafe { FPDFAction_GetType(action) }, 2 | 4 | 5) {
        return 0;
    }
    let path = CpdfAction::new(unsafe { borrowed(action) }).file_path();
    unsafe { copy_terminated(&utf8_encode(&path), buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAction_GetURIPath(
    document: *mut c_void,
    action: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    if unsafe { FPDFAction_GetType(action) } != 3 {
        return 0;
    }
    let path = CpdfAction::new(unsafe { borrowed(action) }).uri(&document.document.borrow());
    unsafe { copy_terminated(&path, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDest_GetDestPageIndex(
    document: *mut c_void,
    dest: *mut c_void,
) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return -1;
    };
    let Some(array) = (unsafe { borrowed(dest) }) else {
        return -1;
    };
    CpdfDest::new(Some(array)).dest_page_index(&mut document.document.borrow_mut())
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDest_GetView(
    dest: *mut c_void,
    num_params: *mut c_ulong,
    params: *mut f32,
) -> c_ulong {
    if num_params.is_null() {
        return 0;
    }
    let dest = CpdfDest::new(unsafe { borrowed(dest) });
    let count = dest.num_params();
    unsafe {
        *num_params = count as c_ulong;
    }
    if !params.is_null() {
        for index in 0..count {
            unsafe {
                *params.add(index) = dest.param(index);
            }
        }
    }
    dest.zoom_mode() as c_ulong
}
#[no_mangle]
pub unsafe extern "C" fn FPDFDest_GetLocationInPage(
    dest: *mut c_void,
    has_x: *mut c_int,
    has_y: *mut c_int,
    has_zoom: *mut c_int,
    x: *mut f32,
    y: *mut f32,
    zoom: *mut f32,
) -> c_int {
    if has_x.is_null()
        || has_y.is_null()
        || has_zoom.is_null()
        || x.is_null()
        || y.is_null()
        || zoom.is_null()
    {
        return 0;
    }
    let Some(location) = CpdfDest::new(unsafe { borrowed(dest) }).xyz() else {
        return 0;
    };
    unsafe {
        *has_x = location.x.is_some() as c_int;
        *has_y = location.y.is_some() as c_int;
        *has_zoom = location.zoom.is_some() as c_int;
        if let Some(value) = location.x {
            *x = value;
        }
        if let Some(value) = location.y {
            *y = value;
        }
        if let Some(value) = location.zoom {
            *zoom = value;
        }
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetLinkAtPoint(page: *mut c_void, x: f64, y: f64) -> *mut c_void {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return std::ptr::null_mut();
    };
    let link = page
        .link_list
        .borrow_mut()
        .get_link_at_point(page.pdf_page().unwrap(), CfxPointF::new(x as f32, y as f32));
    borrow_optional(
        &page.borrowed_objects,
        link.and_then(|(link, _)| link.dict()),
    )
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetLinkZOrderAtPoint(page: *mut c_void, x: f64, y: f64) -> c_int {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return -1;
    };
    let result = page
        .link_list
        .borrow_mut()
        .get_link_at_point(page.pdf_page().unwrap(), CfxPointF::new(x as f32, y as f32));
    result.map_or(-1, |(_, index)| index)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetDest(document: *mut c_void, link: *mut c_void) -> *mut c_void {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = (unsafe { borrowed(link) }) else {
        return std::ptr::null_mut();
    };
    let link = CpdfLink::new(Some(dict));
    let doc = document.document.borrow();
    let dest = link
        .dest(&doc)
        .array()
        .or_else(|| link.action().dest(&doc).array());
    borrow_optional(&document.borrowed_objects, dest)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetAction(link: *mut c_void) -> *mut c_void {
    unsafe { borrow_child(link, CpdfLink::new(borrowed(link)).action().dict()) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_Enumerate(
    page: *mut c_void,
    start: *mut c_int,
    link: *mut *mut c_void,
) -> c_int {
    if start.is_null() || link.is_null() {
        return 0;
    }
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    let Ok(first) = usize::try_from(unsafe { *start }) else {
        return 0;
    };
    let Some(annots) = page.pdf_page().unwrap().annots_array() else {
        return 0;
    };
    let borrowed = annots.borrow();
    let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
        return 0;
    };
    for index in first..array.size() {
        let Some(dict) = array
            .get_direct_object_at(index)
            .filter(|d| d.borrow().object_type() == ObjectType::Dictionary)
        else {
            continue;
        };
        let is_link = dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_byte_string_for(b"Subtype", b"")
            == b"Link";
        if !is_link {
            continue;
        }
        let Ok(next) = c_int::try_from(index + 1) else {
            return 0;
        };
        unsafe {
            *start = next;
            *link = borrow_object(&page.borrowed_objects, dict);
        }
        return 1;
    }
    0
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetAnnot(page: *mut c_void, link: *mut c_void) -> *mut c_void {
    let Some(page) = NonNull::new(page.cast::<PageHandle>()) else {
        return std::ptr::null_mut();
    };
    let Some(dict) =
        (unsafe { borrowed(link) }).filter(|d| d.borrow().object_type() == ObjectType::Dictionary)
    else {
        return std::ptr::null_mut();
    };
    Box::into_raw(Box::new(annot::AnnotHandle {
        context: Some(pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext::new(dict)),
        #[cfg(feature = "xfa")]
        xfa: None,
        page,
        object_handles: Default::default(),
    }))
    .cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetAnnotRect(link: *mut c_void, rect: *mut FS_RECTF) -> c_int {
    if rect.is_null() {
        return 0;
    }
    let Some(dict) = (unsafe { borrowed(link) }) else {
        return 0;
    };
    let value = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .map(|d| d.get_rect_for(b"Rect"));
    let Some(value) = value else {
        return 0;
    };
    unsafe {
        *rect = FS_RECTF {
            left: value.left,
            top: value.top,
            right: value.right,
            bottom: value.bottom,
        };
    }
    1
}
fn quad_points(link: &ObjectRef) -> Option<ObjectRef> {
    link.borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_array_for(b"QuadPoints")
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_CountQuadPoints(link: *mut c_void) -> c_int {
    let Some(array) = (unsafe { borrowed(link) }).and_then(|dict| quad_points(&dict)) else {
        return 0;
    };
    let count = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .unwrap()
        .size()
        / 8;
    c_int::try_from(count).unwrap_or(c_int::MAX)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFLink_GetQuadPoints(
    link: *mut c_void,
    index: c_int,
    quad: *mut FS_QUADPOINTSF,
) -> c_int {
    if quad.is_null() {
        return 0;
    }
    let Ok(index) = usize::try_from(index) else {
        return 0;
    };
    let Some(array) = (unsafe { borrowed(link) }).and_then(|dict| quad_points(&dict)) else {
        return 0;
    };
    let borrowed = array.borrow();
    let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
    if index >= array.size() / 8 {
        return 0;
    }
    let v: [f32; 8] = std::array::from_fn(|i| array.get_float_at(index * 8 + i));
    unsafe {
        *quad = FS_QUADPOINTSF {
            x1: v[0],
            y1: v[1],
            x2: v[2],
            y2: v[3],
            x3: v[4],
            y3: v[5],
            x4: v[6],
            y4: v[7],
        };
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetPageAAction(page: *mut c_void, kind: c_int) -> *mut c_void {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return std::ptr::null_mut();
    };
    let kind = match kind {
        0 => AActionType::OpenPage,
        1 => AActionType::ClosePage,
        _ => return std::ptr::null_mut(),
    };
    let dict = page.pdf_page().unwrap().holder().dict();
    let aa = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|d| d.get_dict_for(b"AA"));
    let aa = CpdfAAction::new(aa);
    if !aa.action_exist(kind) {
        return std::ptr::null_mut();
    }
    borrow_optional(&page.borrowed_objects, aa.action(kind).dict())
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetFileIdentifier(
    document: *mut c_void,
    kind: c_int,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    if !matches!(kind, 0 | 1) {
        return 0;
    }
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let Some(array) = document.document.borrow().file_identifier() else {
        return 0;
    };
    let string = array
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .and_then(|a| a.get_direct_object_at(kind as usize))
        .filter(|v| v.borrow().object_type() == ObjectType::String);
    let Some(string) = string else {
        return 0;
    };
    let bytes = string.borrow().get_string();
    unsafe { copy_terminated(&bytes, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetMetaText(
    document: *mut c_void,
    tag: *const c_char,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    if tag.is_null() {
        return 0;
    }
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let Some(info) = document.document.borrow_mut().info() else {
        return 0;
    };
    let text = info
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .map(|d| d.get_unicode_text_for(unsafe { CStr::from_ptr(tag) }.to_bytes()));
    let Some(text) = text else {
        return 0;
    };
    unsafe { copy_utf16(&text, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetPageLabel(
    document: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    if index < 0 {
        return 0;
    }
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let label = CpdfPageLabel::new(Some(&document.document.borrow())).get_label(index);
    let Some(label) = label else {
        return 0;
    };
    unsafe { copy_utf16(&label, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetLink(annotation: *mut c_void) -> *mut c_void {
    let Some(annotation) = (unsafe { annotation.cast::<annot::AnnotHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = annotation.pdf_dict() else {
        return std::ptr::null_mut();
    };
    if pdfoxium_core::fpdfsdk::fpdf_annot::subtype(&dict)
        != pdfoxium_core::fpdfdoc::cpdf_annot::Subtype::Link
    {
        return std::ptr::null_mut();
    }
    let page = unsafe { annotation.page.as_ref() };
    borrow_object(&page.borrowed_objects, dict.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfoxium_core::fpdfapi::parser::cpdf_object::retain_object;
    use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use pdfoxium_core::fpdfapi::parser::cpdf_string::CpdfString;
    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }
    unsafe fn title(bookmark: *mut c_void) -> String {
        let length = unsafe { FPDFBookmark_GetTitle(bookmark, std::ptr::null_mut(), 0) };
        let mut bytes = vec![0u8; length as usize];
        assert_eq!(
            unsafe { FPDFBookmark_GetTitle(bookmark, bytes.as_mut_ptr().cast(), length) },
            length
        );
        let units: Vec<u16> = bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take(bytes.len() / 2 - 1)
            .collect();
        String::from_utf16(&units).unwrap()
    }
    // Complete Bookmarks and FindBookmarks embedder assertions from the source
    // fixture, with an additional case-insensitive lookup and short-buffer guard.
    #[test]
    fn source_bookmarks_and_find_bookmarks() {
        let bytes = include_bytes!("../../../upstream/pdfium/testing/resources/bookmarks.pdf");
        unsafe {
            let doc = FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
            assert!(!doc.is_null());
            let child = FPDFBookmark_GetFirstChild(doc, std::ptr::null_mut());
            assert!(!child.is_null());
            assert_eq!(title(child), "A Good Beginning");
            assert_eq!(FPDFBookmark_GetTitle(child, std::ptr::null_mut(), 0), 34);
            assert_eq!(FPDFBookmark_GetCount(child), 0);
            assert_eq!(FPDFBookmark_GetCount(std::ptr::null_mut()), 0);
            assert!(FPDFBookmark_GetDest(doc, child).is_null());
            assert!(FPDFBookmark_GetAction(child).is_null());
            assert!(FPDFBookmark_GetFirstChild(doc, child).is_null());
            let sibling = FPDFBookmark_GetNextSibling(doc, child);
            assert!(!sibling.is_null());
            assert_eq!(title(sibling), "Open Middle");
            assert_eq!(FPDFBookmark_GetTitle(sibling, std::ptr::null_mut(), 0), 24);
            assert!(!FPDFBookmark_GetAction(sibling).is_null());
            assert_eq!(FPDFBookmark_GetCount(sibling), 1);
            let sibling2 = FPDFBookmark_GetNextSibling(doc, sibling);
            assert!(!sibling2.is_null());
            assert_eq!(title(sibling2), "A Good Closed Ending");
            assert_eq!(FPDFBookmark_GetTitle(sibling2, std::ptr::null_mut(), 0), 42);
            assert_eq!(FPDFBookmark_GetCount(sibling2), -2);
            assert!(FPDFBookmark_GetNextSibling(doc, sibling2).is_null());
            let grandchild = FPDFBookmark_GetFirstChild(doc, sibling);
            assert!(!grandchild.is_null());
            assert_eq!(title(grandchild), "Open Middle Descendant");
            assert_eq!(
                FPDFBookmark_GetTitle(grandchild, std::ptr::null_mut(), 0),
                46
            );
            assert_eq!(FPDFBookmark_GetCount(grandchild), 0);
            assert!(!FPDFBookmark_GetDest(doc, grandchild).is_null());
            assert!(FPDFBookmark_GetNextSibling(doc, grandchild).is_null());
            assert_eq!(
                FPDFBookmark_Find(doc, wide("A Good Beginning").as_ptr()),
                child
            );
            assert_eq!(
                FPDFBookmark_Find(doc, wide("a good beginning").as_ptr()),
                child
            );
            assert!(FPDFBookmark_Find(doc, wide("A BAD Beginning").as_ptr()).is_null());
            let mut guard = [0xabu8; 8];
            assert_eq!(
                FPDFBookmark_GetTitle(child, guard.as_mut_ptr().cast(), 8),
                34
            );
            assert_eq!(guard, [0xab; 8]);
            FPDF_CloseDocument(doc);
        }
    }
    #[test]
    fn borrowed_link_action_annotation_identity_and_binary_uri_buffers() {
        unsafe {
            let doc = FPDF_CreateNewDocument();
            let page = FPDFPage_New(doc, 0, 200.0, 200.0);
            let annot = FPDFPage_CreateAnnot(page, 2);
            let rect = FS_RECTF {
                left: 10.0,
                top: 40.0,
                right: 60.0,
                bottom: 20.0,
            };
            assert_ne!(FPDFAnnot_SetRect(annot, &rect), 0);
            let dict = (*annot.cast::<annot::AnnotHandle>())
                .context
                .as_ref()
                .unwrap()
                .dict()
                .clone();
            let mut action = CpdfDictionary::new();
            action.set_new_name_for(b"S", b"URI");
            action.set_new_string_for(b"URI", b"https://example.test/\xff\0tail");
            dict.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"A", action);
            let link = FPDFAnnot_GetLink(annot);
            assert!(!link.is_null());
            assert_eq!(FPDFAnnot_GetLink(annot), link);
            assert_eq!(FPDFLink_GetLinkAtPoint(page, 20.0, 25.0), link);
            assert_eq!(FPDFLink_GetLinkZOrderAtPoint(page, 20.0, 25.0), 0);
            let action = FPDFLink_GetAction(link);
            assert!(!action.is_null());
            assert_eq!(FPDFLink_GetAction(link), action);
            assert_eq!(FPDFAction_GetType(action), 3);
            let required = FPDFAction_GetURIPath(doc, action, std::ptr::null_mut(), 0);
            assert_eq!(
                required,
                b"https://example.test/\xff\0tail\0".len() as c_ulong
            );
            let mut guard = vec![0xa5; required as usize];
            assert_eq!(
                FPDFAction_GetURIPath(doc, action, guard.as_mut_ptr().cast(), required - 1),
                required
            );
            assert!(guard.iter().all(|&v| v == 0xa5));
            assert_eq!(
                FPDFAction_GetURIPath(doc, action, guard.as_mut_ptr().cast(), required),
                required
            );
            assert_eq!(guard, b"https://example.test/\xff\0tail\0");
            let context = FPDFLink_GetAnnot(page, link);
            assert!(!context.is_null());
            assert_eq!(FPDFPage_GetAnnotIndex(page, context), 0);
            assert_eq!(FPDFAnnot_GetLink(context), link);
            let mut position = 0;
            let mut enumerated = std::ptr::null_mut();
            assert_ne!(FPDFLink_Enumerate(page, &mut position, &mut enumerated), 0);
            assert_eq!(position, 1);
            assert_eq!(enumerated, link);
            assert_eq!(FPDFLink_Enumerate(page, &mut position, &mut enumerated), 0);
            position = -1;
            assert_eq!(FPDFLink_Enumerate(page, &mut position, &mut enumerated), 0);
            FPDFPage_CloseAnnot(context);
            FPDFPage_CloseAnnot(annot);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(doc);
        }
    }
    #[test]
    fn destination_parameters_preserve_absent_values_and_metadata_is_utf16le() {
        let cache: BorrowedObjectCache = Default::default();
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::number_from_signed(2));
        array.append_new_name(b"XYZ");
        array.append_new(CpdfScalarObject::number_from_float(42.5));
        array.append_new(CpdfScalarObject::null());
        array.append_new(CpdfScalarObject::number_from_signed(0));
        let array = retain_object(array);
        let dest = borrow_object(&cache, array.clone());
        assert_eq!(borrow_object(&cache, array), dest);
        unsafe {
            let mut count = 99;
            let mut params = [-99.0; 4];
            assert_eq!(FPDFDest_GetView(dest, &mut count, params.as_mut_ptr()), 1);
            assert_eq!(count, 3);
            assert_eq!(params, [42.5, 0.0, 0.0, -99.0]);
            let (mut has_x, mut has_y, mut has_zoom) = (9, 9, 9);
            let (mut x, mut y, mut zoom) = (-1.0, -2.0, -3.0);
            assert_ne!(
                FPDFDest_GetLocationInPage(
                    dest,
                    &mut has_x,
                    &mut has_y,
                    &mut has_zoom,
                    &mut x,
                    &mut y,
                    &mut zoom
                ),
                0
            );
            assert_eq!((has_x, has_y, has_zoom), (1, 0, 0));
            assert_eq!((x, y, zoom), (42.5, -2.0, -3.0));
            let doc = FPDF_CreateNewDocument();
            let document = &*doc.cast::<DocumentHandle>();
            let info = document.document.borrow_mut().info().unwrap();
            let value: Vec<u32> = "Title Ω 😀".chars().map(u32::from).collect();
            info.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"Title", CpdfString::from_unicode(None, &value));
            let expected: Vec<u8> = wide("Title Ω 😀")
                .into_iter()
                .flat_map(u16::to_le_bytes)
                .collect();
            let mut buffer = vec![0u8; expected.len()];
            assert_eq!(
                FPDF_GetMetaText(
                    doc,
                    c"Title".as_ptr(),
                    buffer.as_mut_ptr().cast(),
                    buffer.len() as c_ulong
                ),
                expected.len() as c_ulong
            );
            assert_eq!(buffer, expected);
            assert_eq!(
                FPDF_GetMetaText(doc, c"Missing".as_ptr(), std::ptr::null_mut(), 0),
                2
            );
            FPDF_CloseDocument(doc);
        }
    }
    /// Complete `fpdfsdk/fpdf_doc_unittest.cpp::PDFDocTest.FindBookmark`.
    #[test]
    #[allow(non_snake_case)]
    fn FindBookmark() {
        use pdfoxium_core::fpdfapi::parser::cpdf_reference::CpdfReference;
        fn set_reference(
            dict: &ObjectRef,
            key: &[u8],
            target: &ObjectRef,
            holder: &pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef,
        ) {
            dict.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(key, CpdfReference::new(holder, target.borrow().obj_num()));
        }
        unsafe {
            let raw = FPDF_CreateNewDocument();
            let doc = &*raw.cast::<DocumentHandle>();
            let root = doc.document.borrow().root().unwrap();
            for expected in ["", "Preface"] {
                assert!(FPDFBookmark_Find(raw, wide(expected).as_ptr()).is_null());
            }
            root.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_new_for(b"Outlines", CpdfDictionary::new());
            for expected in ["", "Preface"] {
                assert!(FPDFBookmark_Find(raw, wide(expected).as_ptr()).is_null());
            }
            for topology in 0..3 {
                let holder = doc.document.borrow().holder();
                let count = if topology == 2 { 4 } else { 3 };
                let bookmarks: Vec<ObjectRef> = (0..count)
                    .map(|_| holder.borrow_mut().new_indirect_dictionary())
                    .collect();
                for index in 1..count {
                    bookmarks[index]
                        .borrow_mut()
                        .as_any_mut()
                        .downcast_mut::<CpdfDictionary>()
                        .unwrap()
                        .set_new_string_for(b"Title", format!("Chapter {index}").as_bytes());
                    let parent = if topology == 1 && index == 2 { 1 } else { 0 };
                    set_reference(&bookmarks[index], b"Parent", &bookmarks[parent], &holder);
                }
                if topology == 0 {
                    set_reference(&bookmarks[1], b"Next", &bookmarks[2], &holder);
                    set_reference(&bookmarks[2], b"Prev", &bookmarks[1], &holder);
                } else if topology == 1 {
                    set_reference(&bookmarks[1], b"First", &bookmarks[2], &holder);
                    set_reference(&bookmarks[2], b"First", &bookmarks[1], &holder);
                } else {
                    set_reference(&bookmarks[1], b"Next", &bookmarks[2], &holder);
                    set_reference(&bookmarks[2], b"Next", &bookmarks[3], &holder);
                    set_reference(&bookmarks[3], b"Next", &bookmarks[1], &holder);
                }
                {
                    let mut borrowed = bookmarks[0].borrow_mut();
                    let outlines = borrowed
                        .as_any_mut()
                        .downcast_mut::<CpdfDictionary>()
                        .unwrap();
                    outlines.set_new_name_for(b"Type", b"Outlines");
                    outlines.set_new_for(b"Count", CpdfScalarObject::number_from_signed(2));
                }
                set_reference(&bookmarks[0], b"First", &bookmarks[1], &holder);
                set_reference(&bookmarks[0], b"Last", &bookmarks[2], &holder);
                set_reference(&root, b"Outlines", &bookmarks[0], &holder);
                let missing = if topology == 2 {
                    "Chapter 8"
                } else {
                    "Chapter 3"
                };
                assert!(FPDFBookmark_Find(raw, wide(missing).as_ptr()).is_null());
                if topology == 0 {
                    assert!(FPDFBookmark_Find(raw, wide("Chapter").as_ptr()).is_null());
                }
                let index = if topology == 2 { 3 } else { 2 };
                let expected = borrow_object(&doc.borrowed_objects, bookmarks[index].clone());
                assert_eq!(
                    FPDFBookmark_Find(raw, wide(&format!("Chapter {index}")).as_ptr()),
                    expected
                );
                if topology == 0 {
                    assert_eq!(FPDFBookmark_Find(raw, wide("cHaPter 2").as_ptr()), expected);
                }
            }
            FPDF_CloseDocument(raw);
        }
    }
    /// Complete `fpdfsdk/fpdf_doc_unittest.cpp::PDFDocTest.GetLocationInPage`.
    #[test]
    #[allow(non_snake_case)]
    fn GetLocationInPage() {
        let cache: BorrowedObjectCache = Default::default();
        let mut array = CpdfArray::new();
        array.append_new(CpdfScalarObject::number_from_signed(0));
        array.append_new_name(b"XYZ");
        for value in [4, 5, 6] {
            array.append_new(CpdfScalarObject::number_from_signed(value));
        }
        let array = retain_object(array);
        let handle = borrow_object(&cache, array.clone());
        let (mut has_x, mut has_y, mut has_zoom) = (0, 0, 0);
        let (mut x, mut y, mut zoom) = (0.0, 0.0, 0.0);
        unsafe {
            assert_ne!(
                FPDFDest_GetLocationInPage(
                    handle,
                    &mut has_x,
                    &mut has_y,
                    &mut has_zoom,
                    &mut x,
                    &mut y,
                    &mut zoom
                ),
                0
            );
            assert_eq!((has_x, has_y, has_zoom), (1, 1, 1));
            assert_eq!((x, y, zoom), (4.0, 5.0, 6.0));
            for index in 2..5 {
                array
                    .borrow_mut()
                    .as_any_mut()
                    .downcast_mut::<CpdfArray>()
                    .unwrap()
                    .set_new_at(index, CpdfScalarObject::null());
            }
            assert_ne!(
                FPDFDest_GetLocationInPage(
                    handle,
                    &mut has_x,
                    &mut has_y,
                    &mut has_zoom,
                    &mut x,
                    &mut y,
                    &mut zoom
                ),
                0
            );
            assert_eq!((has_x, has_y, has_zoom), (0, 0, 0));
            let empty = borrow_object(&cache, retain_object(CpdfArray::new()));
            assert_eq!(
                FPDFDest_GetLocationInPage(
                    empty,
                    &mut has_x,
                    &mut has_y,
                    &mut has_zoom,
                    &mut x,
                    &mut y,
                    &mut zoom
                ),
                0
            );
        }
    }
}
