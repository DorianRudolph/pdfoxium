// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fpdfapi/page/cpdf_pageobjectholder_unittest.cpp`.
#![allow(non_snake_case, non_upper_case_globals)]

use super::{safe_float_cmp, GraphicsData};
use crate::fxge::dib::fx_dib::BlendMode;
use std::collections::BTreeMap;

#[test]
fn GraphicsDataAsKey() {
    let fMin = f32::MIN_POSITIVE;
    let fMax = f32::MAX;
    let fInf = f32::INFINITY;
    let fNan = f32::NAN;
    for c1 in [fMin, 1.0, 2.0, fMax, fInf, fNan] {
        for c2 in [fMin, 1.0, 2.0, fMax, fInf, fNan] {
            for c3 in [BlendMode::Multiply, BlendMode::Screen] {
                let lhs = GraphicsData {
                    fill_alpha: c1,
                    stroke_alpha: c2,
                    blend_type: c3,
                };
                let rhs = GraphicsData {
                    fill_alpha: c1,
                    stroke_alpha: c2,
                    blend_type: c3,
                };
                assert!(!(lhs < rhs));
            }
        }
    }
    let mut data = [fMax, fInf, fNan, fMin];
    data.sort_by(|a, b| safe_float_cmp(*a, *b));
    assert_eq!(data[0], fMin);
    assert_eq!(data[1], fMax);
    assert_eq!(data[2], fInf);
    assert_eq!(data[3].is_nan(), fNan.is_nan());

    let mut graphics_map = BTreeMap::new();
    let mut x = 0;
    for c3 in [BlendMode::Screen, BlendMode::Multiply] {
        for c2 in [fNan, fInf, fMax, 2.0, 1.0, fMin] {
            for c1 in [fNan, fInf, fMax, 2.0, 1.0, fMin] {
                graphics_map.insert(
                    GraphicsData {
                        fill_alpha: c1,
                        stroke_alpha: c2,
                        blend_type: c3,
                    },
                    x,
                );
                x += 1;
            }
        }
    }
    assert_eq!(x, 72);
    assert_eq!(graphics_map.len(), 72);
    let expected = [
        71, 35, 65, 29, 59, 23, 53, 17, 47, 11, 41, 5, 70, 34, 64, 28, 58, 22, 52, 16, 46, 10, 40,
        4, 69, 33, 63, 27, 57, 21, 51, 15, 45, 9, 39, 3, 68, 32, 62, 26, 56, 20, 50, 14, 44, 8, 38,
        2, 67, 31, 61, 25, 55, 19, 49, 13, 43, 7, 37, 1, 66, 30, 60, 24, 54, 18, 48, 12, 42, 6, 36,
        0,
    ];
    x = 0;
    for item in &graphics_map {
        assert_eq!(expected[x], *item.1, "for position {x}");
        x += 1;
    }
    assert_eq!(x, 72);
    for c3 in [BlendMode::Multiply, BlendMode::Screen] {
        for c2 in [fMin, 1.0, 2.0, fMax, fInf, fNan] {
            for c1 in [fMin, 1.0, 2.0, fMax, fInf, fNan] {
                graphics_map.remove(&GraphicsData {
                    fill_alpha: c1,
                    stroke_alpha: c2,
                    blend_type: c3,
                });
            }
        }
    }
    assert_eq!(graphics_map.len(), 0);
}
