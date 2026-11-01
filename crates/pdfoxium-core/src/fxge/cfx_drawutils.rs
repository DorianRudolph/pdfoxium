// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Focus rectangle drawing from `core/fxge/cfx_drawutils.{h,cpp}`.

use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::{CfxPath, PointType};

pub fn draw_focus_rect(
    driver: &mut CfxAggDeviceDriver<'_, '_>,
    user_to_device: CfxMatrix,
    view_bounding_box: CfxFloatRect,
) {
    let mut path = CfxPath::new();
    let rect = view_bounding_box;
    path.append_point(CfxPointF::new(rect.left, rect.top), PointType::Move);
    for point in [
        CfxPointF::new(rect.left, rect.bottom),
        CfxPointF::new(rect.right, rect.bottom),
        CfxPointF::new(rect.right, rect.top),
        CfxPointF::new(rect.left, rect.top),
    ] {
        path.append_point(point, PointType::Line);
    }
    let mut graph_state = CfxGraphStateData::default();
    graph_state.set_dash_array(vec![1.0]);
    let _ = super::cfx_renderdevice::draw_path_agg(
        driver,
        &path,
        Some(user_to_device),
        Some(&graph_state),
        0,
        0xff00_0000,
        CfxFillRenderOptions::even_odd_options(),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn focus_rectangle_draws_border_without_filling_center() {
        let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
        let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        draw_focus_rect(
            &mut driver,
            CfxMatrix::default(),
            CfxFloatRect::new(2.0, 2.0, 18.0, 18.0),
        );
        assert!(bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] > 0));
        assert_eq!(&bitmap.scanline(10).unwrap()[10 * 4..10 * 4 + 4], &[0; 4]);
    }
}
