// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Public path editing from `fpdfsdk/fpdf_editpath.cpp`.

use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use pdfoxium_core::fpdfsdk::fpdf_editpath as engine;
use pdfoxium_core::fxge::cfx_path::PathPoint;

#[allow(non_camel_case_types)]
#[repr(C)]
pub struct FS_POINTF {
    pub x: f32,
    pub y: f32,
}

// Page segments borrow the public object handle and resolve their point by
// index. Keeping a pointer into the engine's path vector would be invalidated
// by appending another segment. Glyph paths are immutable, so their handles
// can retain a single copied point and be owned by the public glyph-path handle.
pub(super) enum PathSegmentHandle {
    PagePath {
        object: NonNull<PageObjectHandle>,
        index: usize,
    },
    ClipPath {
        clip: NonNull<super::transformpage::ClipPathHandle>,
        path_index: usize,
        segment_index: usize,
    },
    Glyph(PathPoint),
}

unsafe fn with_path<R>(object: *mut c_void, f: impl FnOnce(&mut CpdfPathObject) -> R) -> Option<R> {
    unsafe { with_page_object(object, |o| o.as_any_mut().downcast_mut().map(f)) }.flatten()
}

unsafe fn segment_point(segment: *mut c_void) -> Option<PathPoint> {
    let segment = NonNull::new(segment.cast::<PathSegmentHandle>())?;
    // SAFETY: the ABI requires the segment and its owning page object or glyph
    // path to remain live. Each segment is boxed separately from engine data.
    match unsafe { segment.as_ref() } {
        PathSegmentHandle::PagePath { object, index } => unsafe {
            with_path(object.as_ptr().cast(), |path| {
                engine::segments(path).get(*index).copied()
            })
        }
        .flatten(),
        PathSegmentHandle::ClipPath {
            clip,
            path_index,
            segment_index,
        } => unsafe {
            super::transformpage::clip_segment_point(*clip, *path_index, *segment_index)
        },
        PathSegmentHandle::Glyph(point) => Some(*point),
    }
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDFPageObj_CreateNewPath(x: f32, y: f32) -> *mut c_void {
    new_owned_page_object(Box::new(engine::create_new_path(x, y)))
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDFPageObj_CreateNewRect(x: f32, y: f32, w: f32, h: f32) -> *mut c_void {
    new_owned_page_object(Box::new(engine::create_new_rect(x, y, w, h)))
}

/// # Safety
/// Path is null or a live object handle whose owning page remains live.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_CountSegments(path: *mut c_void) -> c_int {
    unsafe {
        with_path(path, |p| {
            c_int::try_from(engine::segments(p).len()).unwrap_or(-1)
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// Path is null or a live object handle. The returned segment borrows that
/// object and must not be used after the object or its owning page is destroyed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_GetPathSegment(path: *mut c_void, index: c_int) -> *mut c_void {
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let present =
        unsafe { with_path(path, |p| index < engine::segments(p).len()) }.unwrap_or(false);
    if !present {
        return std::ptr::null_mut();
    }
    let object = NonNull::new(path.cast::<PageObjectHandle>()).unwrap();
    // SAFETY: with_path has finished borrowing the object and its owning page.
    // Borrow only the cache while inserting a separately allocated segment.
    let handles = unsafe { &mut (*object.as_ptr()).segment_handles };
    let segment = handles
        .entry(index)
        .or_insert_with(|| Box::new(PathSegmentHandle::PagePath { object, index }));
    (&mut **segment as *mut PathSegmentHandle).cast()
}

/// # Safety
/// Path is null or a live object handle available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_MoveTo(path: *mut c_void, x: f32, y: f32) -> c_int {
    unsafe { with_path(path, |p| engine::move_to(p, x, y)) }.is_some() as c_int
}

/// # Safety
/// Path is null or a live object handle available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_LineTo(path: *mut c_void, x: f32, y: f32) -> c_int {
    unsafe { with_path(path, |p| engine::line_to(p, x, y)) }.is_some() as c_int
}

/// # Safety
/// Path is null or a live object handle available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_BezierTo(
    path: *mut c_void,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    x3: f32,
    y3: f32,
) -> c_int {
    unsafe {
        with_path(path, |p| {
            engine::bezier_to(
                p,
                CfxPointF::new(x1, y1),
                CfxPointF::new(x2, y2),
                CfxPointF::new(x3, y3),
            );
        })
    }
    .is_some() as c_int
}

/// # Safety
/// Path is null or live. Non-null outputs must be writable. Outputs are only
/// written for a valid Bezier endpoint with both output pointers present.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_GetBezierControlPoints(
    path: *mut c_void,
    index: usize,
    first_control_point: *mut FS_POINTF,
    second_control_point: *mut FS_POINTF,
) -> c_int {
    if first_control_point.is_null() || second_control_point.is_null() {
        return 0;
    }
    let result = unsafe { with_path(path, |p| engine::bezier_control_points(p, index)) }.flatten();
    let Some((first, second)) = result else {
        return 0;
    };
    unsafe {
        *first_control_point = FS_POINTF {
            x: first.x,
            y: first.y,
        };
        *second_control_point = FS_POINTF {
            x: second.x,
            y: second.y,
        };
    }
    1
}

/// # Safety
/// Path is null or a live object handle available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_Close(path: *mut c_void) -> c_int {
    unsafe { with_path(path, engine::close) }.unwrap_or(false) as c_int
}

/// # Safety
/// Path is null or a live object handle available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_SetDrawMode(
    path: *mut c_void,
    fillmode: c_int,
    stroke: c_int,
) -> c_int {
    unsafe { with_path(path, |p| engine::set_draw_mode(p, fillmode, stroke != 0)) }.is_some()
        as c_int
}

/// # Safety
/// Path is null or live. Non-null output pointers must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPath_GetDrawMode(
    path: *mut c_void,
    fillmode: *mut c_int,
    stroke: *mut c_int,
) -> c_int {
    if fillmode.is_null() || stroke.is_null() {
        return 0;
    }
    let Some((fill, outline)) = (unsafe { with_path(path, |p| engine::draw_mode(p)) }) else {
        return 0;
    };
    unsafe {
        *fillmode = fill;
        *stroke = outline as c_int;
    }
    1
}

/// # Safety
/// Segment is null or a live borrowed segment and non-null outputs are writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPathSegment_GetPoint(
    segment: *mut c_void,
    x: *mut f32,
    y: *mut f32,
) -> c_int {
    if x.is_null() || y.is_null() {
        return 0;
    }
    let Some(point) = (unsafe { segment_point(segment) }) else {
        return 0;
    };
    unsafe {
        *x = point.point.x;
        *y = point.point.y;
    }
    1
}

/// # Safety
/// Segment is null or a live borrowed segment whose owner remains live.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPathSegment_GetType(segment: *mut c_void) -> c_int {
    unsafe { segment_point(segment) }.map_or(-1, |point| point.point_type as c_int)
}

/// # Safety
/// Segment is null or a live borrowed segment whose owner remains live.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPathSegment_GetClose(segment: *mut c_void) -> c_int {
    unsafe { segment_point(segment) }.is_some_and(|point| point.close_figure) as c_int
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segment_handle_survives_appends_and_reports_closure() {
        unsafe {
            let path = FPDFPageObj_CreateNewPath(4.0, 5.0);
            assert_eq!(FPDFPath_LineTo(path, 8.0, 9.0), 1);
            let endpoint = FPDFPath_GetPathSegment(path, 1);
            assert_eq!(FPDFPathSegment_GetClose(endpoint), 0);
            assert_eq!(FPDFPath_Close(path), 1);
            assert_eq!(FPDFPathSegment_GetClose(endpoint), 1);
            for index in 0..100 {
                assert_eq!(FPDFPath_LineTo(path, index as f32, 0.0), 1);
            }
            assert_eq!(FPDFPath_GetPathSegment(path, 1), endpoint);
            let (mut x, mut y) = (0.0, 0.0);
            assert_eq!(FPDFPathSegment_GetPoint(endpoint, &mut x, &mut y), 1);
            assert_eq!((x, y), (8.0, 9.0));
            assert_eq!(FPDFPathSegment_GetType(endpoint), 0);
            FPDFPageObj_Destroy(path);
        }
    }

    #[test]
    fn invalid_paths_and_outputs_fail_without_writing() {
        unsafe {
            let null = std::ptr::null_mut();
            assert_eq!(FPDFPath_CountSegments(null), -1);
            assert!(FPDFPath_GetPathSegment(null, 0).is_null());
            assert_eq!(FPDFPathSegment_GetType(null), -1);
            let mut value = 42.0;
            assert_eq!(FPDFPathSegment_GetPoint(null, &mut value, &mut value), 0);
            assert_eq!(value, 42.0);
            let path = FPDFPageObj_CreateNewPath(1.0, 2.0);
            assert!(FPDFPath_GetPathSegment(path, -1).is_null());
            assert!(FPDFPath_GetPathSegment(path, 1).is_null());
            assert_eq!(FPDFPath_GetDrawMode(path, null.cast(), null.cast()), 0);
            FPDFPageObj_Destroy(path);
        }
    }
}
