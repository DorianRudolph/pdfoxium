// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Translation of `core/fxcrt/byteorder.h`. Fixed-size arrays retain the
//! upstream span length contract without alignment requirements.

#[inline]
pub const fn byte_swap16(value: u16) -> u16 {
    value.swap_bytes()
}
#[inline]
pub const fn byte_swap32(value: u32) -> u32 {
    value.swap_bytes()
}
#[inline]
pub const fn from_le16(value: u16) -> u16 {
    u16::from_le(value)
}
#[inline]
pub const fn from_le32(value: u32) -> u32 {
    u32::from_le(value)
}
#[inline]
pub const fn from_be16(value: u16) -> u16 {
    u16::from_be(value)
}
#[inline]
pub const fn from_be32(value: u32) -> u32 {
    u32::from_be(value)
}
#[inline]
pub const fn get_uint16_msb_first(span: &[u8; 2]) -> u16 {
    u16::from_be_bytes(*span)
}
#[inline]
pub const fn get_uint32_msb_first(span: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*span)
}
#[inline]
pub const fn get_uint16_lsb_first(span: &[u8; 2]) -> u16 {
    u16::from_le_bytes(*span)
}
#[inline]
pub const fn get_uint32_lsb_first(span: &[u8; 4]) -> u32 {
    u32::from_le_bytes(*span)
}
#[inline]
pub fn put_uint16_msb_first(value: u16, span: &mut [u8; 2]) {
    *span = value.to_be_bytes();
}
#[inline]
pub fn put_uint32_msb_first(value: u32, span: &mut [u8; 4]) {
    *span = value.to_be_bytes();
}
#[inline]
pub fn put_uint16_lsb_first(value: u16, span: &mut [u8; 2]) {
    *span = value.to_le_bytes();
}
#[inline]
pub fn put_uint32_lsb_first(value: u32, span: &mut [u8; 4]) {
    *span = value.to_le_bytes();
}

#[cfg(test)]
#[path = "byteorder_unittest.rs"]
mod upstream_tests;
