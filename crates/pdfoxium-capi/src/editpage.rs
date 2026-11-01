// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Page lifecycle and object ownership from `fpdfsdk/fpdf_editpage.cpp`.
use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_formobject::CpdfFormObject;
use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;

/// # Safety
/// Document is null or live and available for exclusive mutation. Pages being
/// removed must have been closed by the caller.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_Delete(document: *mut c_void, index: c_int) {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return;
    };
    let owner = document.document.clone();
    #[cfg(feature = "xfa")]
    let extension = document.xfa_context.clone();
    #[cfg(feature = "xfa")]
    let number = if let Some(extension) = extension {
        extension.delete_page(index)
    } else {
        owner.borrow_mut().delete_page(index)
    };
    #[cfg(not(feature = "xfa"))]
    let number = owner.borrow_mut().delete_page(index);
    owner.borrow_mut().set_page_to_null_object(number);
}

/// # Safety
/// Document is null or live and exclusively accessible. A nonempty indices
/// array contains at least length readable integers.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_MovePages(
    document: *mut c_void,
    indices: *const c_int,
    length: c_ulong,
    destination: c_int,
) -> c_int {
    let Some(document) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let Ok(length) = usize::try_from(length) else {
        return 0;
    };
    if length == 0
        || indices.is_null()
        || length > isize::MAX as usize / std::mem::size_of::<c_int>()
    {
        return 0;
    }
    let indices = unsafe { std::slice::from_raw_parts(indices, length) };
    document
        .document
        .borrow_mut()
        .move_pages(indices, destination) as c_int
}

/// # Safety
/// Page is null or a live page handle.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetRotation(page: *mut c_void) -> c_int {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return -1;
    };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return -1;
    }
    page.pdf_page().unwrap().rotation()
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetRotation(page: *mut c_void, rotation: c_int) {
    let Some(page) = (unsafe { pdf_handle_mut(page) }) else {
        return;
    };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return;
    }
    let Some(mut engine_page) = page.page_mut() else {
        return;
    };
    engine_page
        .dict()
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .set_new_for(
            b"Rotate",
            CpdfScalarObject::number_from_signed((rotation % 4) * 90),
        );
    engine_page.update_dimensions();
}

/// # Safety
/// Page is null or live and available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_TransformAnnots(
    page: *mut c_void,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) {
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return;
    };
    fpdf_editpage::transform_annots(
        page.pdf_page().unwrap(),
        pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix::new(
            a as f32, b as f32, c as f32, d as f32, e as f32, f as f32,
        ),
    );
}

/// # Safety
/// Object is null or live; active is null or writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetIsActive(object: *mut c_void, active: *mut c_int) -> c_int {
    if active.is_null() {
        return 0;
    }
    let Some(value) = (unsafe { with_page_object(object, |o| o.base().is_active()) }) else {
        return 0;
    };
    unsafe {
        *active = value as c_int;
    }
    1
}

/// # Safety
/// Object is null or live and available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_SetIsActive(object: *mut c_void, active: c_int) -> c_int {
    unsafe { with_page_object(object, |o| o.base_mut().set_is_active(active != 0)) }.is_some()
        as c_int
}

/// # Safety
/// Page is null or live. Object is null or caller-owned, and its ownership
/// transfers even on failure, matching FPDFPage_InsertObject.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_InsertObjectAtIndex(
    page: *mut c_void,
    object: *mut c_void,
    index: usize,
) -> c_int {
    if object.is_null() {
        return 0;
    }
    let mut handle = unsafe { Box::from_raw(object.cast::<PageObjectHandle>()) };
    let ObjectLocation::Owned(mut engine_object) =
        std::mem::replace(&mut handle.location, ObjectLocation::Transferring)
    else {
        panic!("insert requires caller-owned object");
    };
    let Some(pointer) = NonNull::new(page.cast::<PageHandle>()) else {
        return 0;
    };
    let page = unsafe { &mut *pointer.as_ptr() };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return 0;
    }
    engine_object.base_mut().set_dirty(true);
    fpdf_editpage::calc_bounding_box(engine_object.as_mut());
    let Some(mut engine_page) = page.page_mut() else {
        return 0;
    };
    let holder = engine_page.mutable_holder();
    if !holder.insert_page_object_at_index(index, engine_object) {
        return 0;
    }
    let id = holder.page_object_id_by_index(index).unwrap();
    handle.location = ObjectLocation::Attached {
        page: pointer,
        index,
        id,
    };
    drop(engine_page);
    page.object_handles.insert(id, handle);
    1
}

/// # Safety
/// Page and object are null or live. A successfully removed object's handle is
/// now caller-owned and must be inserted again or destroyed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_RemoveObject(page: *mut c_void, object: *mut c_void) -> c_int {
    let Some(pointer) = NonNull::new(page.cast::<PageHandle>()) else {
        return 0;
    };
    let Some(object_pointer) = NonNull::new(object.cast::<PageObjectHandle>()) else {
        return 0;
    };
    let id = match unsafe { &(*object_pointer.as_ptr()).location } {
        ObjectLocation::Attached { page, id, .. } if *page == pointer => *id,
        _ => return 0,
    };
    let page = unsafe { &mut *pointer.as_ptr() };
    if !page
        .pdf_page()
        .is_some_and(|page| CpdfPage::is_valid_page_dict(&page.dict()))
    {
        return 0;
    }
    let Some(mut engine_page) = page.page_mut() else {
        return 0;
    };
    let Some(engine_object) = engine_page.mutable_holder().remove_page_object(id) else {
        return 0;
    };
    drop(engine_page);
    let mut handle = page
        .object_handles
        .remove(&id)
        .expect("attached object has a public handle");
    handle.location = ObjectLocation::Owned(engine_object);
    let detached = Box::into_raw(handle);
    debug_assert_eq!(detached, object_pointer.as_ptr());
    1
}

/// # Safety
/// Form is null or a live object handle. The result borrows the form until
/// explicitly removed, and may not be destroyed while attached.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFFormObj_GetObject(form: *mut c_void, index: c_ulong) -> *mut c_void {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let id = unsafe {
        with_page_object(form, |object| {
            object
                .as_any()
                .downcast_ref::<CpdfFormObject>()?
                .form()
                .holder()
                .page_object_id_by_index(index)
        })
    }
    .flatten();
    let Some(id) = id else {
        return std::ptr::null_mut();
    };
    let pointer = NonNull::new(form.cast::<PageObjectHandle>()).unwrap();
    // Borrow only the alias map after resolving the engine holder.
    let handles = unsafe { &mut (*pointer.as_ptr()).child_handles };
    let handle = handles.entry(id).or_insert_with(|| {
        Box::new(PageObjectHandle::new(ObjectLocation::InForm {
            form: pointer,
            index,
            id,
        }))
    });
    (&mut **handle as *mut PageObjectHandle).cast()
}

/// # Safety
/// Both objects are null or live, with form accessible exclusively. On success,
/// the removed child's existing handle becomes caller-owned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFFormObj_RemoveObject(form: *mut c_void, object: *mut c_void) -> c_int {
    let Some(form_pointer) = NonNull::new(form.cast::<PageObjectHandle>()) else {
        return 0;
    };
    let Some(object_pointer) = NonNull::new(object.cast::<PageObjectHandle>()) else {
        return 0;
    };
    let id = match unsafe { &(*object_pointer.as_ptr()).location } {
        ObjectLocation::InForm { form, id, .. } if *form == form_pointer => *id,
        _ => return 0,
    };
    let removed = unsafe {
        with_page_object(form, |object| {
            let form = object.as_any_mut().downcast_mut::<CpdfFormObject>()?;
            let child = form.form_mut().holder_mut().remove_page_object(id)?;
            form.base_mut().set_dirty(true);
            Some(child)
        })
    }
    .flatten();
    let Some(removed) = removed else {
        return 0;
    };
    let handles = unsafe { &mut (*form_pointer.as_ptr()).child_handles };
    let mut handle = handles
        .remove(&id)
        .expect("attached child has a public handle");
    handle.location = ObjectLocation::Owned(removed);
    let detached = Box::into_raw(handle);
    debug_assert_eq!(detached, object_pointer.as_ptr());
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ownership_and_aliases_survive_insertion_removal_and_index_shifts() {
        unsafe {
            let doc = FPDF_CreateNewDocument();
            let page = FPDFPage_New(doc, 0, 100., 200.);
            let a = FPDFPageObj_CreateNewRect(1., 2., 3., 4.);
            let b = FPDFPageObj_CreateNewRect(5., 6., 7., 8.);
            assert_eq!(FPDFPath_SetDrawMode(b, 2, 0), 1);
            assert_eq!(FPDFPage_InsertObject(page, a), 1);
            assert_eq!(FPDFPage_InsertObjectAtIndex(page, b, 0), 1);
            assert_eq!(FPDFPage_GetObject(page, 0), b);
            assert_eq!(FPDFPage_GetObject(page, 1), a);
            assert_eq!(FPDFPageObj_GetType(a), 2);
            assert_eq!(FPDFPage_RemoveObject(page, b), 1);
            assert_eq!(FPDFPage_GetObject(page, 0), a);
            assert_eq!(FPDFPageObj_GetType(a), 2);
            assert_eq!(FPDFPage_RemoveObject(page, b), 0);
            assert_eq!(FPDFPage_InsertObjectAtIndex(page, b, 1), 1);
            assert_eq!(FPDFPage_RemoveObject(page, a), 1);
            FPDFPageObj_Destroy(a);
            let mut active = -1;
            assert_eq!(FPDFPageObj_SetIsActive(b, 0), 1);
            assert_eq!(FPDFPageObj_GetIsActive(b, &mut active), 1);
            assert_eq!(active, 0);
            assert_eq!(FPDFPageObj_SetIsActive(b, 1), 1);
            assert_eq!(FPDFPage_GenerateContent(page), 1);
            let context = FPDF_NewXObjectFromPage(doc, doc, 0);
            assert!(!context.is_null());
            let form = FPDF_NewFormObjectFromXObject(context);
            assert_eq!(FPDFFormObj_CountObjects(form), 1);
            let child = FPDFFormObj_GetObject(form, 0);
            assert!(!child.is_null());
            assert_eq!(FPDFFormObj_GetObject(form, 0), child);
            assert_eq!(FPDFPageObj_GetType(child), 2);
            assert_eq!(FPDFFormObj_RemoveObject(form, child), 1);
            assert_eq!(FPDFFormObj_CountObjects(form), 0);
            assert_eq!(FPDFPage_InsertObject(page, child), 1);
            FPDFPageObj_Destroy(form);
            FPDF_CloseXObject(context);
            assert_eq!(FPDFPageObj_GetType(child), 2);
            FPDFPage_SetRotation(page, -1);
            assert_eq!(FPDFPage_GetRotation(page), 3);
            assert_eq!(FPDF_GetPageWidthF(page), 200.);
            assert_eq!(FPDF_GetPageHeightF(page), 100.);
            FPDF_ClosePage(page);
            FPDFPage_Delete(doc, 0);
            assert_eq!(FPDF_GetPageCount(doc), 0);
            FPDF_CloseDocument(doc);
        }
    }
}
