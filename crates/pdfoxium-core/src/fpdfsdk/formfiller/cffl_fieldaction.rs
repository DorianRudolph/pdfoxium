// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Field event data from `fpdfsdk/formfiller/cffl_fieldaction.{h,cpp}`.
pub struct CfflFieldAction {
    pub b_modifier: bool,
    pub b_shift: bool,
    pub b_key_down: bool,
    pub b_will_commit: bool,
    pub b_field_full: bool,
    pub b_rc: bool,
    pub n_sel_end: i32,
    pub n_sel_start: i32,
    pub s_change: Vec<u32>,
    pub s_change_ex: Vec<u32>,
    pub s_value: Vec<u32>,
}
impl Default for CfflFieldAction {
    fn default() -> Self {
        Self {
            b_modifier: false,
            b_shift: false,
            b_key_down: false,
            b_will_commit: false,
            b_field_full: false,
            b_rc: true,
            n_sel_end: 0,
            n_sel_start: 0,
            s_change: Vec::new(),
            s_change_ex: Vec::new(),
            s_value: Vec::new(),
        }
    }
}
