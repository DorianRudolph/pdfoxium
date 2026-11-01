// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
// Original code is licensed as follows:
/*
 * Copyright 2006-2007 Jeremias Maerki.
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

use super::bc_encoder::Encoding;
use super::bc_symbol_info::CbcSymbolInfo;
use crate::fxcrt::fx_string::utf8_encode;
pub(crate) struct CbcEncoderContext {
    pub msg: Vec<u8>,
    pub codewords: Vec<u8>,
    pub pos: usize,
    pub new_encoding: Option<Encoding>,
    pub symbol_info: Option<&'static CbcSymbolInfo>,
    allow_rectangular: bool,
    skip_at_end: usize,
}
impl CbcEncoderContext {
    pub fn new(msg: &[u32]) -> Self {
        let msg = utf8_encode(msg);
        let codewords = Vec::with_capacity(msg.len());
        Self {
            msg,
            codewords,
            pos: 0,
            new_encoding: None,
            symbol_info: None,
            allow_rectangular: false,
            skip_at_end: 0,
        }
    }
    pub fn set_skip_at_end(&mut self, count: usize) {
        self.skip_at_end = count;
    }
    pub fn current(&self) -> u8 {
        self.msg[self.pos]
    }
    pub fn has_more_characters(&self) -> bool {
        self.pos < self.msg.len().saturating_sub(self.skip_at_end)
    }
    pub fn remaining_characters(&self) -> usize {
        self.msg
            .len()
            .saturating_sub(self.skip_at_end)
            .saturating_sub(self.pos)
    }
    pub fn update_symbol_info(&mut self, len: usize) -> bool {
        if self.symbol_info.is_none_or(|info| len > info.data_capacity) {
            self.symbol_info = CbcSymbolInfo::lookup(len, self.allow_rectangular);
        }
        self.symbol_info.is_some()
    }
    pub fn capacity(&self) -> usize {
        self.symbol_info
            .expect("symbol info checked before capacity")
            .data_capacity
    }
    pub fn reset_symbol_info(&mut self) {
        self.allow_rectangular = true;
    }
}
