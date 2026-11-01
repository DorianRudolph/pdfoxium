// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Complete `fpdfsdk/fpdf_structtree.cpp` public ABI. A tree owns stable boxed
//! element/attribute handles; their owner links are weak, avoiding cache cycles.
use super::*;
use pdfoxium_core::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray,
        cpdf_object::{get_direct_object, ObjectRef},
        cpdf_scalar_object::ObjectType,
    },
    fpdfdoc::{cpdf_structelement::CpdfStructElement, cpdf_structtree::CpdfStructTree},
    fxcrt::fx_string::utf8_decode,
};
use std::{collections::HashMap, rc::Weak};

struct TreeHandle {
    tree: Rc<CpdfStructTree>,
    _document: Rc<RefCell<CpdfDocument>>,
    elements: RefCell<HashMap<usize, Box<ElementHandle>>>,
    attributes: RefCell<HashMap<usize, Box<AttributeHandle>>>,
}
struct ElementHandle {
    element: Rc<CpdfStructElement>,
    owner: Weak<TreeHandle>,
}
struct AttributeHandle {
    object: ObjectRef,
    owner: Weak<TreeHandle>,
}
impl TreeHandle {
    fn element(self: &Rc<Self>, element: Option<Rc<CpdfStructElement>>) -> *mut c_void {
        let Some(element) = element else {
            return std::ptr::null_mut();
        };
        let key = Rc::as_ptr(&element) as usize;
        let mut cache = self.elements.borrow_mut();
        let handle = cache.entry(key).or_insert_with(|| {
            Box::new(ElementHandle {
                element,
                owner: Rc::downgrade(self),
            })
        });
        (&mut **handle as *mut ElementHandle).cast()
    }
    fn attribute(self: &Rc<Self>, object: Option<ObjectRef>) -> *mut c_void {
        let Some(object) = object else {
            return std::ptr::null_mut();
        };
        let key = Rc::as_ptr(&object) as *const () as usize;
        let mut cache = self.attributes.borrow_mut();
        let handle = cache.entry(key).or_insert_with(|| {
            Box::new(AttributeHandle {
                object,
                owner: Rc::downgrade(self),
            })
        });
        (&mut **handle as *mut AttributeHandle).cast()
    }
}
unsafe fn tree(handle: *mut c_void) -> Option<Rc<TreeHandle>> {
    Some(unsafe { handle.cast::<Rc<TreeHandle>>().as_ref()? }.clone())
}
unsafe fn element(handle: *mut c_void) -> Option<(Rc<TreeHandle>, Rc<CpdfStructElement>)> {
    let handle = unsafe { handle.cast::<ElementHandle>().as_ref()? };
    Some((handle.owner.upgrade()?, handle.element.clone()))
}
unsafe fn attribute(handle: *mut c_void) -> Option<(Rc<TreeHandle>, ObjectRef)> {
    let handle = unsafe { handle.cast::<AttributeHandle>().as_ref()? };
    Some((handle.owner.upgrade()?, handle.object.clone()))
}
fn raw(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
fn array_item(object: &ObjectRef, index: usize) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()?
        .get_object_at(index)
}
fn size(object: &ObjectRef) -> c_int {
    let object = object.borrow();
    let count = match object.object_type() {
        ObjectType::Array => object.as_any().downcast_ref::<CpdfArray>().unwrap().size(),
        ObjectType::Dictionary => object
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .size(),
        _ => return -1,
    };
    c_int::try_from(count).unwrap_or(-1)
}
unsafe fn wide_result(text: &[u32], buffer: *mut c_void, length: c_ulong) -> c_ulong {
    if text.is_empty() {
        return 0;
    }
    unsafe { doc::copy_utf16(text, buffer, length) }
}
unsafe fn text_result(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
    getter: fn(&CpdfStructElement) -> Vec<u32>,
) -> c_ulong {
    let Some((_owner, element)) = (unsafe { element(handle) }) else {
        return 0;
    };
    unsafe { wide_result(&getter(&element), buffer, length) }
}
unsafe fn name_result(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
    getter: fn(&CpdfStructElement) -> Vec<u8>,
) -> c_ulong {
    let Some((_owner, element)) = (unsafe { element(handle) }) else {
        return 0;
    };
    unsafe { wide_result(&utf8_decode(&getter(&element)), buffer, length) }
}

#[no_mangle]
pub unsafe extern "C" fn FPDF_StructTree_GetForPage(page: *mut c_void) -> *mut c_void {
    let (document, dictionary) = {
        let Some(page) = (unsafe { pdf_handle(page) }) else {
            return std::ptr::null_mut();
        };
        (page._document.clone(), page.pdf_page().unwrap().dict())
    };
    let catalog = document.borrow().root();
    let Some(core_tree) =
        catalog.and_then(|root| CpdfStructTree::load_page_from_root(root, dictionary))
    else {
        return std::ptr::null_mut();
    };
    let owner = Rc::new(TreeHandle {
        tree: core_tree,
        _document: document,
        elements: RefCell::new(HashMap::new()),
        attributes: RefCell::new(HashMap::new()),
    });
    Box::into_raw(Box::new(owner)).cast()
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructTree_Close(handle: *mut c_void) {
    if !handle.is_null() {
        drop(unsafe { Box::from_raw(handle.cast::<Rc<TreeHandle>>()) });
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructTree_CountChildren(handle: *mut c_void) -> c_int {
    unsafe { tree(handle) }
        .and_then(|owner| c_int::try_from(owner.tree.count_top_elements()).ok())
        .unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructTree_GetChildAtIndex(
    handle: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let (Some(owner), Ok(index)) = (unsafe { tree(handle) }, usize::try_from(index)) else {
        return std::ptr::null_mut();
    };
    owner.element(owner.tree.get_top_element(index))
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetAltText(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { text_result(handle, buffer, length, CpdfStructElement::get_alt_text) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetActualText(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { text_result(handle, buffer, length, CpdfStructElement::get_actual_text) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetExpansion(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { text_result(handle, buffer, length, CpdfStructElement::get_expansion) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetTitle(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { text_result(handle, buffer, length, CpdfStructElement::get_title) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetID(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(text) = (unsafe { element(handle) }).and_then(|(_, element)| element.get_id()) else {
        return 0;
    };
    unsafe { doc::copy_utf16(&text, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetLang(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    let Some(text) = (unsafe { element(handle) }).and_then(|(_, element)| element.get_lang())
    else {
        return 0;
    };
    unsafe { doc::copy_utf16(&text, buffer, length) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetRawType(
    handle: *mut c_void,
    buffer: *mut u16,
    length: usize,
) -> c_ulong {
    unsafe {
        name_result(
            handle,
            buffer.cast(),
            c_ulong::try_from(length).unwrap_or(c_ulong::MAX),
            CpdfStructElement::get_raw_type,
        )
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetType(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { name_result(handle, buffer, length, CpdfStructElement::get_type) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetObjType(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    unsafe { name_result(handle, buffer, length, CpdfStructElement::get_obj_type) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_CountChildren(handle: *mut c_void) -> c_int {
    unsafe { element(handle) }
        .and_then(|(_, element)| c_int::try_from(element.count_kids()).ok())
        .unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetChildAtIndex(
    handle: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let (Some((owner, element)), Ok(index)) = (unsafe { element(handle) }, usize::try_from(index))
    else {
        return std::ptr::null_mut();
    };
    owner.element(element.get_kid_if_element(index))
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetChildMarkedContentID(
    handle: *mut c_void,
    index: c_int,
) -> c_int {
    let (Some((_, element)), Ok(index)) = (unsafe { element(handle) }, usize::try_from(index))
    else {
        return -1;
    };
    element.get_kid_content_id(index)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetParent(handle: *mut c_void) -> *mut c_void {
    let Some((owner, element)) = (unsafe { element(handle) }) else {
        return std::ptr::null_mut();
    };
    owner.element(element.get_parent())
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetAttributeCount(handle: *mut c_void) -> c_int {
    let Some(attributes) = (unsafe { element(handle) })
        .and_then(|(_, element)| element.get_a())
        .and_then(|object| get_direct_object(&object))
    else {
        return -1;
    };
    let kind = attributes.borrow().object_type();
    match kind {
        ObjectType::Array => size(&attributes),
        ObjectType::Dictionary => 1,
        _ => -1,
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetAttributeAtIndex(
    handle: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let (Some((owner, element)), Ok(index)) = (unsafe { element(handle) }, usize::try_from(index))
    else {
        return std::ptr::null_mut();
    };
    let Some(attributes) = element
        .get_a()
        .and_then(|object| get_direct_object(&object))
    else {
        return std::ptr::null_mut();
    };
    let kind = attributes.borrow().object_type();
    let result = match kind {
        ObjectType::Dictionary if index == 0 => Some(attributes),
        ObjectType::Array => array_item(&attributes, index)
            .and_then(|object| get_direct_object(&object))
            .filter(|object| object.borrow().object_type() == ObjectType::Dictionary),
        _ => None,
    };
    owner.attribute(result)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetStringAttribute(
    handle: *mut c_void,
    name: *const c_char,
    buffer: *mut c_void,
    length: c_ulong,
) -> c_ulong {
    if name.is_null() {
        return 0;
    }
    let Some(attributes) = (unsafe { element(handle) }).and_then(|(_, element)| element.get_a())
    else {
        return 0;
    };
    if attributes.borrow().object_type() != ObjectType::Array {
        return 0;
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    for index in 0..size(&attributes).max(0) as usize {
        let Some(value) = array_item(&attributes, index).and_then(|object| raw(&object, name))
        else {
            continue;
        };
        let value = value.borrow();
        if matches!(value.object_type(), ObjectType::Name | ObjectType::String) {
            return unsafe { doc::copy_utf16(&value.get_unicode_text(), buffer, length) };
        }
    }
    0
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetMarkedContentID(handle: *mut c_void) -> c_int {
    let Some(object) = (unsafe { element(handle) })
        .and_then(|(_, element)| element.get_k())
        .and_then(|object| get_direct_object(&object))
    else {
        return -1;
    };
    let object = object.borrow();
    if object.object_type() == ObjectType::Number {
        object.get_integer()
    } else {
        -1
    }
}
fn mcid(object: &ObjectRef) -> c_int {
    let Some(kind) = raw(object, b"Type").and_then(|value| get_direct_object(&value)) else {
        return -1;
    };
    if kind.borrow().object_type() != ObjectType::Name || kind.borrow().get_string() != b"MCR" {
        return -1;
    }
    let Some(number) = raw(object, b"MCID").and_then(|value| get_direct_object(&value)) else {
        return -1;
    };
    let number = number.borrow();
    if number.object_type() == ObjectType::Number {
        number.get_integer()
    } else {
        -1
    }
}
// /K can mix child structure elements, object references and marked content.
// The public ID count/index operate on IDs, not raw kid-array positions.
fn marked_content_ids(object: &ObjectRef) -> impl Iterator<Item = c_int> {
    let object = get_direct_object(object);
    let is_array = object
        .as_ref()
        .is_some_and(|value| value.borrow().object_type() == ObjectType::Array);
    let count = if is_array {
        size(object.as_ref().unwrap()).max(0) as usize
    } else {
        1
    };
    (0..count).filter_map(move |index| {
        let object = object.as_ref()?;
        let value = if is_array {
            array_item(object, index)?
        } else {
            object.clone()
        };
        let value = get_direct_object(&value)?;
        let kind = value.borrow().object_type();
        let id = match kind {
            ObjectType::Number => value.borrow().get_integer(),
            ObjectType::Dictionary => mcid(&value),
            _ => -1,
        };
        (id >= 0).then_some(id)
    })
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetMarkedContentIdCount(handle: *mut c_void) -> c_int {
    let Some(object) = (unsafe { element(handle) }).and_then(|(_, element)| element.get_k()) else {
        return -1;
    };
    let count = marked_content_ids(&object).count();
    if count == 0 {
        -1
    } else {
        c_int::try_from(count).unwrap_or(-1)
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_GetMarkedContentIdAtIndex(
    handle: *mut c_void,
    index: c_int,
) -> c_int {
    let (Some((_, element)), Ok(index)) = (unsafe { element(handle) }, usize::try_from(index))
    else {
        return -1;
    };
    let Some(object) = element.get_k() else {
        return -1;
    };
    let id = marked_content_ids(&object).nth(index).unwrap_or(-1);
    id
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetCount(handle: *mut c_void) -> c_int {
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return -1;
    };
    if object.borrow().object_type() != ObjectType::Dictionary {
        return -1;
    }
    size(&object)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetName(
    handle: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    length: c_ulong,
    out_length: *mut c_ulong,
) -> c_int {
    if out_length.is_null() {
        return 0;
    }
    let (Some((_, object)), Ok(index)) = (unsafe { attribute(handle) }, usize::try_from(index))
    else {
        return 0;
    };
    let key = object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .and_then(|dictionary| {
            dictionary.with_locked_entries(|entries| entries.keys().nth(index).cloned())
        });
    let Some(key) = key else { return 0 };
    unsafe {
        *out_length = doc::copy_terminated(&key, buffer, length);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetValue(
    handle: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let Some((owner, object)) = (unsafe { attribute(handle) }) else {
        return std::ptr::null_mut();
    };
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    owner.attribute(raw(&object, name).and_then(|object| get_direct_object(&object)))
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetType(handle: *mut c_void) -> c_int {
    unsafe { attribute(handle) }.map_or(0, |(_, object)| object.borrow().object_type() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetBooleanValue(
    handle: *mut c_void,
    output: *mut c_int,
) -> c_int {
    if output.is_null() {
        return 0;
    }
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return 0;
    };
    let object = object.borrow();
    if object.object_type() != ObjectType::Boolean {
        return 0;
    }
    unsafe {
        *output = object.get_integer();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetNumberValue(
    handle: *mut c_void,
    output: *mut f32,
) -> c_int {
    if output.is_null() {
        return 0;
    }
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return 0;
    };
    let object = object.borrow();
    if object.object_type() != ObjectType::Number {
        return 0;
    }
    unsafe {
        *output = object.get_number();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetStringValue(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
    out_length: *mut c_ulong,
) -> c_int {
    if out_length.is_null() {
        return 0;
    }
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return 0;
    };
    let text = {
        let object = object.borrow();
        if !matches!(object.object_type(), ObjectType::Name | ObjectType::String) {
            return 0;
        }
        utf8_decode(&object.get_string())
    };
    unsafe {
        *out_length = doc::copy_utf16(&text, buffer, length);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetBlobValue(
    handle: *mut c_void,
    buffer: *mut c_void,
    length: c_ulong,
    out_length: *mut c_ulong,
) -> c_int {
    if out_length.is_null() {
        return 0;
    }
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return 0;
    };
    let bytes = {
        let object = object.borrow();
        if object.object_type() != ObjectType::String {
            return 0;
        }
        object.get_string()
    };
    unsafe {
        *out_length = doc::copy_bytes(&bytes, buffer, length);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_CountChildren(handle: *mut c_void) -> c_int {
    let Some((_, object)) = (unsafe { attribute(handle) }) else {
        return -1;
    };
    if object.borrow().object_type() != ObjectType::Array {
        return -1;
    }
    size(&object)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_StructElement_Attr_GetChildAtIndex(
    handle: *mut c_void,
    index: c_int,
) -> *mut c_void {
    let (Some((owner, object)), Ok(index)) = (unsafe { attribute(handle) }, usize::try_from(index))
    else {
        return std::ptr::null_mut();
    };
    // The public attribute-value API promises that references are dereferenced,
    // including array children (the native raw-child conversion misses this).
    owner.attribute(array_item(&object, index).and_then(|object| get_direct_object(&object)))
}

#[cfg(test)]
#[path = "structtree_tests.rs"]
mod tests;
