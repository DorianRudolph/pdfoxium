// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Public page boxes, transforms and clips from `fpdfsdk/fpdf_transformpage.cpp`.

use super::*;
use pdfoxium_core::fpdfapi::page::cpdf_clippath::CpdfClipPath;
use pdfoxium_core::fpdfsdk::fpdf_transformpage as engine;
use pdfoxium_core::fxcrt::fx_coordinates::CfxMatrix;
use pdfoxium_core::fxge::cfx_path::PathPoint;
use std::collections::HashMap;

pub(super) struct ClipPathHandle {
    location: ClipPathLocation,
    segments: HashMap<(usize, usize), Box<super::editpath::PathSegmentHandle>>,
}

enum ClipPathLocation {
    Owned(CpdfClipPath),
    Borrowed(NonNull<PageObjectHandle>),
}

unsafe fn with_clip<R>(handle: *mut c_void, f: impl FnOnce(&CpdfClipPath) -> R) -> Option<R> {
    let handle = NonNull::new(handle.cast::<ClipPathHandle>())?;
    // SAFETY: the clip and (for borrowed clips) its owning page object must be
    // live. Resolving by handle preserves clip identity across copy-on-write.
    match unsafe { &(*handle.as_ptr()).location } {
        ClipPathLocation::Owned(clip) => Some(f(clip)),
        ClipPathLocation::Borrowed(object) => unsafe {
            with_page_object(object.as_ptr().cast(), |o| f(o.base().clip_path()))
        },
    }
}

pub(super) unsafe fn clip_segment_point(
    clip: NonNull<ClipPathHandle>,
    path_index: usize,
    segment_index: usize,
) -> Option<PathPoint> {
    unsafe {
        with_clip(clip.as_ptr().cast(), |clip| {
            if !clip.has_ref() || path_index >= clip.path_count() {
                return None;
            }
            let path = clip.path(path_index);
            path.get_object()?.points().get(segment_index).copied()
        })
    }
    .flatten()
}

unsafe fn set_box(page: *mut c_void, key: &[u8], rect: CfxFloatRect) {
    if page.is_null() {
        return;
    }
    let Some(mut page) = unsafe { pdf_handle_mut(page) }.and_then(PageHandle::page_mut) else {
        return;
    };
    engine::set_bounding_box(&mut page, key, rect);
}

unsafe fn get_box(
    page: *mut c_void,
    key: &[u8],
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    if page.is_null() || left.is_null() || bottom.is_null() || right.is_null() || top.is_null() {
        return 0;
    }
    let Some(page) = unsafe { pdf_handle(page) }.and_then(PageHandle::pdf_page) else {
        return 0;
    };
    let Some(rect) = engine::bounding_box(page, key) else {
        return 0;
    };
    unsafe {
        *left = rect[0];
        *bottom = rect[1];
        *right = rect[2];
        *top = rect[3];
    }
    1
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetMediaBox(
    page: *mut c_void,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
) {
    unsafe {
        set_box(
            page,
            b"MediaBox",
            CfxFloatRect::new(left, bottom, right, top),
        )
    };
}

/// # Safety
/// Page is null or live; non-null outputs must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetMediaBox(
    page: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    unsafe { get_box(page, b"MediaBox", left, bottom, right, top) }
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetCropBox(
    page: *mut c_void,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
) {
    unsafe {
        set_box(
            page,
            b"CropBox",
            CfxFloatRect::new(left, bottom, right, top),
        )
    };
}

/// # Safety
/// Page is null or live; non-null outputs must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetCropBox(
    page: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    unsafe { get_box(page, b"CropBox", left, bottom, right, top) }
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetBleedBox(
    page: *mut c_void,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
) {
    unsafe {
        set_box(
            page,
            b"BleedBox",
            CfxFloatRect::new(left, bottom, right, top),
        )
    };
}

/// # Safety
/// Page is null or live; non-null outputs must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetBleedBox(
    page: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    unsafe { get_box(page, b"BleedBox", left, bottom, right, top) }
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetTrimBox(
    page: *mut c_void,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
) {
    unsafe {
        set_box(
            page,
            b"TrimBox",
            CfxFloatRect::new(left, bottom, right, top),
        )
    };
}

/// # Safety
/// Page is null or live; non-null outputs must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetTrimBox(
    page: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    unsafe { get_box(page, b"TrimBox", left, bottom, right, top) }
}

/// # Safety
/// Page is null or a live page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_SetArtBox(
    page: *mut c_void,
    left: f32,
    bottom: f32,
    right: f32,
    top: f32,
) {
    unsafe { set_box(page, b"ArtBox", CfxFloatRect::new(left, bottom, right, top)) };
}

/// # Safety
/// Page is null or live; non-null outputs must be writable.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_GetArtBox(
    page: *mut c_void,
    left: *mut f32,
    bottom: *mut f32,
    right: *mut f32,
    top: *mut f32,
) -> c_int {
    unsafe { get_box(page, b"ArtBox", left, bottom, right, top) }
}

/// # Safety
/// Page is null or live. Matrix and clip rectangle are null or readable; the
/// page and document must remain exclusively accessible during this mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_TransFormWithClip(
    page: *mut c_void,
    matrix: *const FS_MATRIX,
    clip_rect: *const FS_RECTF,
) -> c_int {
    if page.is_null() || (matrix.is_null() && clip_rect.is_null()) {
        return 0;
    }
    let matrix = if matrix.is_null() {
        None
    } else {
        let matrix = unsafe { &*matrix };
        Some(CfxMatrix::new(
            matrix.a, matrix.b, matrix.c, matrix.d, matrix.e, matrix.f,
        ))
    };
    let clip = if clip_rect.is_null() {
        None
    } else {
        let clip = unsafe { &*clip_rect };
        Some(CfxFloatRect::new(
            clip.left,
            clip.bottom,
            clip.right,
            clip.top,
        ))
    };
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return 0;
    };
    engine::transform_with_clip(
        &page._document.borrow(),
        page.pdf_page().unwrap(),
        matrix,
        clip,
    ) as c_int
}

/// # Safety
/// Object is null or live, with its owning page available for exclusive mutation.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_TransformClipPath(
    object: *mut c_void,
    a: f64,
    b: f64,
    c: f64,
    d: f64,
    e: f64,
    f: f64,
) {
    let matrix = CfxMatrix::new(a as f32, b as f32, c as f32, d as f32, e as f32, f as f32);
    unsafe { with_page_object(object, |o| engine::transform_clip_path(o, matrix)) };
}

/// # Safety
/// Object is null or live. The returned clip is borrowed and must not be
/// destroyed; its owning page object and page must remain live while used.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPageObj_GetClipPath(object: *mut c_void) -> *mut c_void {
    if unsafe { with_page_object(object, |_| ()) }.is_none() {
        return std::ptr::null_mut();
    }
    let object = NonNull::new(object.cast::<PageObjectHandle>()).unwrap();
    // SAFETY: the preceding object lookup has ended. Mutate only the cache,
    // retaining an address-stable clip handle separately from engine storage.
    let cache = unsafe { &mut (*object.as_ptr()).clip_handle };
    let clip = cache.get_or_insert_with(|| {
        Box::new(ClipPathHandle {
            location: ClipPathLocation::Borrowed(object),
            segments: HashMap::new(),
        })
    });
    (&mut **clip as *mut ClipPathHandle).cast()
}

/// # Safety
/// Clip is null or a live owned/borrowed clip handle whose owner remains live.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFClipPath_CountPaths(clip: *mut c_void) -> c_int {
    unsafe {
        with_clip(clip, |clip| {
            if clip.has_ref() {
                c_int::try_from(clip.path_count()).unwrap_or(-1)
            } else {
                -1
            }
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// Clip is null or a live owned/borrowed clip handle whose owner remains live.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFClipPath_CountPathSegments(
    clip: *mut c_void,
    path_index: c_int,
) -> c_int {
    let Ok(path_index) = usize::try_from(path_index) else {
        return -1;
    };
    unsafe {
        with_clip(clip, |clip| {
            if !clip.has_ref() || path_index >= clip.path_count() {
                return -1;
            }
            let path = clip.path(path_index);
            let count = path.get_object().map_or(0, |path| path.points().len());
            c_int::try_from(count).unwrap_or(-1)
        })
    }
    .unwrap_or(-1)
}

/// # Safety
/// Clip is null or live. A returned segment borrows the clip and must not be
/// used after its owning clip/page object/page is destroyed.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFClipPath_GetPathSegment(
    clip: *mut c_void,
    path_index: c_int,
    segment_index: c_int,
) -> *mut c_void {
    let (Ok(path_index), Ok(segment_index)) =
        (usize::try_from(path_index), usize::try_from(segment_index))
    else {
        return std::ptr::null_mut();
    };
    let Some(clip) = NonNull::new(clip.cast::<ClipPathHandle>()) else {
        return std::ptr::null_mut();
    };
    if unsafe { clip_segment_point(clip, path_index, segment_index) }.is_none() {
        return std::ptr::null_mut();
    }
    let segments = unsafe { &mut (*clip.as_ptr()).segments };
    let segment = segments
        .entry((path_index, segment_index))
        .or_insert_with(|| {
            Box::new(super::editpath::PathSegmentHandle::ClipPath {
                clip,
                path_index,
                segment_index,
            })
        });
    (&mut **segment as *mut super::editpath::PathSegmentHandle).cast()
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn FPDF_CreateClipPath(left: f32, bottom: f32, right: f32, top: f32) -> *mut c_void {
    Box::into_raw(Box::new(ClipPathHandle {
        location: ClipPathLocation::Owned(engine::create_clip_path(CfxFloatRect::new(
            left, bottom, right, top,
        ))),
        segments: HashMap::new(),
    }))
    .cast()
}

/// # Safety
/// Clip is null or a live caller-owned clip from FPDF_CreateClipPath. Borrowed
/// clips returned by FPDFPageObj_GetClipPath must never be passed here.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDF_DestroyClipPath(clip: *mut c_void) {
    if !clip.is_null() {
        drop(unsafe { Box::from_raw(clip.cast::<ClipPathHandle>()) });
    }
}

/// # Safety
/// Page and clip are null or live. The page and document are exclusively
/// accessible during this synchronous content mutation. Clip ownership remains
/// with the caller (or with the page object for a borrowed clip).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn FPDFPage_InsertClipPath(page: *mut c_void, clip: *mut c_void) {
    if page.is_null() {
        return;
    }
    // Retain the source's shared copy-on-write clip before borrowing the target
    // page; the clip may belong to an object on that same page. This does not
    // clone any path buffers or text objects.
    let Some(clip) = (unsafe { with_clip(clip, Clone::clone) }) else {
        return;
    };
    let Some(page) = (unsafe { pdf_handle(page) }) else {
        return;
    };
    engine::insert_clip_path(&page._document.borrow(), page.pdf_page().unwrap(), &clip);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn borrowed_clip_segments_follow_object_transforms() {
        unsafe {
            let object = super::super::editpath::FPDFPageObj_CreateNewRect(0.0, 0.0, 30.0, 40.0);
            with_page_object(object, |o| {
                *o.base_mut().mutable_clip_path() =
                    engine::create_clip_path(CfxFloatRect::new(1.0, 2.0, 10.0, 20.0));
            });
            let clip = FPDFPageObj_GetClipPath(object);
            assert_eq!(FPDFClipPath_CountPaths(clip), 1);
            assert_eq!(FPDFClipPath_CountPathSegments(clip, 0), 5);
            assert_eq!(FPDFClipPath_CountPathSegments(clip, -1), -1);
            let segment = FPDFClipPath_GetPathSegment(clip, 0, 0);
            assert!(!segment.is_null());
            assert_eq!(FPDFClipPath_GetPathSegment(clip, 0, 0), segment);
            FPDFPageObj_TransformClipPath(object, 2.0, 0.0, 0.0, 3.0, 4.0, 5.0);
            let (mut x, mut y) = (0.0, 0.0);
            assert_eq!(
                super::super::editpath::FPDFPathSegment_GetPoint(segment, &mut x, &mut y),
                1
            );
            assert_eq!((x, y), (6.0, 11.0));
            FPDFPageObj_Destroy(object);
        }
    }

    #[test]
    fn owned_clip_queries_validate_ranges_and_empty_object_clip_is_distinct() {
        unsafe {
            let clip = FPDF_CreateClipPath(0.0, 1.0, 2.0, 3.0);
            assert_eq!(FPDFClipPath_CountPaths(clip), 1);
            assert_eq!(FPDFClipPath_CountPathSegments(clip, 1), -1);
            assert!(FPDFClipPath_GetPathSegment(clip, 0, 5).is_null());
            assert!(FPDFClipPath_GetPathSegment(clip, -1, 0).is_null());
            FPDF_DestroyClipPath(clip);
            let object = super::super::editpath::FPDFPageObj_CreateNewPath(0.0, 0.0);
            let clip = FPDFPageObj_GetClipPath(object);
            assert!(!clip.is_null());
            assert_eq!(FPDFClipPath_CountPaths(clip), -1);
            FPDFPageObj_Destroy(object);
        }
    }
}
