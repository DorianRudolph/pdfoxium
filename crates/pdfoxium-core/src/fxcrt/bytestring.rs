// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! String operations from `core/fxcrt/bytestring.{h,cpp}` that differ from
//! ordinary Rust slices. Owned strings use Vec<u8>; length-bearing views use
//! slices and retain embedded NULs. Only native C-string operations truncate.

pub fn c_str(bytes: &[u8]) -> &[u8] {
    &bytes[..bytes
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(bytes.len())]
}

/// ByteString's operator==(const char*) uses strcmp, unlike its view overload.
pub fn eq_cstr(bytes: &[u8], other: &[u8]) -> bool {
    c_str(bytes) == c_str(other)
}

/// ByteStringView::GetID zero-pads or truncates to four bytes, including NULs.
pub fn get_id(bytes: &[u8]) -> u32 {
    u32::from_be_bytes(std::array::from_fn(|index| {
        bytes.get(index).copied().unwrap_or(0)
    }))
}

/// ByteString::Format("%c", value) releases its buffer at the first NUL.
pub fn format_character(value: u16) -> Vec<u8> {
    let byte = value as u8;
    if byte == 0 {
        Vec::new()
    } else {
        vec![byte]
    }
}

/// FX_HashCode_GetA, the native ByteString hash's unsigned-byte polynomial.
pub fn hash_code(bytes: &[u8]) -> u32 {
    let mut hash = 0u32;
    for &c in bytes {
        hash = hash.wrapping_mul(31).wrapping_add(u32::from(c));
    }
    hash
}

/// FX_HashCode_GetLoweredA in the selected portable C-locale configuration.
pub fn hash_code_lowered(bytes: &[u8]) -> u32 {
    let mut hash = 0u32;
    for &c in bytes {
        hash = hash
            .wrapping_mul(31)
            .wrapping_add(u32::from(c.to_ascii_lowercase()));
    }
    hash
}

/// FX_HashCode_GetAsIfW / GetLoweredAsIfW avoid a temporary wide string.
pub fn hash_code_as_if_w(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |hash, &byte| {
        hash.wrapping_mul(1313).wrapping_add(u32::from(byte))
    })
}
pub fn hash_code_lowered_as_if_w(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0u32, |hash, &byte| {
        hash.wrapping_mul(1313)
            .wrapping_add(super::fx_extension::to_lower_wide(u32::from(byte)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn c_strings_and_views_keep_distinct_boundaries() {
        assert!(eq_cstr(b"Page\0tail", b"Page"));
        assert!(!eq_cstr(b"PageX", b"Page"));
        assert!(eq_cstr(b"\0tail", b""));
        assert_eq!(get_id(b"A\0BCtail"), u32::from_be_bytes(*b"A\0BC"));
        assert_eq!(get_id(b"Hue"), u32::from_be_bytes(*b"Hue\0"));
        assert!(format_character(0x100).is_empty());
        assert_eq!(format_character(0x1ff), [0xff]);
    }
}
