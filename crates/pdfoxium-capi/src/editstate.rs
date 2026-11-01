// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Graphics-state and geometry C ABI portions of fpdfsdk/fpdf_editpage.cpp.
//! Raw pointers refer only to public handles and caller-provided buffers.

use super::*;
use pdfoxium_core::fpdfsdk::fpdf_editstate as edit;
use pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix;
use std::mem::size_of;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(non_camel_case_types)]
pub struct FS_QUADPOINTSF {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
    pub x3: f32,
    pub y3: f32,
    pub x4: f32,
    pub y4: f32,
}

/// # Safety
/// A non-null page must be a live page handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPage_HasTransparency(page: *mut c_void) -> c_int {
    if page.is_null() {
        return 0;
    }
    // SAFETY: the public API requires a live page.
    unsafe { pdf_handle(page) }
        .is_some_and(|page| page.pdf_page().unwrap().holder().background_alpha_needed())
        as c_int
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_HasTransparency(object: *mut c_void) -> c_int {
    unsafe { with_page_object(object, |object| edit::has_transparency(object)) }.unwrap_or(false)
        as c_int
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_Transform(
    object: *mut c_void,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) {
    let matrix = FS_MATRIX {
        a: a as f32,
        b: b as f32,
        c: c as f32,
        d: d as f32,
        e: e as f32,
        f: f as f32,
    };
    unsafe { FPDFPageObj_TransformF(object, &matrix) };
}

/// # Safety
/// Non-null object and matrix pointers must be live and readable, respectively.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_TransformF(
    object: *mut c_void,
    matrix: *const FS_MATRIX,
) -> c_int {
    let Some(matrix) = (unsafe { matrix.as_ref() }) else {
        return 0;
    };
    let matrix = CfxMatrix::new(matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f);
    unsafe { with_page_object(object, |object| object.transform(matrix)) }.is_some() as c_int
}

/// # Safety
/// A non-null object must be live; matrix must point to writable output storage.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetMatrix(
    object: *mut c_void,
    matrix: *mut FS_MATRIX,
) -> c_int {
    if matrix.is_null() {
        return 0;
    }
    let Some(value) =
        (unsafe { with_page_object(object, |object| edit::object_matrix(object)) }).flatten()
    else {
        return 0;
    };
    // SAFETY: the caller supplies a writable FS_MATRIX.
    unsafe {
        matrix.write(FS_MATRIX {
            a: value.a,
            b: value.b,
            c: value.c,
            d: value.d,
            e: value.e,
            f: value.f,
        });
    }
    1
}

/// # Safety
/// A non-null object must be live and blend_mode must be a null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetBlendMode(object: *mut c_void, blend_mode: *const c_char) {
    if blend_mode.is_null() {
        return;
    }
    let mode = unsafe { CStr::from_ptr(blend_mode) }.to_bytes();
    unsafe { with_page_object(object, |object| edit::set_blend_mode(object, mode)) };
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetFillColor(
    object: *mut c_void,
    r: u32,
    g: u32,
    b: u32,
    a: u32,
) -> c_int {
    unsafe { with_page_object(object, |object| edit::set_fill_color(object, [r, g, b, a])) }
        .unwrap_or(false) as c_int
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetStrokeColor(
    object: *mut c_void,
    r: u32,
    g: u32,
    b: u32,
    a: u32,
) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            edit::set_stroke_color(object, [r, g, b, a])
        })
    }
    .unwrap_or(false) as c_int
}

unsafe fn get_color(
    object: *mut c_void,
    r: *mut u32,
    g: *mut u32,
    b: *mut u32,
    a: *mut u32,
    stroke: bool,
) -> c_int {
    if r.is_null() || g.is_null() || b.is_null() || a.is_null() {
        return 0;
    }
    let Some(color) = (unsafe {
        with_page_object(object, |object| {
            if stroke {
                edit::stroke_color(object)
            } else {
                edit::fill_color(object)
            }
        })
    })
    .flatten() else {
        return 0;
    };
    // SAFETY: all output pointers are writable; use sequential writes so the
    // public API does not manufacture aliased mutable Rust references.
    unsafe {
        r.write(color[0]);
        g.write(color[1]);
        b.write(color[2]);
        a.write(color[3]);
    }
    1
}

/// # Safety
/// A non-null object must be live and the output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetFillColor(
    object: *mut c_void,
    r: *mut u32,
    g: *mut u32,
    b: *mut u32,
    a: *mut u32,
) -> c_int {
    unsafe { get_color(object, r, g, b, a, false) }
}

/// # Safety
/// A non-null object must be live and the output pointers must be writable.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetStrokeColor(
    object: *mut c_void,
    r: *mut u32,
    g: *mut u32,
    b: *mut u32,
    a: *mut u32,
) -> c_int {
    unsafe { get_color(object, r, g, b, a, true) }
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetStrokeWidth(object: *mut c_void, width: f32) -> c_int {
    unsafe { with_page_object(object, |object| edit::set_stroke_width(object, width)) }
        .unwrap_or(false) as c_int
}

/// # Safety
/// A non-null object must be live and width must point to writable storage.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetStrokeWidth(object: *mut c_void, width: *mut f32) -> c_int {
    if width.is_null() {
        return 0;
    }
    let Some(value) = (unsafe {
        with_page_object(object, |object| {
            object.base().graph_state().get_line_width()
        })
    }) else {
        return 0;
    };
    unsafe { width.write(value) };
    1
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetLineJoin(object: *mut c_void) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            c_int::from(object.base().graph_state().get_line_join().0)
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetLineJoin(object: *mut c_void, line_join: c_int) -> c_int {
    unsafe { with_page_object(object, |object| edit::set_line_join(object, line_join)) }
        .unwrap_or(false) as c_int
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetLineCap(object: *mut c_void) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            c_int::from(object.base().graph_state().get_line_cap().0)
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetLineCap(object: *mut c_void, line_cap: c_int) -> c_int {
    unsafe { with_page_object(object, |object| edit::set_line_cap(object, line_cap)) }
        .unwrap_or(false) as c_int
}

/// # Safety
/// A non-null object must be live and phase must point to writable storage.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetDashPhase(object: *mut c_void, phase: *mut f32) -> c_int {
    if phase.is_null() {
        return 0;
    }
    let Some(value) = (unsafe {
        with_page_object(object, |object| {
            object.base().graph_state().get_line_dash_phase()
        })
    }) else {
        return 0;
    };
    unsafe { phase.write(value) };
    1
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetDashPhase(object: *mut c_void, phase: f32) -> c_int {
    unsafe { with_page_object(object, |object| edit::set_dash_phase(object, phase)) }.is_some()
        as c_int
}

/// # Safety
/// A non-null object must be a live page-object handle.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetDashCount(object: *mut c_void) -> c_int {
    unsafe {
        with_page_object(object, |object| {
            c_int::try_from(object.base().graph_state().get_line_dash_size()).unwrap_or(-1)
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// A non-null object must be live; dash_array must be writable for dash_count floats.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetDashArray(
    object: *mut c_void,
    dash_array: *mut f32,
    dash_count: usize,
) -> c_int {
    if dash_array.is_null() || dash_count > isize::MAX as usize / size_of::<f32>() {
        return 0;
    }
    unsafe {
        with_page_object(object, |object| {
            // SAFETY: the caller supplies a writable array of the stated size.
            let result = std::slice::from_raw_parts_mut(dash_array, dash_count);
            edit::copy_dash_array(object, result)
        })
    }
    .unwrap_or(false) as c_int
}

/// # Safety
/// A non-null object must be live; a nonempty dash_array must contain dash_count floats.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_SetDashArray(
    object: *mut c_void,
    dash_array: *const f32,
    dash_count: usize,
    phase: f32,
) -> c_int {
    if (dash_count > 0 && dash_array.is_null())
        || dash_count > isize::MAX as usize / size_of::<f32>()
    {
        return 0;
    }
    unsafe {
        with_page_object(object, |object| {
            let dashes = if dash_count == 0 {
                Vec::new()
            } else {
                // SAFETY: the caller supplies a readable array; the source API
                // copies it into the object's own graphics state.
                std::slice::from_raw_parts(dash_array, dash_count).to_vec()
            };
            edit::set_dash_array(object, dashes, phase);
        })
    }
    .is_some() as c_int
}

/// # Safety
/// A non-null object must be live and quad_points must point to writable storage.
#[no_mangle]
pub unsafe extern "C" fn FPDFPageObj_GetRotatedBounds(
    object: *mut c_void,
    quad_points: *mut FS_QUADPOINTSF,
) -> c_int {
    if quad_points.is_null() {
        return 0;
    }
    let Some([p1, p2, p3, p4]) =
        (unsafe { with_page_object(object, |object| edit::rotated_bounds(object)) }).flatten()
    else {
        return 0;
    };
    unsafe {
        quad_points.write(FS_QUADPOINTSF {
            x1: p1.x,
            y1: p1.y,
            x2: p2.x,
            y2: p2.y,
            x3: p3.x,
            y3: p3.y,
            x4: p4.x,
            y4: p4.y,
        });
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ptr::null_mut;

    #[test]
    fn line_join() {
        // Complete PDFEditTest.LineJoin from fpdf_edit_unittest.cpp.
        unsafe {
            for invalid_object_value in [-1, 0, 1, 2, 3, 1000] {
                assert_eq!(FPDFPageObj_SetLineJoin(null_mut(), invalid_object_value), 0);
            }
            let path = crate::FPDFPageObj_CreateNewPath(0.0, 0.0);
            assert_eq!(FPDFPageObj_GetLineJoin(path), 0);
            for invalid in [-1, 3, 1000] {
                assert_eq!(FPDFPageObj_SetLineJoin(path, invalid), 0);
            }
            for valid in [2, 1, 0] {
                assert_eq!(FPDFPageObj_SetLineJoin(path, valid), 1);
                assert_eq!(FPDFPageObj_GetLineJoin(path), valid);
            }
            FPDFPageObj_Destroy(path);
        }
    }

    #[test]
    fn line_cap() {
        // Complete PDFEditTest.LineCap from fpdf_edit_unittest.cpp.
        unsafe {
            for invalid_object_value in [-1, 0, 1, 2, 3, 1000] {
                assert_eq!(FPDFPageObj_SetLineCap(null_mut(), invalid_object_value), 0);
            }
            let path = crate::FPDFPageObj_CreateNewPath(0.0, 0.0);
            assert_eq!(FPDFPageObj_GetLineCap(path), 0);
            for invalid in [-1, 3, 1000] {
                assert_eq!(FPDFPageObj_SetLineCap(path, invalid), 0);
            }
            for valid in [2, 1, 0] {
                assert_eq!(FPDFPageObj_SetLineCap(path, valid), 1);
                assert_eq!(FPDFPageObj_GetLineCap(path), valid);
            }
            FPDFPageObj_Destroy(path);
        }
    }

    #[test]
    fn null_objects_and_output_buffers() {
        unsafe {
            assert_eq!(FPDFPageObj_GetLineCap(null_mut()), -1);
            assert_eq!(FPDFPageObj_GetLineJoin(null_mut()), -1);
            assert_eq!(FPDFPageObj_GetDashCount(null_mut()), -1);
            assert_eq!(FPDFPageObj_HasTransparency(null_mut()), 0);
            assert_eq!(FPDFPage_HasTransparency(null_mut()), 0);
            assert_eq!(FPDFPageObj_GetMatrix(null_mut(), null_mut()), 0);
            assert_eq!(FPDFPageObj_GetRotatedBounds(null_mut(), null_mut()), 0);
            assert_eq!(FPDFPageObj_TransformF(null_mut(), std::ptr::null()), 0);
            let path = crate::FPDFPageObj_CreateNewPath(0.0, 0.0);
            let mut untouched = [-1.0; 4];
            assert_eq!(FPDFPageObj_GetDashPhase(path, null_mut()), 0);
            assert_eq!(FPDFPageObj_GetStrokeWidth(path, null_mut()), 0);
            assert_eq!(FPDFPageObj_GetDashArray(path, null_mut(), 0), 0);
            assert_eq!(FPDFPageObj_SetDashArray(path, std::ptr::null(), 1, 0.0), 0);
            let dashes = [1.0, 2.0, 3.0];
            assert_eq!(FPDFPageObj_SetDashArray(path, dashes.as_ptr(), 3, 5.0), 1);
            assert_eq!(FPDFPageObj_GetDashArray(path, untouched.as_mut_ptr(), 2), 0);
            assert_eq!(untouched, [-1.0; 4]);
            assert_eq!(FPDFPageObj_GetDashArray(path, untouched.as_mut_ptr(), 4), 1);
            assert_eq!(untouched, [1.0, 2.0, 3.0, -1.0]);
            assert_eq!(FPDFPageObj_SetDashArray(path, std::ptr::null(), 0, 0.0), 1);
            assert_eq!(FPDFPageObj_GetDashCount(path), 0);
            FPDFPageObj_Destroy(path);
        }
    }
}
