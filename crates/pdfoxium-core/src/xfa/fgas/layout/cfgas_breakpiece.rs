// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source CFGAS_BreakPiece. Explicit owner slices replace its unowned pointer.
use super::{
    cfgas_char::{BreakType, CfgasChar},
    cfgas_textuserdata::CfgasTextUserData,
};
use std::rc::Rc;
#[derive(Clone)]
pub struct CfgasBreakPiece {
    pub status: BreakType,
    pub start_pos: i32,
    pub width: i32,
    pub start_char: i32,
    pub char_count: i32,
    pub bidi_level: i32,
    pub bidi_pos: i32,
    pub font_size: i32,
    pub horizontal_scale: i32,
    pub vertical_scale: i32,
    pub char_styles: u32,
    pub user_data: Option<Rc<CfgasTextUserData>>,
}
impl Default for CfgasBreakPiece {
    fn default() -> Self {
        Self {
            status: BreakType::Piece,
            start_pos: 0,
            width: -1,
            start_char: 0,
            char_count: 0,
            bidi_level: 0,
            bidi_pos: 0,
            font_size: 0,
            horizontal_scale: 100,
            vertical_scale: 100,
            char_styles: 0,
            user_data: None,
        }
    }
}
impl CfgasBreakPiece {
    pub fn get_end_pos(&self) -> i32 {
        if self.width < 0 {
            self.start_pos
        } else {
            self.start_pos.saturating_add(self.width)
        }
    }
    pub fn get_length(&self) -> usize {
        self.char_count.max(0) as usize
    }
    pub fn chars<'a>(&self, chars: &'a [CfgasChar]) -> &'a [CfgasChar] {
        let start = self.start_char.max(0) as usize;
        chars
            .get(start..start.saturating_add(self.get_length()))
            .unwrap_or(&[])
    }
    pub fn get_char<'a>(&self, chars: &'a [CfgasChar], index: usize) -> Option<&'a CfgasChar> {
        self.chars(chars).get(index)
    }
    pub fn get_string(&self, chars: &[CfgasChar]) -> Vec<u32> {
        self.chars(chars)
            .iter()
            .map(|ch| u32::from(ch.char_code()))
            .collect()
    }
    pub fn get_widths(&self, chars: &[CfgasChar]) -> Vec<i32> {
        self.chars(chars).iter().map(|ch| ch.char_width).collect()
    }
}
