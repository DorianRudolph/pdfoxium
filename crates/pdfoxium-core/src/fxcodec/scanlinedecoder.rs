// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fxcodec/scanlinedecoder.h` and `.cpp`. Rust keeps the
//! last scanline in the decoder rather than storing a self-referential span.

use std::rc::Rc;

use crate::fxcrt::pauseindicator_iface::PauseIndicatorIface;

// A native source span can borrow caller memory or retain the accessor's
// immutable allocation when a decoder outlives the initial accessor borrow.
pub(crate) enum ScanlineSource<'a> {
    Borrowed(&'a [u8]),
    Shared(Rc<Vec<u8>>),
}

impl std::ops::Deref for ScanlineSource<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Borrowed(data) => data,
            Self::Shared(data) => data,
        }
    }
}

pub struct ScanlineState {
    pub orig_width: i32,
    pub orig_height: i32,
    pub output_width: i32,
    pub output_height: i32,
    pub comps: i32,
    pub bpc: i32,
    pub pitch: u32,
    pub scanlines_are_bgr: bool,
    pub next_line: i32,
    pub has_last_scanline: bool,
}

impl Default for ScanlineState {
    fn default() -> Self {
        Self::new(0, 0, 0, 0, 0, 0, 0)
    }
}

impl ScanlineState {
    pub fn new(
        orig_width: i32,
        orig_height: i32,
        output_width: i32,
        output_height: i32,
        comps: i32,
        bpc: i32,
        pitch: u32,
    ) -> Self {
        Self {
            orig_width,
            orig_height,
            output_width,
            output_height,
            comps,
            bpc,
            pitch,
            scanlines_are_bgr: false,
            next_line: -1,
            has_last_scanline: false,
        }
    }
}

pub trait ScanlineDecoder {
    fn state(&self) -> &ScanlineState;
    fn state_mut(&mut self) -> &mut ScanlineState;
    fn rewind(&mut self) -> bool;
    /// Returns whether the decoder produced a nonempty scanline.
    fn get_next_line(&mut self) -> bool;
    fn current_line(&self) -> &[u8];
    fn get_src_offset(&self) -> u32;

    fn get_width(&self) -> i32 {
        self.state().output_width
    }
    fn get_height(&self) -> i32 {
        self.state().output_height
    }
    fn count_comps(&self) -> i32 {
        self.state().comps
    }
    fn get_bpc(&self) -> i32 {
        self.state().bpc
    }
    fn scanlines_are_bgr(&self) -> bool {
        self.state().scanlines_are_bgr
    }

    fn get_scanline(&mut self, line: i32) -> &[u8] {
        if self.state().next_line == line.wrapping_add(1) {
            return if self.state().has_last_scanline {
                self.current_line()
            } else {
                &[]
            };
        }
        if self.state().next_line < 0 || self.state().next_line > line {
            if !self.rewind() {
                return &[];
            }
            self.state_mut().next_line = 0;
        }
        while self.state().next_line < line {
            self.get_next_line();
            self.state_mut().next_line += 1;
        }
        let has_line = self.get_next_line();
        self.state_mut().has_last_scanline = has_line;
        self.state_mut().next_line += 1;
        if has_line {
            self.current_line()
        } else {
            &[]
        }
    }

    fn skip_to_scanline(
        &mut self,
        line: i32,
        mut pause: Option<&mut dyn PauseIndicatorIface>,
    ) -> bool {
        if self.state().next_line == line || self.state().next_line == line.wrapping_add(1) {
            return false;
        }
        if self.state().next_line < 0 || self.state().next_line > line {
            if !self.rewind() {
                return false;
            }
            self.state_mut().next_line = 0;
        }
        self.state_mut().has_last_scanline = false;
        while self.state().next_line < line {
            let has_line = self.get_next_line();
            self.state_mut().has_last_scanline = has_line;
            self.state_mut().next_line += 1;
            if pause.as_mut().is_some_and(|p| p.need_to_pause_now()) {
                return true;
            }
        }
        false
    }
}
