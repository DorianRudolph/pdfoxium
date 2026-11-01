// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fxge::dib::fx_dib::FxdibFormat;
struct Pause;
impl PauseIndicatorIface for Pause {
    fn need_to_pause_now(&mut self) -> bool {
        true
    }
}
#[test]
fn owned_stretch_and_affine_tasks_resume_actual_engine_without_recompositing_rows() {
    let mut source = CfxDibitmap::create(1024, 1024, FxdibFormat::Bgra, 0).unwrap();
    for y in 0..1024 {
        for (x, pixel) in source
            .scanline_mut(y)
            .unwrap()
            .chunks_exact_mut(4)
            .enumerate()
        {
            pixel.copy_from_slice(&[x as u8, y as u8, 200, 128]);
        }
    }
    let source = Rc::new(source);
    for matrix in [
        CfxMatrix::new(128.0, 0.0, 0.0, -96.0, 0.0, 96.0),
        CfxMatrix::new(110.0, 12.0, 9.0, -80.0, 0.0, 80.0),
    ] {
        let mut expected = CfxDibitmap::create(128, 96, FxdibFormat::Bgra, 0).unwrap();
        expected.clear(0xffffffff);
        let mut actual = CfxDibitmap::create(128, 96, FxdibFormat::Bgra, 0).unwrap();
        actual.clear(0xffffffff);
        let mut clip = CfxAggClipRgn::new(128, 96);
        clip.intersect_rect(FxRect::new(2, 3, 120, 90));
        // Same retained clip region is used by borrowed and owned paths.
        let options = FxdibResampleOptions::default();
        {
            let mut renderer = CfxAggImageRenderer::new(
                &mut expected,
                Some(&clip),
                source.clone(),
                0.65,
                0,
                matrix,
                options,
                false,
            );
            while renderer.continue_render(None) {}
        }
        let mut task = CfxAggImageRenderer::new(
            &mut actual,
            Some(&clip),
            source.clone(),
            0.65,
            0,
            matrix,
            options,
            false,
        )
        .into_task();
        let mut yields = 0;
        while task.continue_render(&mut actual, Some(&mut Pause)) {
            yields += 1;
            assert!(yields < 1000);
        }
        assert!(yields > 0);
        assert_eq!(actual.buffer(), expected.buffer(), "matrix={matrix:?}");
    }
}
