// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use crate::fxcrt::fx_coordinates::CfxPointF;
use crate::xfa::fgas::graphics::cfgas_gepattern::{CfgasGePattern, HatchStyle};

fn pixel(bitmap: &CfxDibitmap, x: usize, y: usize) -> u32 {
    let row = bitmap.scanline(y as i32).unwrap();
    u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().unwrap())
}
fn bitmap() -> CfxDibitmap {
    let mut bitmap = CfxDibitmap::create(40, 40, FxdibFormat::Bgra, 0).unwrap();
    bitmap.clear(0xffffffff);
    bitmap
}
fn rectangle(left: f32, top: f32, width: f32, height: f32) -> CfgasGePath {
    let mut path = CfgasGePath::new();
    path.add_rectangle(left, top, width, height);
    path
}

#[test]
fn solid_path_clip_matrix_and_state_restoration() {
    let mut bitmap = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        graphics.set_fill_color(&CfgasGeColor::Solid(0xffff0000));
        graphics.concat_matrix(CfxMatrix::new(1., 0., 0., 1., 4., 5.));
        assert!(graphics.fill_path(
            &rectangle(0., 0., 10., 10.),
            FillType::Winding,
            CfxMatrix::default()
        ));
        let saved = graphics.get_clip_rect();
        {
            let mut state = graphics.state_restorer();
            state.set_fill_color(&CfgasGeColor::Solid(0xff0000ff));
            assert!(state.set_clip_rect(CfxRectF::new(20., 20., 4., 4.)));
            state.concat_matrix(CfxMatrix::new(1., 0., 0., 1., 16., 15.));
            assert!(state.fill_path(
                &rectangle(0., 0., 10., 10.),
                FillType::EvenOdd,
                CfxMatrix::default()
            ));
        }
        assert_eq!(graphics.get_clip_rect(), saved);
        assert_eq!(
            graphics.get_matrix(),
            CfxMatrix::new(1., 0., 0., 1., 4., 5.)
        );
        assert!(!graphics.restore_graph_state());
        assert!(graphics.fill_path(
            &rectangle(24., 20., 4., 4.),
            FillType::Winding,
            CfxMatrix::default()
        ));
        graphics.set_line_width(2.);
        graphics.set_stroke_color(&CfgasGeColor::Solid(0xff00ff00));
        let mut line = CfgasGePath::new();
        line.add_line(CfxPointF::new(0., 30.), CfxPointF::new(10., 30.));
        assert!(graphics.stroke_path(&line, CfxMatrix::default()));
    }
    assert_eq!(pixel(&bitmap, 5, 6), 0xffff0000);
    assert_eq!(pixel(&bitmap, 3, 5), 0xffffffff);
    assert_eq!(pixel(&bitmap, 21, 21), 0xff0000ff);
    assert_eq!(pixel(&bitmap, 24, 21), 0xffffffff);
    assert_eq!(pixel(&bitmap, 29, 26), 0xffff0000);
    assert_eq!(pixel(&bitmap, 8, 35), 0xff00ff00);
}

#[test]
fn solid_fractional_rectangles_use_render_device_hard_rounding() {
    // CFX_RenderDevice::DrawPath chooses a ten-by-five integer rectangle,
    // dropping the outer edge with less coverage (ties drop right/bottom).
    // A half-covered blue fringe is not the source widget-fill contract.
    for (offset, left, top) in [(0.25, 2, 3), (0.75, 3, 4)] {
        for (color, expected) in [(0xff99_c1da, 0xff99_c1da), (0x8099_c1da, 0xffcb_dfec)] {
            let mut bitmap = bitmap();
            {
                let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
                let mut graphics = CfgasGeGraphics::new(&mut device);
                graphics.concat_matrix(CfxMatrix::new(1., 0., 0., 1., 2., 3.));
                graphics.set_fill_color(&CfgasGeColor::Solid(color));
                assert!(graphics.fill_path(
                    &rectangle(0., 0., 10., 5.),
                    FillType::Winding,
                    CfxMatrix::new(1., 0., 0., 1., offset, offset),
                ));
                assert_eq!(
                    graphics.get_matrix(),
                    CfxMatrix::new(1., 0., 0., 1., 2., 3.)
                );
            }
            for y in 0..40 {
                for x in 0..40 {
                    let painted = (left..left + 10).contains(&x) && (top..top + 5).contains(&y);
                    assert_eq!(
                        pixel(&bitmap, x, y),
                        if painted { expected } else { 0xffff_ffff }
                    );
                }
            }
        }
    }
}

#[test]
fn two_point_solid_fill_uses_cosmetic_line_and_stroke_preserves_width_and_matrix() {
    let mut bitmap = bitmap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        graphics.concat_matrix(CfxMatrix::new(1., 0., 0., 1., 4., 6.));
        graphics.set_fill_color(&CfgasGeColor::Solid(0xff00_00ff));
        let mut line = CfgasGePath::new();
        line.add_line(CfxPointF::new(2., 4.), CfxPointF::new(12., 4.));
        assert!(graphics.fill_path(&line, FillType::Winding, CfxMatrix::default()));
        graphics.set_stroke_color(&CfgasGeColor::Solid(0xffff_0000));
        graphics.set_line_width(2.);
        let mut stroke = CfgasGePath::new();
        stroke.add_line(CfxPointF::new(2., 14.), CfxPointF::new(12., 14.));
        assert!(graphics.stroke_path(&stroke, CfxMatrix::default()));
    }
    // The filled two-point path must be visible through DrawCosmeticLine,
    // with blue-only coverage; its default one-pixel stroke may straddle rows.
    let blue = [pixel(&bitmap, 8, 9), pixel(&bitmap, 8, 10)];
    assert!(blue.iter().any(|&color| color != 0xffff_ffff));
    for color in blue {
        assert_eq!(color & 0xff00_00ff, 0xff00_00ff);
        assert_eq!((color >> 16) & 255, (color >> 8) & 255);
    }
    assert_eq!(pixel(&bitmap, 8, 19), 0xffff_0000);
    assert_eq!(pixel(&bitmap, 8, 20), 0xffff_0000);
    assert_eq!(pixel(&bitmap, 8, 18), 0xffff_ffff);
    assert_eq!(pixel(&bitmap, 8, 21), 0xffff_ffff);
    assert_eq!(pixel(&bitmap, 3, 10), 0xffff_ffff);
    assert_eq!(pixel(&bitmap, 17, 20), 0xffff_ffff);
}

#[test]
fn all_hatches_fill_background_preserve_pattern_phase_and_clip() {
    for style in [
        HatchStyle::Horizontal,
        HatchStyle::Vertical,
        HatchStyle::ForwardDiagonal,
        HatchStyle::BackwardDiagonal,
        HatchStyle::Cross,
        HatchStyle::DiagonalCross,
    ] {
        let mut bitmap = bitmap();
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            graphics.set_fill_color(&CfgasGeColor::Pattern {
                pattern: Rc::new(CfgasGePattern::new(style, 0xff000000, 0xff00ff00)),
                argb: 0,
            });
            assert!(graphics.fill_path(
                &rectangle(3., 4., 32., 32.),
                FillType::Winding,
                CfxMatrix::default()
            ));
            assert_eq!(graphics.get_clip_rect(), CfxRectF::new(0., 0., 40., 40.));
        }
        for y in 0..40 {
            for x in 0..40 {
                let expected = if (3..35).contains(&x) && (4..36).contains(&y) {
                    let (cx, cy) = ((x - 3) % 16, (y - 4) % 16);
                    let marked = match style {
                        HatchStyle::Horizontal => cy % 8 == 0,
                        HatchStyle::Vertical => cx % 8 == 0,
                        HatchStyle::ForwardDiagonal => cx % 8 == cy % 8,
                        HatchStyle::BackwardDiagonal => cx % 8 + cy % 8 == 7,
                        HatchStyle::Cross => cx % 8 == 0 || cy % 8 == 0,
                        HatchStyle::DiagonalCross => cx % 8 == cy % 8 || cx % 8 + cy % 8 == 7,
                    };
                    if marked {
                        0xff000000
                    } else {
                        0xff00ff00
                    }
                } else {
                    0xffffffff
                };
                assert_eq!(pixel(&bitmap, x, y), expected, "{style:?} ({x},{y})");
            }
        }
    }
}

#[test]
fn axial_shading_extensions_and_translation() {
    for extend in [false, true] {
        let mut bitmap = bitmap();
        let shading = Rc::new(CfgasGeShading::axial(
            CfxPointF::new(8., 0.),
            CfxPointF::new(24., 0.),
            extend,
            extend,
            0xffff0000,
            0xff0000ff,
        ));
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            graphics.set_fill_color(&CfgasGeColor::Shading(shading.clone()));
            assert!(graphics.fill_path(
                &rectangle(0., 0., 32., 32.),
                FillType::Winding,
                CfxMatrix::default()
            ));
        }
        assert_eq!(
            pixel(&bitmap, 4, 16),
            if extend { 0xffff0000 } else { 0xffffffff }
        );
        assert_eq!(pixel(&bitmap, 8, 16), 0xffff0000);
        assert_eq!(pixel(&bitmap, 16, 16), shading.get_argb(0.5));
        assert_eq!(pixel(&bitmap, 24, 16), 0xff0000ff);
        assert_eq!(
            pixel(&bitmap, 28, 16),
            if extend { 0xff0000ff } else { 0xffffffff }
        );
        assert_eq!(pixel(&bitmap, 33, 16), 0xffffffff);
    }
    let mut bitmap = bitmap();
    let shading = Rc::new(CfgasGeShading::axial(
        CfxPointF::new(0., 0.),
        CfxPointF::new(8., 0.),
        true,
        true,
        0xffff0000,
        0xff0000ff,
    ));
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        graphics.set_fill_color(&CfgasGeColor::Shading(shading.clone()));
        assert!(graphics.fill_path(
            &rectangle(0., 0., 8., 8.),
            FillType::Winding,
            CfxMatrix::new(1., 0., 0., 1., 8., 4.)
        ));
    }
    assert_eq!(pixel(&bitmap, 7, 6), 0xffffffff);
    assert_eq!(pixel(&bitmap, 8, 6), 0xffff0000);
    assert_eq!(pixel(&bitmap, 12, 6), shading.get_argb(0.5));
    assert_eq!(pixel(&bitmap, 16, 6), 0xffffffff);
}

#[test]
fn radial_shading_roots_extensions_and_degenerate_axis() {
    let mut bitmap = bitmap();
    let shading = Rc::new(CfgasGeShading::radial(
        CfxPointF::new(16., 16.),
        CfxPointF::new(16., 16.),
        0.,
        8.,
        false,
        false,
        0xffff0000,
        0xff0000ff,
    ));
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        graphics.set_fill_color(&CfgasGeColor::Shading(shading.clone()));
        assert!(graphics.fill_path(
            &rectangle(0., 0., 32., 32.),
            FillType::Winding,
            CfxMatrix::default()
        ));
    }
    assert_eq!(pixel(&bitmap, 16, 16), 0xffff0000);
    assert_eq!(pixel(&bitmap, 20, 16), shading.get_argb(0.5));
    assert_eq!(pixel(&bitmap, 24, 16), 0xff0000ff);
    assert_eq!(pixel(&bitmap, 25, 16), 0xffffffff);
    let shading = CfgasGeShading::radial(
        CfxPointF::new(0., 0.),
        CfxPointF::new(0., 0.),
        0.,
        0.,
        false,
        false,
        0xffff0000,
        0xff0000ff,
    );
    let unchanged = bitmap.buffer().to_vec();
    fill_shading(&mut bitmap, &shading);
    assert_eq!(bitmap.buffer(), unchanged);
    let shading = CfgasGeShading::axial(
        CfxPointF::new(0., 0.),
        CfxPointF::new(0., 0.),
        false,
        false,
        0xffff0000,
        0xff0000ff,
    );
    fill_shading(&mut bitmap, &shading);
    assert_eq!(pixel(&bitmap, 16, 16), 0xffff0000);
}

#[test]
fn invalid_inputs_and_dash_scaling_leave_state_usable() {
    let mut bitmap = bitmap();
    let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
    let mut graphics = CfgasGeGraphics::new(&mut device);
    graphics.set_line_width(3.);
    graphics.enable_act_on_dash();
    assert!(graphics.set_line_dash(vec![1., 2.]));
    assert_eq!(graphics.info.graph_state.dash_array(), &[3., 6.]);
    assert_eq!(graphics.info.graph_state.dash_phase(), 0.);
    assert!(!graphics.set_line_dash(vec![]));
    assert!(!graphics.set_line_dash(vec![f32::NAN]));
    assert!(!graphics.set_line_dash(vec![-1.]));
    assert_eq!(graphics.info.graph_state.dash_array(), &[3., 6.]);
    graphics.set_solid_line_dash();
    assert!(graphics.info.graph_state.dash_array().is_empty());
    let rect = rectangle(0., 0., 8., 8.);
    graphics.set_fill_color(&CfgasGeColor::Solid(0xffff0000));
    assert!(!graphics.fill_path(
        &rect,
        FillType::Winding,
        CfxMatrix::new(f32::NAN, 0., 0., 1., 0., 0.)
    ));
    assert!(!graphics.set_clip_rect(CfxRectF::new(f32::INFINITY, 0., 1., 1.)));
    graphics.set_fill_color(&CfgasGeColor::Shading(Rc::new(CfgasGeShading::axial(
        CfxPointF::new(f32::NAN, 0.),
        CfxPointF::new(1., 0.),
        true,
        true,
        0,
        0,
    ))));
    assert!(!graphics.fill_path(&rect, FillType::Winding, CfxMatrix::default()));
    assert_eq!(graphics.get_clip_rect(), CfxRectF::new(0., 0., 40., 40.));
    graphics.set_fill_color(&CfgasGeColor::Pattern {
        pattern: Rc::new(CfgasGePattern::new(
            HatchStyle::Vertical,
            0xff000000,
            0xff00ff00,
        )),
        argb: 0,
    });
    assert!(graphics.fill_path(
        &rectangle(-1e8, -1e8, 2e8, 2e8),
        FillType::Winding,
        CfxMatrix::default()
    ));
}

#[test]
fn hatch_masks_match_every_pinned_byte() {
    let source = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/xfa/fgas/graphics/cfgas_gegraphics.cpp"
    ));
    let source = source
        .split("constexpr auto kHatchBitmapData")
        .nth(1)
        .unwrap()
        .split("const FX_HATCHDATA kHatchPlaceHolder")
        .next()
        .unwrap();
    let values: Vec<u8> = source
        .split("0x")
        .skip(1)
        .map(|part| {
            let digits: String = part.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
            u8::from_str_radix(&digits, 16).unwrap()
        })
        .collect();
    assert_eq!(values.len(), 384);
    assert_eq!(
        values,
        HATCH_BITMAP_DATA.into_iter().flatten().collect::<Vec<_>>()
    );
}
