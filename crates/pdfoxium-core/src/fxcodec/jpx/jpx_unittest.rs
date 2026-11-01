// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Selected `YUV420ToRGB` case from `core/fxcodec/jpx/jpx_unittest.cpp`.
//! Component planes are the selected safe codec's owned sample vectors;
//! native bpp is folded into precision. The direct helper bypasses subsampling
//! dispatch, preserving the source's dx/dy=1 even for the chroma components.
#![allow(non_snake_case)]
use super::*;
fn TakeComponent(source: &mut PdfiumComponent) -> PdfiumComponent {
    PdfiumComponent {
        dx: source.dx,
        dy: source.dy,
        width: source.width,
        height: source.height,
        precision: source.precision,
        signed: source.signed,
        samples: std::mem::take(&mut source.samples),
    }
}
#[test]
fn YUV420ToRGB() {
    let mut u = PdfiumComponent {
        dx: 1,
        dy: 1,
        width: 16,
        height: 16,
        precision: 8,
        signed: false,
        samples: Vec::new(),
    };
    let mut v = PdfiumComponent {
        dx: 1,
        dy: 1,
        width: 16,
        height: 16,
        precision: 8,
        signed: false,
        samples: Vec::new(),
    };
    let mut y = PdfiumComponent {
        dx: 1,
        dy: 1,
        width: 0,
        height: 0,
        precision: 8,
        signed: false,
        samples: Vec::new(),
    };
    let mut img: Vec<PdfiumComponent> = (0..3)
        .map(|_| PdfiumComponent {
            dx: 0,
            dy: 0,
            width: 0,
            height: 0,
            precision: 0,
            signed: false,
            samples: Vec::new(),
        })
        .collect();
    let cases: [(u32, bool); 8] = [
        (0, false),
        (1, false),
        (30, false),
        (31, true),
        (32, true),
        (33, false),
        (34, false),
        (u32::MAX, false),
    ];
    for testcase in cases {
        y.width = testcase.0;
        y.height = y.width;
        // Native uint32 width*height wraps before the size_t byte multiplier:
        // UINT_MAX*UINT_MAX allocates one sample, not an enormous fake span.
        // memset(...,1,...) fills each native int with 0x01010101.
        y.samples = vec![0x01010101i32 as f32; y.width.wrapping_mul(y.height) as usize];
        v.samples = vec![0.; (v.width * v.height) as usize];
        u.samples = vec![0.; (u.width * u.height) as usize];
        // Native copies component structs (transferring their raw allocation
        // ownership to img); move the Vec buffers and retain only scalar metadata.
        img[0] = TakeComponent(&mut y);
        img[1] = TakeComponent(&mut u);
        img[2] = TakeComponent(&mut v);
        sycc420_to_rgb(&mut img);
        if testcase.1 {
            assert_eq!(img[0].width, img[1].width);
            assert_eq!(img[0].height, img[1].height);
            assert_eq!(img[0].width, img[2].width);
            assert_eq!(img[0].height, img[2].height);
        } else {
            assert_ne!(img[0].width, img[1].width);
            assert_ne!(img[0].height, img[1].height);
            assert_ne!(img[0].width, img[2].width);
            assert_ne!(img[0].height, img[2].height);
        }
        img[0].samples = Vec::new();
        img[1].samples = Vec::new();
        img[2].samples = Vec::new();
    }
}
