// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
#[repr(C)]
pub(super) struct Pause {
    interface: IFSDK_PAUSE,
    pause: bool,
    pub(super) calls: usize,
    action: u8,
    page: *mut c_void,
    bitmap: *mut c_void,
    nested: c_int,
}
impl Pause {
    pub(super) fn new(pause: bool) -> Self {
        Self {
            interface: IFSDK_PAUSE {
                version: 1,
                NeedToPauseNow: Some(pause_now),
                user: std::ptr::null_mut(),
            },
            pause,
            calls: 0,
            action: 0,
            page: std::ptr::null_mut(),
            bitmap: std::ptr::null_mut(),
            nested: -1,
        }
    }
    pub(super) fn pointer(&mut self) -> *mut IFSDK_PAUSE {
        std::ptr::addr_of_mut!(self.interface)
    }
}
unsafe extern "C" fn pause_now(interface: *mut IFSDK_PAUSE) -> c_int {
    let state = interface.cast::<Pause>();
    let (page, bitmap, action, paused) = unsafe {
        (
            (*state).page,
            (*state).bitmap,
            (*state).action,
            (*state).pause,
        )
    };
    unsafe {
        (*state).calls += 1;
        (*state).action = 0;
    }
    if action == 1 {
        assert_eq!(unsafe { FPDF_GetPageWidthF(page) }, 250.0);
        let nested = unsafe { FPDF_RenderPage_Continue(page, interface) };
        unsafe {
            (*state).nested = nested;
        }
        assert_eq!(
            unsafe { FPDFBitmap_FillRect(bitmap, 249, 0, 1, 1, 0xff123456) },
            1
        );
        unsafe {
            FPDF_RenderPage_Close(page);
        }
    } else if action == 2 {
        unsafe {
            FPDF_ClosePage(page);
            FPDFBitmap_Destroy(bitmap);
            (*state).page = std::ptr::null_mut();
            (*state).bitmap = std::ptr::null_mut();
        }
    }
    c_int::from(paused)
}
unsafe fn create_many_paths() -> (*mut c_void, *mut c_void, *mut c_void) {
    let document = FPDF_CreateNewDocument();
    let page = unsafe { FPDFPage_New(document, 0, 250.0, 1.0) };
    assert!(!page.is_null());
    for index in 0..220 {
        let object = FPDFPageObj_CreateNewRect(index as f32, 0.0, 1.0, 1.0);
        assert_eq!(
            unsafe { FPDFPageObj_SetFillColor(object, 255, 0, 0, 128) },
            1
        );
        assert_eq!(unsafe { FPDFPath_SetDrawMode(object, 2, 0) }, 1);
        assert_eq!(unsafe { FPDFPage_InsertObject(page, object) }, 1);
    }
    let bitmap = FPDFBitmap_Create(250, 1, 1);
    assert_eq!(
        unsafe { FPDFBitmap_FillRect(bitmap, 0, 0, 250, 1, 0xffffffff) },
        1
    );
    (document, page, bitmap)
}
pub(super) unsafe fn pixels(bitmap: *mut c_void) -> Vec<u8> {
    let length = unsafe { FPDFBitmap_GetStride(bitmap) } as usize
        * unsafe { FPDFBitmap_GetHeight(bitmap) } as usize;
    unsafe { std::slice::from_raw_parts(FPDFBitmap_GetBuffer(bitmap).cast::<u8>(), length) }
        .to_vec()
}
#[test]
fn native_pause_cursor_paints_each_translucent_object_once_and_preserves_target() {
    unsafe {
        let (document, page, bitmap) = create_many_paths();
        let expected = FPDFBitmap_Create(250, 1, 1);
        FPDFBitmap_FillRect(expected, 0, 0, 250, 1, 0xffffffff);
        FPDF_RenderPageBitmap(expected, page, 0, 0, 250, 1, 0, 0);
        let mut pause = Pause::new(true);
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 250, 1, 0, 0, pause.pointer()),
            FPDF_RENDER_TOBECONTINUED
        );
        let first = pixels(bitmap);
        assert_ne!(&first[..4], &[255; 4]);
        assert_eq!(&first[100 * 4..101 * 4], &[255; 4]);
        assert_eq!(
            FPDF_RenderPage_Continue(page, pause.pointer()),
            FPDF_RENDER_TOBECONTINUED
        );
        let second = pixels(bitmap);
        assert_eq!(&second[..400], &first[..400]);
        assert_ne!(&second[100 * 4..101 * 4], &[255; 4]);
        assert_eq!(&second[200 * 4..201 * 4], &[255; 4]);
        let mut status = FPDF_RENDER_TOBECONTINUED;
        for _ in 0..4 {
            if status != FPDF_RENDER_TOBECONTINUED {
                break;
            }
            status = FPDF_RenderPage_Continue(page, pause.pointer());
        }
        assert_eq!(status, FPDF_RENDER_DONE);
        assert_eq!(pixels(bitmap), pixels(expected));
        assert!(pause.calls >= 3);
        assert_eq!(
            FPDF_RenderPage_Continue(page, pause.pointer()),
            FPDF_RENDER_DONE
        );
        FPDF_RenderPage_Close(page);
        assert_eq!(
            FPDF_RenderPage_Continue(page, pause.pointer()),
            FPDF_RENDER_FAILED
        );
        FPDFBitmap_Destroy(expected);
        FPDFBitmap_Destroy(bitmap);
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
}
#[test]
fn native_annotation_fixture_paused_and_unpaused_outputs_match_static_agg() {
    // Native RenderWithoutPause/RenderWithPause/RenderAnnotWithPause use this
    // same fixture. The actual renderer's final pixels are compared in full.
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/annotation_stamp_with_ap.pdf"
    ));
    unsafe {
        let document = FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
        let page = FPDF_LoadPage(document, 0);
        let width = FPDF_GetPageWidthF(page) as c_int;
        let height = FPDF_GetPageHeightF(page) as c_int;
        assert!(width > 0 && height > 0);
        let mut plain = None;
        for flags in [0, 1] {
            let expected = FPDFBitmap_Create(width, height, 1);
            FPDFBitmap_FillRect(expected, 0, 0, width, height, 0xffffffff);
            FPDF_RenderPageBitmap(expected, page, 0, 0, width, height, 0, flags);
            for paused in [false, true] {
                let bitmap = FPDFBitmap_Create(width, height, 1);
                FPDFBitmap_FillRect(bitmap, 0, 0, width, height, 0xffffffff);
                let mut pause = Pause::new(paused);
                let mut result = FPDF_RenderPageBitmap_Start(
                    bitmap,
                    page,
                    0,
                    0,
                    width,
                    height,
                    0,
                    flags,
                    pause.pointer(),
                );
                if paused {
                    assert_eq!(result, FPDF_RENDER_TOBECONTINUED);
                } else {
                    assert_eq!(result, FPDF_RENDER_DONE);
                }
                for _ in 0..100 {
                    if result != FPDF_RENDER_TOBECONTINUED {
                        break;
                    }
                    result = FPDF_RenderPage_Continue(page, pause.pointer());
                }
                assert_eq!(result, FPDF_RENDER_DONE);
                assert_eq!(pixels(bitmap), pixels(expected));
                FPDF_RenderPage_Close(page);
                FPDFBitmap_Destroy(bitmap);
            }
            let output = pixels(expected);
            if flags == 0 {
                plain = Some(output);
            } else {
                assert_ne!(Some(output), plain);
            }
            FPDFBitmap_Destroy(expected);
        }
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
}
#[test]
fn callback_reentry_cancel_destroy_and_invalid_interfaces_keep_original_owners_safe() {
    unsafe {
        let (document, page, bitmap) = create_many_paths();
        let mut pause = Pause::new(true);
        pause.page = page;
        pause.bitmap = bitmap;
        pause.action = 1;
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 250, 1, 0, 0, pause.pointer()),
            FPDF_RENDER_FAILED
        );
        assert_eq!(pause.nested, FPDF_RENDER_FAILED);
        let pixels = pixels(bitmap);
        assert_eq!(&pixels[249 * 4..250 * 4], &[0x56, 0x34, 0x12, 0xff]);
        pause.interface.version = 2;
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 250, 1, 0, 0, pause.pointer()),
            FPDF_RENDER_FAILED
        );
        pause.interface.version = 1;
        pause.action = 2;
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 250, 1, 0, 0, std::ptr::null_mut()),
            FPDF_RENDER_FAILED
        );
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 250, 1, 0, 0, pause.pointer()),
            FPDF_RENDER_FAILED
        );
        assert!(pause.page.is_null() && pause.bitmap.is_null());
        FPDF_CloseDocument(document);
    }
}
#[test]
fn native_color_scheme_and_fill_to_stroke_produce_real_selected_colors() {
    let bytes = include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources/rectangles.pdf"
    ));
    unsafe {
        let document = FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), std::ptr::null());
        let page = FPDF_LoadPage(document, 0);
        let width = FPDF_GetPageWidthF(page) as c_int;
        let height = FPDF_GetPageHeightF(page) as c_int;
        let scheme = FPDF_COLORSCHEME {
            path_fill_color: 0xffffffff,
            path_stroke_color: 0xffff0000,
            text_fill_color: 0xff0000ff,
            text_stroke_color: 0xff0000ff,
        };
        let mut previous = None;
        // Integer rectangle edges with a one-pixel smoothed stroke have no
        // fully covered red pixel. Disable path smoothing for exact scheme
        // colors, while independently exercising fill-to-stroke conversion.
        for flags in [0x4000, 0x4020] {
            let bitmap = FPDFBitmap_Create(width, height, 1);
            FPDFBitmap_FillRect(bitmap, 0, 0, width, height, 0xff000000);
            let mut pause = Pause::new(false);
            assert_eq!(
                FPDF_RenderPageBitmapWithColorScheme_Start(
                    bitmap,
                    page,
                    0,
                    0,
                    width,
                    height,
                    0,
                    flags,
                    &scheme,
                    pause.pointer()
                ),
                FPDF_RENDER_DONE
            );
            let output = pixels(bitmap);
            assert!(output
                .chunks_exact(4)
                .any(|pixel| pixel == [0, 0, 255, 255]));
            if flags & 0x20 == 0 {
                assert!(output.chunks_exact(4).any(|pixel| pixel == [255; 4]));
                previous = Some(output);
            } else {
                assert_ne!(Some(output), previous);
            }
            FPDF_RenderPage_Close(page);
            FPDFBitmap_Destroy(bitmap);
        }
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
}
