// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Platform font provider contract from `core/fxge/systemfontinfo_iface.h`.
//! The provider owns opaque handles and returns installed font entries from
//! enumeration so the mapper can process them without aliasing the provider.

use crate::fxcrt::fx_codepage::FxCharset;

pub const TABLE_NAME: u32 = u32::from_be_bytes(*b"name");
pub const TABLE_TTCF: u32 = u32::from_be_bytes(*b"ttcf");
pub const TABLE_NONE: u32 = 0;

pub trait SystemFontInfo {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)>;
    fn map_font(
        &mut self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch_family: u32,
        face: &[u8],
    ) -> Option<u64>;
    fn get_font(&mut self, face: &[u8]) -> Option<u64>;
    /// Returns the required table size for an empty buffer, or bytes read.
    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize;
    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>>;
    fn get_font_charset(&mut self, font: u64) -> Option<FxCharset>;
    fn delete_font(&mut self, font: u64);
}
