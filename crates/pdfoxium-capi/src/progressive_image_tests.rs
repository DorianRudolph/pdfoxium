// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::tests::{pixels, Pause};
use super::*;
#[test]
fn original_image_decoder_and_sampler_remain_on_same_object_across_pauses() {
    unsafe {
        let document = FPDF_CreateNewDocument();
        let page = FPDFPage_New(document, 0, 128.0, 96.0);
        let source = FPDFBitmap_Create(1024, 1024, 0);
        FPDFBitmap_FillRect(source, 0, 0, 1024, 1024, 0xff804020);
        let image = FPDFPageObj_NewImageObj(document);
        assert_eq!(
            FPDFImageObj_SetBitmap(std::ptr::null(), 0, image, source),
            1
        );
        assert_eq!(
            FPDFImageObj_SetMatrix(image, 128.0, 0.0, 0.0, 96.0, 0.0, 0.0),
            1
        );
        assert_eq!(FPDFPage_InsertObject(page, image), 1);
        FPDFBitmap_Destroy(source);
        let expected = FPDFBitmap_Create(128, 96, 1);
        FPDFBitmap_FillRect(expected, 0, 0, 128, 96, 0xffffffff);
        FPDF_RenderPageBitmap(expected, page, 0, 0, 128, 96, 0, 0);
        let bitmap = FPDFBitmap_Create(128, 96, 1);
        FPDFBitmap_FillRect(bitmap, 0, 0, 128, 96, 0xffffffff);
        let mut pause = Pause::new(true);
        assert_eq!(
            FPDF_RenderPageBitmap_Start(bitmap, page, 0, 0, 128, 96, 0, 0, pause.pointer()),
            FPDF_RENDER_TOBECONTINUED
        );
        let slot = (&*page.cast::<PageHandle>()).progressive.clone();
        assert_eq!(
            slot.job
                .borrow()
                .as_ref()
                .unwrap()
                .context
                .renderer()
                .object_index(),
            0
        );
        let mut status = FPDF_RENDER_TOBECONTINUED;
        for _ in 0..1000 {
            if status != FPDF_RENDER_TOBECONTINUED {
                break;
            }
            status = FPDF_RenderPage_Continue(page, pause.pointer());
        }
        assert_eq!(status, FPDF_RENDER_DONE);
        assert!(pause.calls > 2);
        assert_eq!(pixels(bitmap), pixels(expected));
        FPDF_RenderPage_Close(page);
        FPDFBitmap_Destroy(bitmap);
        FPDFBitmap_Destroy(expected);
        FPDF_ClosePage(page);
        FPDF_CloseDocument(document);
    }
}
