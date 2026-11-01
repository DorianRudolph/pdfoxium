// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Ordinary annotation C ABI from `fpdfsdk/fpdf_annot.cpp`.
//! Callers keep annotation/page handles live and supply valid buffers. The
//! annotation borrows its page; attached object aliases borrow the annotation.
use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext;
use pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef;
use pdfoxium_core::fpdfsdk::fpdf_annot as sdk;
use pdfoxium_core::fxcrt::fx_string::utf16_encode;
use std::collections::HashMap;

pub(super) struct AnnotHandle {
    pub(super) context: Option<CpdfAnnotContext>,
    #[cfg(feature = "xfa")]
    pub(super) xfa:
        Option<std::rc::Weak<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_widget::CpdfXfaWidget>>,
    pub(super) page: NonNull<PageHandle>,
    pub(super) object_handles: HashMap<PageObjectId, Box<PageObjectHandle>>,
}
impl AnnotHandle {
    #[cfg(feature = "xfa")]
    pub(super) fn xfa_widget(
        &self,
    ) -> Option<Rc<pdfoxium_core::fpdfsdk::fpdfxfa::cpdfxfa_widget::CpdfXfaWidget>> {
        self.xfa
            .as_ref()?
            .upgrade()
            .filter(|widget| widget.is_alive())
    }
    pub(super) fn pdf_dict(&self) -> Option<ObjectRef> {
        Some(self.context.as_ref()?.dict().clone())
    }
}
fn new_annotation(dict: ObjectRef, page: NonNull<PageHandle>) -> *mut c_void {
    Box::into_raw(Box::new(AnnotHandle {
        context: Some(CpdfAnnotContext::new(dict)),
        #[cfg(feature = "xfa")]
        xfa: None,
        page,
        object_handles: Default::default(),
    }))
    .cast()
}
unsafe fn annotation_dict(annot: *mut c_void) -> Option<ObjectRef> {
    Some(
        unsafe { annot.cast::<AnnotHandle>().as_ref()? }
            .context
            .as_ref()?
            .dict()
            .clone(),
    )
}
unsafe fn byte_string<'a>(value: *const c_char) -> &'a [u8] {
    if value.is_null() {
        &[]
    } else {
        unsafe { CStr::from_ptr(value) }.to_bytes()
    }
}
unsafe fn wide_string(value: *const u16) -> Vec<u32> {
    if value.is_null() {
        return Vec::new();
    }
    let mut length = 0;
    while unsafe { *value.add(length) } != 0 {
        length += 1;
    }
    char::decode_utf16(
        unsafe { std::slice::from_raw_parts(value, length) }
            .iter()
            .copied(),
    )
    .map(|c| c.map_or(0xfffd, u32::from))
    .collect()
}
unsafe fn copy_wide(text: &[u32], buffer: *mut u16, buflen: c_ulong) -> c_ulong {
    let bytes: Vec<u16> = utf16_encode(text)
        .into_iter()
        .chain(std::iter::once(0))
        .collect();
    let size = bytes.len() * 2;
    if !buffer.is_null() && buflen as usize >= size {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer, bytes.len());
        }
    }
    size as c_ulong
}
fn quad_values(quad: &FS_QUADPOINTSF) -> [f32; 8] {
    [
        quad.x1, quad.y1, quad.x2, quad.y2, quad.x3, quad.y3, quad.x4, quad.y4,
    ]
}
fn quad_from_values(v: [f32; 8]) -> FS_QUADPOINTSF {
    FS_QUADPOINTSF {
        x1: v[0],
        y1: v[1],
        x2: v[2],
        y2: v[3],
        x3: v[4],
        y3: v[5],
        x4: v[6],
        y4: v[7],
    }
}

#[no_mangle]
pub extern "C" fn FPDFAnnot_IsSupportedSubtype(subtype: c_int) -> c_int {
    sdk::supported_subtype(subtype) as c_int
}
#[no_mangle]
pub extern "C" fn FPDFAnnot_IsObjectSupportedSubtype(subtype: c_int) -> c_int {
    sdk::object_supported_subtype(subtype) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_CreateAnnot(page: *mut c_void, subtype: c_int) -> *mut c_void {
    let Some(pointer) = NonNull::new(page.cast::<PageHandle>()) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = unsafe { &mut *pointer.as_ptr() }
        .page_mut()
        .and_then(|mut page| sdk::create_annotation(&mut page, subtype))
    else {
        return std::ptr::null_mut();
    };
    let owner = unsafe { &*pointer.as_ptr() };
    owner
        .link_list
        .borrow_mut()
        .invalidate_page(owner.pdf_page().unwrap());
    new_annotation(dict, pointer)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetAnnotCount(page: *mut c_void) -> c_int {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    c_int::try_from(sdk::annotation_count(page.pdf_page().unwrap())).unwrap_or(c_int::MAX)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetAnnot(page: *mut c_void, index: c_int) -> *mut c_void {
    let (Some(pointer), Ok(index)) = (
        NonNull::new(page.cast::<PageHandle>()),
        usize::try_from(index),
    ) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = unsafe { pointer.as_ref() }
        .pdf_page()
        .and_then(|page| sdk::annotation_at(page, index))
    else {
        return std::ptr::null_mut();
    };
    new_annotation(dict, pointer)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_GetAnnotIndex(page: *mut c_void, annot: *mut c_void) -> c_int {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return -1;
    };
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return -1;
    };
    sdk::annotation_index(page.pdf_page().unwrap(), &dict)
        .and_then(|i| c_int::try_from(i).ok())
        .unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_CloseAnnot(annot: *mut c_void) {
    if !annot.is_null() {
        drop(unsafe { Box::from_raw(annot.cast::<AnnotHandle>()) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_RemoveAnnot(page: *mut c_void, index: c_int) -> c_int {
    let (Some(page), Ok(index)) = ((unsafe { pdf_handle(page) }), usize::try_from(index)) else {
        return 0;
    };
    let removed = sdk::remove_annotation(page.pdf_page().unwrap(), index);
    if removed {
        page.link_list
            .borrow_mut()
            .invalidate_page(page.pdf_page().unwrap());
    }
    removed as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetSubtype(annot: *mut c_void) -> c_int {
    #[cfg(feature = "xfa")]
    if let Some(widget) =
        (unsafe { annot.cast::<AnnotHandle>().as_ref() }).and_then(AnnotHandle::xfa_widget)
    {
        return widget.subtype() as c_int;
    }
    unsafe { annotation_dict(annot) }.map_or(0, |d| sdk::subtype(&d) as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetObjectCount(annot: *mut c_void) -> c_int {
    let Some(pointer) = NonNull::new(annot.cast::<AnnotHandle>()) else {
        return 0;
    };
    let page = unsafe { (*pointer.as_ptr()).page };
    let Some(context) = (unsafe { &mut (*pointer.as_ptr()).context }).as_mut() else {
        return 0;
    };
    if !sdk::ensure_form(context, unsafe { page.as_ref() }.pdf_page().unwrap()) {
        return 0;
    }
    c_int::try_from(context.form().unwrap().holder().page_object_count()).unwrap_or(c_int::MAX)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetObject(annot: *mut c_void, index: c_int) -> *mut c_void {
    let (Some(pointer), Ok(index)) = (
        NonNull::new(annot.cast::<AnnotHandle>()),
        usize::try_from(index),
    ) else {
        return std::ptr::null_mut();
    };
    let page = unsafe { (*pointer.as_ptr()).page };
    let Some(context) = (unsafe { &mut (*pointer.as_ptr()).context }).as_mut() else {
        return std::ptr::null_mut();
    };
    if !sdk::ensure_form(context, unsafe { page.as_ref() }.pdf_page().unwrap()) {
        return std::ptr::null_mut();
    }
    let Some(id) = context
        .form()
        .unwrap()
        .holder()
        .page_object_id_by_index(index)
    else {
        return std::ptr::null_mut();
    };
    let handles = unsafe { &mut (*pointer.as_ptr()).object_handles };
    let handle = handles.entry(id).or_insert_with(|| {
        Box::new(PageObjectHandle::new(ObjectLocation::InAnnotation {
            annot: pointer,
            index,
            id,
        }))
    });
    (&mut **handle as *mut PageObjectHandle).cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_AppendObject(annot: *mut c_void, object: *mut c_void) -> c_int {
    let (Some(pointer), Some(object_pointer)) = (
        NonNull::new(annot.cast::<AnnotHandle>()),
        NonNull::new(object.cast::<PageObjectHandle>()),
    ) else {
        return 0;
    };
    if !matches!(
        unsafe { &(*object_pointer.as_ptr()).location },
        ObjectLocation::Owned(_)
    ) {
        return 0;
    }
    let page = unsafe { (*pointer.as_ptr()).page };
    let Some(context) = (unsafe { &mut (*pointer.as_ptr()).context }).as_mut() else {
        return 0;
    };
    if !sdk::ensure_append_form(context, unsafe { page.as_ref() }.pdf_page().unwrap()) {
        return 0;
    }
    let mut handle = unsafe { Box::from_raw(object_pointer.as_ptr()) };
    let ObjectLocation::Owned(object) =
        std::mem::replace(&mut handle.location, ObjectLocation::Transferring)
    else {
        unreachable!()
    };
    let holder = context.form_mut().unwrap().holder_mut();
    let index = holder.page_object_count();
    holder.append_page_object(object);
    let id = holder.page_object_id_by_index(index).unwrap();
    handle.location = ObjectLocation::InAnnotation {
        annot: pointer,
        index,
        id,
    };
    unsafe { &mut (*pointer.as_ptr()).object_handles }.insert(id, handle);
    let document = unsafe { &(*page.as_ptr())._document };
    sdk::update_content_stream(context, &mut document.borrow_mut()) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_UpdateObject(annot: *mut c_void, object: *mut c_void) -> c_int {
    let (Some(pointer), Some(object_pointer)) = (
        NonNull::new(annot.cast::<AnnotHandle>()),
        NonNull::new(object.cast::<PageObjectHandle>()),
    ) else {
        return 0;
    };
    if !matches!(unsafe{&(*object_pointer.as_ptr()).location},ObjectLocation::InAnnotation{annot,..} if *annot==pointer)
    {
        return 0;
    }
    let page = unsafe { (*pointer.as_ptr()).page };
    let Some(context) = (unsafe { &mut (*pointer.as_ptr()).context }).as_mut() else {
        return 0;
    };
    if !sdk::object_supported_subtype(sdk::subtype(context.dict()) as c_int) {
        return 0;
    }
    sdk::update_content_stream(
        context,
        &mut unsafe { &(*page.as_ptr())._document }.borrow_mut(),
    ) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_RemoveObject(annot: *mut c_void, index: c_int) -> c_int {
    let (Some(pointer), Ok(index)) = (
        NonNull::new(annot.cast::<AnnotHandle>()),
        usize::try_from(index),
    ) else {
        return 0;
    };
    let page = unsafe { (*pointer.as_ptr()).page };
    let Some(context) = (unsafe { &mut (*pointer.as_ptr()).context }).as_mut() else {
        return 0;
    };
    if !sdk::object_supported_subtype(sdk::subtype(context.dict()) as c_int)
        || sdk::appearance(context.dict(), 0, true).is_none()
    {
        return 0;
    }
    let Some(form) = context.form_mut() else {
        return 0;
    };
    let Some(id) = form.holder().page_object_id_by_index(index) else {
        return 0;
    };
    if !form.holder_mut().erase_page_object_at_index(index) {
        return 0;
    }
    unsafe { &mut (*pointer.as_ptr()).object_handles }.remove(&id);
    sdk::update_content_stream(
        context,
        &mut unsafe { &(*page.as_ptr())._document }.borrow_mut(),
    ) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_AddInkStroke(
    annot: *mut c_void,
    points: *const FS_POINTF,
    count: usize,
) -> c_int {
    if points.is_null() || count == 0 || count > c_int::MAX as usize {
        return -1;
    }
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return -1;
    };
    let points = unsafe { std::slice::from_raw_parts(points, count) }
        .iter()
        .map(|p| CfxPointF::new(p.x, p.y));
    sdk::add_ink_stroke(&dict, points).unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_RemoveInkList(annot: *mut c_void) -> c_int {
    unsafe { annotation_dict(annot) }.is_some_and(|d| sdk::remove_ink_list(&d)) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetColor(
    annot: *mut c_void,
    kind: c_int,
    r: u32,
    g: u32,
    b: u32,
    a: u32,
) -> c_int {
    unsafe { annotation_dict(annot) }.is_some_and(|d| sdk::set_color(&d, kind == 1, [r, g, b, a]))
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetColor(
    annot: *mut c_void,
    kind: c_int,
    r: *mut u32,
    g: *mut u32,
    b: *mut u32,
    a: *mut u32,
) -> c_int {
    if r.is_null() || g.is_null() || b.is_null() || a.is_null() {
        return 0;
    }
    let Some(values) = (unsafe { annotation_dict(annot) }).and_then(|d| sdk::color(&d, kind == 1))
    else {
        return 0;
    };
    unsafe {
        *r = values[0];
        *g = values[1];
        *b = values[2];
        *a = values[3];
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_HasAttachmentPoints(annot: *mut c_void) -> c_int {
    unsafe { annotation_dict(annot) }.is_some_and(|d| sdk::has_attachment_points(&d)) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_CountAttachmentPoints(annot: *mut c_void) -> usize {
    unsafe { annotation_dict(annot) }.map_or(0, |d| sdk::attachment_points_count(&d))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetAttachmentPoints(
    annot: *mut c_void,
    index: usize,
    quad: *const FS_QUADPOINTSF,
) -> c_int {
    let Some(quad) = (unsafe { quad.as_ref() }) else {
        return 0;
    };
    unsafe { annotation_dict(annot) }
        .is_some_and(|d| sdk::set_attachment_points(&d, Some(index), quad_values(quad)))
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_AppendAttachmentPoints(
    annot: *mut c_void,
    quad: *const FS_QUADPOINTSF,
) -> c_int {
    let Some(quad) = (unsafe { quad.as_ref() }) else {
        return 0;
    };
    unsafe { annotation_dict(annot) }
        .is_some_and(|d| sdk::set_attachment_points(&d, None, quad_values(quad))) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetAttachmentPoints(
    annot: *mut c_void,
    index: usize,
    quad: *mut FS_QUADPOINTSF,
) -> c_int {
    if quad.is_null() {
        return 0;
    }
    let Some(values) =
        (unsafe { annotation_dict(annot) }).and_then(|d| sdk::attachment_points(&d, index))
    else {
        return 0;
    };
    unsafe {
        *quad = quad_from_values(values);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetRect(annot: *mut c_void, rect: *const FS_RECTF) -> c_int {
    let Some(rect) = (unsafe { rect.as_ref() }) else {
        return 0;
    };
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    sdk::set_rect(
        &dict,
        CfxFloatRect::new(rect.left, rect.bottom, rect.right, rect.top),
    );
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetRect(annot: *mut c_void, rect: *mut FS_RECTF) -> c_int {
    #[cfg(feature = "xfa")]
    if let Some(widget) =
        (unsafe { annot.cast::<AnnotHandle>().as_ref() }).and_then(AnnotHandle::xfa_widget)
    {
        let Some(rect) = (unsafe { rect.as_mut() }) else {
            return 0;
        };
        let bbox = widget.rect();
        *rect = FS_RECTF {
            left: bbox.left,
            top: bbox.top,
            right: bbox.right,
            bottom: bbox.bottom,
        };
        return 1;
    }

    if rect.is_null() {
        return 0;
    }
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    let r = sdk::rect(&dict);
    unsafe {
        *rect = FS_RECTF {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        };
    }
    1
}
unsafe fn copy_points(
    points: Option<Vec<CfxPointF>>,
    buffer: *mut FS_POINTF,
    length: c_ulong,
) -> c_ulong {
    let Some(points) = points else {
        return 0;
    };
    if !buffer.is_null() && length as usize >= points.len() {
        for (i, p) in points.iter().enumerate() {
            unsafe {
                *buffer.add(i) = FS_POINTF { x: p.x, y: p.y };
            }
        }
    }
    points.len() as c_ulong
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetVertices(
    annot: *mut c_void,
    buffer: *mut FS_POINTF,
    length: c_ulong,
) -> c_ulong {
    unsafe {
        copy_points(
            annotation_dict(annot).and_then(|d| sdk::vertices(&d)),
            buffer,
            length,
        )
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetInkListCount(annot: *mut c_void) -> c_ulong {
    unsafe { annotation_dict(annot) }.map_or(0, |d| sdk::ink_list_count(&d) as c_ulong)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetInkListPath(
    annot: *mut c_void,
    index: c_ulong,
    buffer: *mut FS_POINTF,
    length: c_ulong,
) -> c_ulong {
    unsafe {
        copy_points(
            annotation_dict(annot).and_then(|d| sdk::ink_list_path(&d, index as usize)),
            buffer,
            length,
        )
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetLine(
    annot: *mut c_void,
    start: *mut FS_POINTF,
    end: *mut FS_POINTF,
) -> c_int {
    if start.is_null() || end.is_null() {
        return 0;
    }
    let Some(v) = (unsafe { annotation_dict(annot) }).and_then(|d| sdk::line(&d)) else {
        return 0;
    };
    unsafe {
        *start = FS_POINTF {
            x: v[0].x,
            y: v[0].y,
        };
        *end = FS_POINTF {
            x: v[1].x,
            y: v[1].y,
        };
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetBorder(
    annot: *mut c_void,
    h: f32,
    v: f32,
    width: f32,
) -> c_int {
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    sdk::set_border(&dict, [h, v, width]);
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetBorder(
    annot: *mut c_void,
    h: *mut f32,
    v: *mut f32,
    width: *mut f32,
) -> c_int {
    if h.is_null() || v.is_null() || width.is_null() {
        return 0;
    }
    let Some(values) = (unsafe { annotation_dict(annot) }).and_then(|d| sdk::border(&d)) else {
        return 0;
    };
    unsafe {
        *h = values[0];
        *v = values[1];
        *width = values[2];
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_HasKey(annot: *mut c_void, key: *const c_char) -> c_int {
    unsafe { annotation_dict(annot) }.is_some_and(|d| sdk::has_key(&d, unsafe { byte_string(key) }))
        as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetValueType(annot: *mut c_void, key: *const c_char) -> c_int {
    unsafe { annotation_dict(annot) }
        .and_then(|d| sdk::value_type(&d, unsafe { byte_string(key) }))
        .map_or(0, |k| k as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetStringValue(
    annot: *mut c_void,
    key: *const c_char,
    value: *const u16,
) -> c_int {
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    sdk::set_string(&dict, unsafe { byte_string(key) }, &unsafe {
        wide_string(value)
    });
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetStringValue(
    annot: *mut c_void,
    key: *const c_char,
    buffer: *mut u16,
    length: c_ulong,
) -> c_ulong {
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    unsafe { copy_wide(&sdk::string(&dict, byte_string(key)), buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetNumberValue(
    annot: *mut c_void,
    key: *const c_char,
    value: *mut f32,
) -> c_int {
    if value.is_null() {
        return 0;
    }
    let Some(result) = (unsafe { annotation_dict(annot) })
        .and_then(|d| sdk::number(&d, unsafe { byte_string(key) }))
    else {
        return 0;
    };
    unsafe {
        *value = result;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetAP(
    annot: *mut c_void,
    mode: c_int,
    value: *const u16,
) -> c_int {
    if !(0..3).contains(&mode) {
        return 0;
    }
    let Some(pointer) = NonNull::new(annot.cast::<AnnotHandle>()) else {
        return 0;
    };
    let Some(dict) = (unsafe { pointer.as_ref() }).pdf_dict() else {
        return 0;
    };
    let page = unsafe { (*pointer.as_ptr()).page };
    let objects = unsafe { &(*page.as_ptr())._document }.borrow().holder();
    let value = (!value.is_null()).then(|| unsafe { wide_string(value) });
    sdk::set_appearance(&dict, &objects, mode, value.as_deref()) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetAP(
    annot: *mut c_void,
    mode: c_int,
    buffer: *mut u16,
    length: c_ulong,
) -> c_ulong {
    let Some(text) =
        (unsafe { annotation_dict(annot) }).and_then(|d| sdk::appearance_text(&d, mode))
    else {
        return 0;
    };
    unsafe { copy_wide(&text, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetLinkedAnnot(
    annot: *mut c_void,
    key: *const c_char,
) -> *mut c_void {
    let Some(pointer) = NonNull::new(annot.cast::<AnnotHandle>()) else {
        return std::ptr::null_mut();
    };
    let Some(dict) = (unsafe { pointer.as_ref() })
        .pdf_dict()
        .and_then(|dict| sdk::linked_annotation(&dict, unsafe { byte_string(key) }))
    else {
        return std::ptr::null_mut();
    };
    new_annotation(dict, unsafe { (*pointer.as_ptr()).page })
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFlags(annot: *mut c_void) -> c_int {
    unsafe { annotation_dict(annot) }.map_or(0, |d| sdk::flags(&d))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetFlags(annot: *mut c_void, flags: c_int) -> c_int {
    let Some(dict) = (unsafe { annotation_dict(annot) }) else {
        return 0;
    };
    sdk::set_flags(&dict, flags);
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetURI(annot: *mut c_void, uri: *const c_char) -> c_int {
    if uri.is_null() {
        return 0;
    }
    unsafe { annotation_dict(annot) }.is_some_and(|d| sdk::set_uri(&d, unsafe { byte_string(uri) }))
        as c_int
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn annotation_handles_alias_objects_and_preserve_buffer_contracts() {
        unsafe {
            let document = FPDF_CreateNewDocument();
            let page = FPDFPage_New(document, 0, 100., 100.);
            assert_eq!(FPDFPage_GetAnnotCount(page), 0);
            assert!(FPDFPage_CreateAnnot(page, 4).is_null());
            let stamp = FPDFPage_CreateAnnot(page, 13);
            assert!(!stamp.is_null());
            let rect = FS_RECTF {
                left: 0.,
                top: 60.,
                right: 60.,
                bottom: 0.,
            };
            assert_eq!(FPDFAnnot_SetRect(stamp, &rect), 1);
            assert_eq!(FPDFAnnot_GetSubtype(stamp), 13);
            let object = FPDFPageObj_CreateNewRect(1., 2., 20., 30.);
            assert_eq!(FPDFPath_SetDrawMode(object, 2, 0), 1);
            assert_eq!(FPDFPageObj_SetFillColor(object, 255, 0, 0, 255), 1);
            assert_eq!(FPDFAnnot_AppendObject(stamp, object), 1);
            assert_eq!(FPDFAnnot_GetObjectCount(stamp), 1);
            assert_eq!(FPDFAnnot_GetObject(stamp, 0), object);
            assert_eq!(FPDFAnnot_GetObject(stamp, 0), object);
            assert!(FPDFAnnot_GetObject(stamp, -1).is_null());
            assert_eq!(FPDFAnnot_AppendObject(stamp, object), 0);
            assert_eq!(FPDFPageObj_SetFillColor(object, 0, 0, 255, 255), 1);
            assert_eq!(FPDFAnnot_UpdateObject(stamp, object), 1);
            assert_eq!(FPDFAnnot_RemoveObject(stamp, 0), 1);
            assert_eq!(FPDFAnnot_GetObjectCount(stamp), 0);
            assert_eq!(FPDFAnnot_RemoveObject(stamp, 0), 0);
            let label = [0x41u16, 0xd83d, 0xde42, 0];
            assert_eq!(
                FPDFAnnot_SetStringValue(stamp, c"Contents".as_ptr(), label.as_ptr()),
                1
            );
            assert_eq!(
                FPDFAnnot_GetStringValue(stamp, c"Contents".as_ptr(), std::ptr::null_mut(), 0),
                8
            );
            let mut short = [0x7777u16; 3];
            assert_eq!(
                FPDFAnnot_GetStringValue(stamp, c"Contents".as_ptr(), short.as_mut_ptr(), 6),
                8
            );
            assert_eq!(short, [0x7777; 3]);
            let mut full = [0u16; 5];
            assert_eq!(
                FPDFAnnot_GetStringValue(stamp, c"Contents".as_ptr(), full.as_mut_ptr(), 10),
                8
            );
            assert_eq!(&full[..4], &label);
            let alias = FPDFPage_GetAnnot(page, 0);
            assert!(!alias.is_null());
            assert_eq!(FPDFPage_GetAnnotIndex(page, alias), 0);
            assert_eq!(FPDFPage_RemoveAnnot(page, 0), 1);
            assert_eq!(FPDFPage_GetAnnotIndex(page, stamp), -1);
            assert_eq!(FPDFAnnot_GetSubtype(alias), 13);
            FPDFPage_CloseAnnot(alias);
            FPDFPage_CloseAnnot(stamp);
            FPDF_ClosePage(page);
            FPDF_CloseDocument(document);
        }
    }
}
