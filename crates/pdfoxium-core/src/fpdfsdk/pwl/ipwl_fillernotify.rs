// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2020 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Host callbacks from `fpdfsdk/pwl/ipwl_fillernotify.h`.
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use std::any::Any;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum CursorStyle {
    #[default]
    Arrow = 0,
    NESW = 1,
    NWSE = 2,
    VBeam = 3,
    HBeam = 4,
    Hand = 5,
}
/// Per-window caller data is owned by a window and independently cloned for its
/// children, as in the C++ virtual Clone contract.
pub trait PerWindowData: Any {
    fn as_any(&self) -> &dyn Any;
    fn clone_data(&self) -> Box<dyn PerWindowData>;
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BeforeKeystrokeResult {
    pub rc: bool,
    pub exit: bool,
}
pub trait FillerNotify {
    fn invalidate_rect(&self, data: Option<&dyn PerWindowData>, rect: CfxFloatRect);
    fn output_selected_rect(&self, data: Option<&dyn PerWindowData>, rect: CfxFloatRect);
    fn is_selection_implemented(&self) -> bool;
    fn set_cursor(&self, cursor: CursorStyle);
    /// Returns (place below, popup extent), replacing the two required outputs.
    fn query_where_popup(
        &self,
        data: Option<&dyn PerWindowData>,
        min: f32,
        max: f32,
    ) -> (bool, f32);
    fn on_before_key_stroke(
        &self,
        data: Option<&dyn PerWindowData>,
        change: &mut Vec<u32>,
        change_ex: &[u32],
        selection_start: i32,
        selection_end: i32,
        key_down: bool,
        flags: u32,
    ) -> BeforeKeystrokeResult;
    fn on_popup_pre_open(&self, data: Option<&dyn PerWindowData>, flags: u32) -> bool;
    fn on_popup_post_open(&self, data: Option<&dyn PerWindowData>, flags: u32) -> bool;
}
