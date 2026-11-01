// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Phase 1 `core/fxge/fx_font.{h,cpp}` font constants, metrics, and tables.

use crate::fxcrt::fx_coordinates::{CfxPoint, FxRect};

use super::cfx_face::CfxFace;
use super::text_glyph_pos::TextGlyphPos;

pub const FONT_PITCH_FAMILY_FIXED: u32 = 1 << 0;
pub const FONT_PITCH_FAMILY_ROMAN: u32 = 1 << 4;
pub const FONT_PITCH_FAMILY_SCRIPT: u32 = 1 << 6;

pub const FONT_STYLE_NORMAL: u32 = 0;
pub const FONT_STYLE_FIXED_PITCH: u32 = 1 << 0;
pub const FONT_STYLE_SERIF: u32 = 1 << 1;
pub const FONT_STYLE_SYMBOLIC: u32 = 1 << 2;
pub const FONT_STYLE_SCRIPT: u32 = 1 << 3;
pub const FONT_STYLE_NON_SYMBOLIC: u32 = 1 << 5;
pub const FONT_STYLE_ITALIC: u32 = 1 << 6;
pub const FONT_STYLE_ALL_CAP: u32 = 1 << 16;
pub const FONT_STYLE_SMALL_CAP: u32 = 1 << 17;
pub const FONT_STYLE_FORCE_BOLD: u32 = 1 << 18;

pub const FONT_WEIGHT_EXTRA_LIGHT: u16 = 100;
pub const FONT_WEIGHT_NORMAL: u16 = 400;
pub const FONT_WEIGHT_BOLD: u16 = 700;
pub const FONT_WEIGHT_EXTRA_BOLD: u16 = 900;
pub const FONT_USE_EXTERN_ATTR: u32 = 0x80000;

pub const PLATFORM_APPLE_UNICODE: u16 = 0;
pub const PLATFORM_MAC: u16 = 1;
pub const PLATFORM_ISO: u16 = 2;
pub const PLATFORM_WINDOWS: u16 = 3;
pub const PLATFORM_CUSTOM: u16 = 4;
pub const PLATFORM_ADOBE: u16 = 7;
pub const APPLE_UNICODE_ENCODING_UNICODE_2_0: u16 = 3;
pub const MAC_ENCODING_ROMAN: u16 = 0;
pub const ISO_ENCODING_10646: u16 = 1;
pub const ISO_ENCODING_8859_1: u16 = 2;
pub const WINDOWS_ENCODING_SYMBOL: u16 = 0;
pub const WINDOWS_ENCODING_UNICODE: u16 = 1;
pub const WINDOWS_ENCODING_SJIS: u16 = 2;
pub const WINDOWS_ENCODING_PRC: u16 = 3;
pub const WINDOWS_ENCODING_GB2312: u16 = WINDOWS_ENCODING_PRC;
pub const WINDOWS_ENCODING_BIG5: u16 = 4;
pub const WINDOWS_ENCODING_WANSUNG: u16 = 5;
pub const WINDOWS_ENCODING_JOHAB: u16 = 6;
pub const WINDOWS_ENCODING_UCS4: u16 = 10;
pub const ADOBE_ENCODING_STANDARD: u16 = 0;
pub const ADOBE_ENCODING_EXPERT: u16 = 1;
pub const ADOBE_ENCODING_CUSTOM: u16 = 2;
pub const ADOBE_ENCODING_LATIN1: u16 = 3;

pub const SUBSETTED_FONT_PREFIX_LENGTH: usize = 6;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(i32)]
pub enum FontAntiAliasingMode {
    #[default]
    Normal = 0,
    Mono = 1,
    Lcd = 2,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CharCodeAndIndex {
    pub char_code: u32,
    pub glyph_index: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FontTableLocation {
    pub offset: u32,
    pub size: u32,
}

pub fn get_name_from_tt(name_table: &[u8], name_id: u32) -> Vec<u8> {
    if name_table.len() < 6 {
        return Vec::new();
    }
    let name_count = usize::from(u16::from_be_bytes([name_table[2], name_table[3]]));
    let string_offset = usize::from(u16::from_be_bytes([name_table[4], name_table[5]]));
    let Some(strings) = name_table.get(string_offset..) else {
        return Vec::new();
    };
    let Some(records_len) = name_count.checked_mul(12) else {
        return Vec::new();
    };
    let Some(records) = name_table.get(6..6 + records_len) else {
        return Vec::new();
    };
    for record in records.chunks_exact(12) {
        let record_name_id = u32::from(u16::from_be_bytes([record[6], record[7]]));
        if record_name_id != name_id {
            continue;
        }
        let platform = u16::from_be_bytes([record[0], record[1]]);
        let encoding = u16::from_be_bytes([record[2], record[3]]);
        if !(platform == PLATFORM_MAC && encoding == MAC_ENCODING_ROMAN
            || platform == PLATFORM_WINDOWS && encoding == WINDOWS_ENCODING_UNICODE)
        {
            continue;
        }
        let length = usize::from(u16::from_be_bytes([record[8], record[9]]));
        let offset = usize::from(u16::from_be_bytes([record[10], record[11]]));
        let Some(end) = offset.checked_add(length) else {
            return Vec::new();
        };
        let Some(data) = strings.get(offset..end) else {
            return Vec::new();
        };
        if platform == 1 {
            return data.to_vec();
        }
        if data.is_empty() || data.len() % 2 != 0 {
            return Vec::new();
        }
        let mut result = Vec::with_capacity(data.len());
        let mut units = data
            .chunks_exact(2)
            .map(|pair| u16::from_be_bytes([pair[0], pair[1]]))
            .peekable();
        while let Some(unit) = units.next() {
            let mut code_point = u32::from(unit);
            if (0xd800..=0xdbff).contains(&unit) {
                if let Some(&low) = units.peek() {
                    if (0xdc00..=0xdfff).contains(&low) {
                        units.next();
                        code_point = 0x10000
                            + ((u32::from(unit) - 0xd800) << 10)
                            + (u32::from(low) - 0xdc00);
                    }
                }
            }
            if code_point < 0x80 {
                result.push(code_point as u8);
            } else if code_point < 0x800 {
                result.extend_from_slice(&[
                    (0xc0 | (code_point >> 6)) as u8,
                    (0x80 | (code_point & 0x3f)) as u8,
                ]);
            } else if code_point < 0x10000 {
                result.extend_from_slice(&[
                    (0xe0 | (code_point >> 12)) as u8,
                    (0x80 | ((code_point >> 6) & 0x3f)) as u8,
                    (0x80 | (code_point & 0x3f)) as u8,
                ]);
            } else {
                result.extend_from_slice(&[
                    (0xf0 | (code_point >> 18)) as u8,
                    (0x80 | ((code_point >> 12) & 0x3f)) as u8,
                    (0x80 | ((code_point >> 6) & 0x3f)) as u8,
                    (0x80 | (code_point & 0x3f)) as u8,
                ]);
            }
        }
        return result;
    }
    Vec::new()
}

pub fn get_ttc_index(font_data: &[u8], font_offset: usize) -> u32 {
    let Some(count) = font_data
        .get(8..12)
        .map(|bytes| u32::from_be_bytes(bytes.try_into().expect("four-byte count")))
    else {
        return 0;
    };
    for index in 0..count {
        let Some(start) = (index as usize)
            .checked_mul(4)
            .and_then(|n| n.checked_add(12))
        else {
            break;
        };
        let Some(end) = start.checked_add(4) else {
            break;
        };
        let Some(bytes) = font_data.get(start..end) else {
            break;
        };
        if u32::from_be_bytes(bytes.try_into().expect("four-byte offset")) as usize == font_offset {
            return index;
        }
    }
    0
}

pub fn find_font_table_location(table_dir: &[u8], tag: u32) -> Option<FontTableLocation> {
    for entry in table_dir.chunks_exact(16) {
        if u32::from_be_bytes(entry[..4].try_into().expect("four-byte tag")) == tag {
            return Some(FontTableLocation {
                offset: u32::from_be_bytes(entry[8..12].try_into().expect("four-byte offset")),
                size: u32::from_be_bytes(entry[12..16].try_into().expect("four-byte size")),
            });
        }
    }
    None
}

pub fn get_code_page_range_from_os2(os2_table: &[u8]) -> u32 {
    if os2_table.len() < 86 {
        return 0;
    }
    os2_table.get(78..82).map_or(0, |bytes| {
        u32::from_be_bytes(bytes.try_into().expect("four bytes"))
    })
}

pub fn get_glyph_count_from_maxp(maxp_table: &[u8]) -> u16 {
    maxp_table.get(4..6).map_or(0, |bytes| {
        u16::from_be_bytes(bytes.try_into().expect("two bytes"))
    })
}

pub fn normalize_font_metric(value: i64, upem: u32) -> i32 {
    if upem == 0 {
        return value.clamp(i64::from(i32::MIN), i64::from(i32::MAX)) as i32;
    }
    ((value as f64 * 1000.0 + f64::from(upem / 2)) / f64::from(upem)) as i32
}

pub fn unicode_from_adobe_name(name: &str) -> u32 {
    CfxFace::unicode_from_adobe_name(name)
}

pub fn adobe_name_from_unicode(unicode: u32) -> Vec<u8> {
    CfxFace::adobe_name_from_unicode(unicode)
}

pub fn maybe_remove_subsetted_font_prefix(font_name: &mut Vec<u8>) {
    if font_name.len() > SUBSETTED_FONT_PREFIX_LENGTH
        && font_name[SUBSETTED_FONT_PREFIX_LENGTH] == b'+'
        && font_name[..SUBSETTED_FONT_PREFIX_LENGTH]
            .iter()
            .all(|ch| ch.is_ascii_uppercase())
    {
        font_name.drain(..SUBSETTED_FONT_PREFIX_LENGTH + 1);
    }
}

pub fn is_open_type_cff(data: &[u8]) -> bool {
    data.starts_with(b"OTTO")
}

pub fn font_style_is_force_bold(style: u32) -> bool {
    style & FONT_STYLE_FORCE_BOLD != 0
}

pub fn font_style_is_italic(style: u32) -> bool {
    style & FONT_STYLE_ITALIC != 0
}

pub fn font_style_is_fixed_pitch(style: u32) -> bool {
    style & FONT_STYLE_FIXED_PITCH != 0
}

pub fn font_style_is_symbolic(style: u32) -> bool {
    style & FONT_STYLE_SYMBOLIC != 0
}

pub fn font_style_is_non_symbolic(style: u32) -> bool {
    style & FONT_STYLE_NON_SYMBOLIC != 0
}

pub fn font_style_is_all_caps(style: u32) -> bool {
    style & FONT_STYLE_ALL_CAP != 0
}

pub fn font_style_is_serif(style: u32) -> bool {
    style & FONT_STYLE_SERIF != 0
}

pub fn font_style_is_script(style: u32) -> bool {
    style & FONT_STYLE_SCRIPT != 0
}

pub fn font_family_is_fixed_pitch(family: u32) -> bool {
    family & FONT_PITCH_FAMILY_FIXED != 0
}

pub fn font_family_is_roman(family: u32) -> bool {
    family & FONT_PITCH_FAMILY_ROMAN != 0
}

pub fn font_family_is_script(family: i32) -> bool {
    family & FONT_PITCH_FAMILY_SCRIPT as i32 != 0
}

pub fn get_glyphs_bbox(glyphs: &[TextGlyphPos<'_>], anti_alias_is_lcd: bool) -> FxRect {
    get_glyphs_bbox_iter(glyphs.iter().copied(), anti_alias_is_lcd)
}

pub(crate) fn get_glyphs_bbox_iter<'a>(
    glyphs: impl IntoIterator<Item = TextGlyphPos<'a>>,
    anti_alias_is_lcd: bool,
) -> FxRect {
    let mut rect = FxRect::new(0, 0, 0, 0);
    let mut started = false;
    for glyph in glyphs {
        let Some(bitmap) = glyph.glyph else {
            continue;
        };
        let Some(point) = glyph.get_origin(CfxPoint::new(0, 0)) else {
            continue;
        };
        let bitmap = bitmap.bitmap();
        let char_width = if anti_alias_is_lcd {
            bitmap.width() / 3
        } else {
            bitmap.width()
        };
        let Some(right) = point.x.checked_add(char_width) else {
            continue;
        };
        let Some(bottom) = point.y.checked_add(bitmap.height()) else {
            continue;
        };
        if started {
            rect.left = rect.left.min(point.x);
            rect.top = rect.top.min(point.y);
            rect.right = rect.right.max(right);
            rect.bottom = rect.bottom.max(bottom);
        } else {
            rect = FxRect::new(point.x, point.y, right, bottom);
            started = true;
        }
    }
    rect
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::fxcrt::fx_coordinates::CfxPointF;
    use crate::fxge::cfx_glyphbitmap::CfxGlyphBitmap;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[test]
    fn glyph_bounds_skip_overflow_and_scale_lcd_width() {
        let image = CfxDibitmap::create(6, 2, FxdibFormat::EightBppMask, 0).unwrap();
        let glyph = CfxGlyphBitmap::new(CfxPoint::new(1, 1), Rc::new(RefCell::new(image)));
        let valid = TextGlyphPos {
            glyph: Some(&glyph),
            origin: CfxPoint::new(10, 10),
            device_origin: CfxPointF::new(10.0, 10.0),
        };
        let overflow = TextGlyphPos {
            origin: CfxPoint::new(i32::MAX, 10),
            ..valid
        };
        assert_eq!(
            get_glyphs_bbox(&[valid, overflow], false),
            FxRect::new(11, 9, 17, 11)
        );
        assert_eq!(get_glyphs_bbox(&[valid], true), FxRect::new(11, 9, 13, 11));
    }

    #[test]
    fn font_table_helpers_handle_offsets_and_short_buffers() {
        let mut ttc = vec![0; 20];
        ttc[8..12].copy_from_slice(&2u32.to_be_bytes());
        ttc[12..16].copy_from_slice(&10u32.to_be_bytes());
        ttc[16..20].copy_from_slice(&42u32.to_be_bytes());
        assert_eq!(get_ttc_index(&ttc, 42), 1);
        assert_eq!(get_ttc_index(&ttc[..15], 42), 0);

        let mut dir = [0u8; 16];
        dir[..4].copy_from_slice(b"name");
        dir[8..12].copy_from_slice(&100u32.to_be_bytes());
        dir[12..16].copy_from_slice(&20u32.to_be_bytes());
        assert_eq!(
            find_font_table_location(&dir, u32::from_be_bytes(*b"name")),
            Some(FontTableLocation {
                offset: 100,
                size: 20
            })
        );
        assert_eq!(
            find_font_table_location(&dir[..15], u32::from_be_bytes(*b"name")),
            None
        );
        assert_eq!(get_code_page_range_from_os2(&[0; 85]), 0);
        assert_eq!(get_glyph_count_from_maxp(&[0; 5]), 0);
        assert_eq!(normalize_font_metric(123, 1000), 123);
        assert_eq!(normalize_font_metric(i64::MAX, 0), i32::MAX);
        let mut name = b"ABCDEF+Minion".to_vec();
        maybe_remove_subsetted_font_prefix(&mut name);
        assert_eq!(name, b"Minion");
        assert!(is_open_type_cff(b"OTTO"));
        assert_eq!(unicode_from_adobe_name("A"), u32::from('A'));
        assert_eq!(unicode_from_adobe_name("notaglyph"), 0);
        assert_eq!(adobe_name_from_unicode(u32::from('A')), b"A");
    }

    #[test]
    fn truetype_name_reads_mac_bytes_and_windows_utf16() {
        let mut mac = vec![0, 0, 0, 1, 0, 18];
        mac.extend_from_slice(&[0, 1, 0, 0, 0, 0, 0, 1, 0, 3, 0, 0]);
        mac.extend_from_slice(b"ABC");
        assert_eq!(get_name_from_tt(&mac, 1), b"ABC");

        let mut windows = vec![0, 0, 0, 1, 0, 18];
        windows.extend_from_slice(&[0, 3, 0, 1, 0, 0, 0, 1, 0, 4, 0, 0]);
        windows.extend_from_slice(&[0xd8, 0x3c, 0xdf, 0xa8]);
        assert_eq!(get_name_from_tt(&windows, 1), "🎨".as_bytes());
        windows[15] = 3;
        assert!(get_name_from_tt(&windows, 1).is_empty());
        windows[15] = 2;
        windows[18..20].copy_from_slice(&[0xd8, 0x00]);
        assert_eq!(get_name_from_tt(&windows, 1), &[0xed, 0xa0, 0x80]);
    }
}

#[cfg(test)]
#[path = "fx_font_unittest.rs"]
mod upstream_tests;
