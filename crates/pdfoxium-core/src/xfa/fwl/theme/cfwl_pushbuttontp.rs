// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/theme/cfwl_pushbuttontp.{h,cpp}` drawer.
use super::cfwl_widgettp::CfwlWidgetTp;
use crate::fxge::cfx_fillrenderoptions::FillType;
use crate::xfa::fgas::graphics::{
    cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics, cfgas_gepath::CfgasGePath,
};
use crate::xfa::fwl::{
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
};

use crate::fxcrt::fx_coordinates::CfxPointF;

// Complete pinned theme data, including the start colors retained by the
// native interface even though this backend fills the end/inner colors.
pub const BORDER: [u32; 5] = [0xff003c74, 0xff003c74, 0xff003c74, 0xff003c74, 0xffc9c7ba];
pub const START: [u32; 5] = [0xffffffff, 0xffd1ccc1, 0xfffff0cf, 0xffcee7ff, 0xfff5f4ea];
pub const END: [u32; 5] = [0xffd6d0c5, 0xfff2f1ee, 0xffe59700, 0xff6982ee, 0xfff5f4ea];
pub const FILL: [u32; 5] = [0xffffffff, 0xffe2e1da, 0xffffffff, 0xffffffff, 0xfff5f4ea];

pub fn color_id(states: PartState) -> usize {
    // Disabled wins over other flags. Native added these offsets and could
    // index beyond its five-color table for disabled+hover/default states.
    if states.contains(PartState::DISABLED) {
        return 4;
    }
    if states.contains(PartState::DEFAULT) {
        return 3;
    }
    usize::from(states.contains(PartState::HOVERED)) * 2
        + usize::from(states.contains(PartState::PRESSED))
}

pub fn draw_background(params: &CfwlThemeBackground<'_>, graphics: &mut CfgasGeGraphics) {
    match params.part {
        Part::Border => CfwlWidgetTp::draw_border(graphics, params.part_rect, params.matrix),
        Part::Background => {
            let rect = params.part_rect;
            let mut stroke = CfgasGePath::new();
            stroke.move_to(CfxPointF::new(rect.left + 2., rect.top));
            for point in [
                CfxPointF::new(rect.right() - 2., rect.top),
                CfxPointF::new(rect.right(), rect.top + 2.),
                CfxPointF::new(rect.right(), rect.bottom() - 2.),
                CfxPointF::new(rect.right() - 2., rect.bottom()),
                CfxPointF::new(rect.left + 2., rect.bottom()),
                CfxPointF::new(rect.left, rect.bottom() - 2.),
                CfxPointF::new(rect.left, rect.top + 2.),
                CfxPointF::new(rect.left + 2., rect.top),
            ] {
                stroke.line_to(point);
            }
            let mut fill = CfgasGePath::new();
            fill.add_subpath(&stroke);
            let mut state = graphics.state_restorer();
            let mut inner = rect;
            inner.deflate(3., 3., 2., 2.);
            fill.add_rectangle(inner.left, inner.top, inner.width, inner.height);
            let index = color_id(params.states);
            CfwlWidgetTp::fill_solid_rect(&mut state, END[index], rect, params.matrix);
            state.set_stroke_color(&CfgasGeColor::Solid(BORDER[index]));
            state.stroke_path(&stroke, params.matrix);
            fill.clear();
            fill.add_rectangle(inner.left, inner.top, inner.width, inner.height);
            state.set_fill_color(&CfgasGeColor::Solid(FILL[index]));
            state.fill_path(&fill, FillType::Winding, params.matrix);
            if params.states.contains(PartState::FOCUSED) {
                inner.inflate(1., 1., 0., 0.);
                CfwlWidgetTp::draw_focus(&mut state, inner, params.matrix);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn button_palette_retains_all_source_entries_and_bounds_mixed_states() {
        assert_eq!(
            BORDER,
            [0xff003c74, 0xff003c74, 0xff003c74, 0xff003c74, 0xffc9c7ba]
        );
        assert_eq!(
            START,
            [0xffffffff, 0xffd1ccc1, 0xfffff0cf, 0xffcee7ff, 0xfff5f4ea]
        );
        assert_eq!(
            END,
            [0xffd6d0c5, 0xfff2f1ee, 0xffe59700, 0xff6982ee, 0xfff5f4ea]
        );
        assert_eq!(
            FILL,
            [0xffffffff, 0xffe2e1da, 0xffffffff, 0xffffffff, 0xfff5f4ea]
        );
        for bits in 0..=u16::MAX {
            let state = PartState(bits);
            let index = color_id(state);
            assert!(index < 5);
            if state.contains(PartState::DISABLED) {
                assert_eq!(index, 4);
            } else if state.contains(PartState::DEFAULT) {
                assert_eq!(index, 3);
            } else {
                let hovered = state.contains(PartState::HOVERED);
                let pressed = state.contains(PartState::PRESSED);
                assert_eq!(
                    index,
                    match (hovered, pressed) {
                        (false, false) => 0,
                        (false, true) => 1,
                        (true, false) => 2,
                        (true, true) => 3,
                    }
                );
            }
        }
    }
}
