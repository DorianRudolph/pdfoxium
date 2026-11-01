// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/fx_font_unittest.cpp`.
#![allow(non_snake_case)]

use super::*;
use crate::fxge::cfx_folderfontinfo::CfxFolderFontInfo;
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use std::path::Path;

#[test]
fn UnicodeFromAdobeName() {
    assert_eq!(unicode_from_adobe_name("nonesuch"), 0x0000);
    assert_eq!(unicode_from_adobe_name(""), 0x0000);
    assert_eq!(unicode_from_adobe_name("paragraph"), 0x00b6);
    assert_eq!(unicode_from_adobe_name("Oacute"), 0x00d3);
    assert_eq!(unicode_from_adobe_name("thorn"), 0x00fe);
    assert_eq!(unicode_from_adobe_name("tonos"), 0x0384);
    assert_eq!(unicode_from_adobe_name("bullet"), 0x2022);
}

#[test]
fn AdobeNameFromUnicode() {
    assert_eq!(adobe_name_from_unicode(0x0000), b"");
    assert_eq!(adobe_name_from_unicode(0x00f7), b"divide");
    assert_eq!(adobe_name_from_unicode(0x0141), b"Lslash");
    assert_eq!(adobe_name_from_unicode(0x0384), b"tonos");
    assert_eq!(adobe_name_from_unicode(0x0691), b"afii57513");
    assert_eq!(adobe_name_from_unicode(0x0e5a), b"angkhankhuthai");
    assert_eq!(adobe_name_from_unicode(0x20ac), b"Euro");
}

#[test]
fn ReadFontNameFromMicrosoftEntries() {
    let test_data_dir =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../upstream/pdfium/testing/resources");
    assert!(!test_data_dir.as_os_str().is_empty());
    let mut font_mapper = CfxFontMapper::new();
    {
        // Preserve the original destruction order: folder info before mapper.
        let mut folder_font_info = CfxFolderFontInfo::new();
        folder_font_info.add_path(test_data_dir.join("font_tests"));
        // The portable folder backend is the selected default system-font
        // provider. Enumeration passes its results to the mapper explicitly.
        font_mapper.set_system_font_info(Some(Box::new(CfxFolderFontInfo::new())));
        for (name, charset) in folder_font_info.enum_font_list() {
            font_mapper.add_installed_font(&name, charset);
        }
    }
    assert_eq!(font_mapper.face_count(), 1);
    assert_eq!(font_mapper.face_name(0), Some(&b"Test"[..]));
}

#[test]
fn FindFontTableLocation() {
    let table_dir = [
        b'h', b'e', b'a', b'd', 0, 0, 0, 0, 0, 0, 0, 0x2c, 0, 0, 0, 0x0a, b'n', b'a', b'm', b'e',
        0, 0, 0, 0, 0, 0, 0, 0x36, 0, 0, 0, 0x14,
    ];
    let head_tag = u32::from_be_bytes(*b"head");
    let head_loc = find_font_table_location(&table_dir, head_tag);
    assert!(head_loc.is_some());
    let head_loc = head_loc.unwrap();
    assert_eq!(head_loc.offset, 44);
    assert_eq!(head_loc.size, 10);
    let name_tag = u32::from_be_bytes(*b"name");
    let name_loc = find_font_table_location(&table_dir, name_tag);
    assert!(name_loc.is_some());
    let name_loc = name_loc.unwrap();
    assert_eq!(name_loc.offset, 54);
    assert_eq!(name_loc.size, 20);
    let gasp_tag = u32::from_be_bytes(*b"gasp");
    let gasp_loc = find_font_table_location(&table_dir, gasp_tag);
    assert!(gasp_loc.is_none());
    let truncated_span = &table_dir[..20];
    let head_loc_trunc = find_font_table_location(truncated_span, head_tag);
    assert!(head_loc_trunc.is_some());
    let head_loc_trunc = head_loc_trunc.unwrap();
    assert_eq!(head_loc_trunc.offset, 44);
    assert_eq!(head_loc_trunc.size, 10);
    let name_loc_trunc = find_font_table_location(truncated_span, name_tag);
    assert!(name_loc_trunc.is_none());
    let short_span = &table_dir[..10];
    let head_loc_short = find_font_table_location(short_span, head_tag);
    assert!(head_loc_short.is_none());
}

#[test]
fn GetCodePageRangeFromOS2() {
    let short_os2 = vec![0; 85];
    assert_eq!(get_code_page_range_from_os2(&short_os2), 0);
    let mut mock_os2 = vec![0; 86];
    mock_os2[78] = 0x12;
    mock_os2[79] = 0x34;
    mock_os2[80] = 0x56;
    mock_os2[81] = 0x78;
    assert_eq!(get_code_page_range_from_os2(&mock_os2), 0x12345678);
}

#[test]
fn GetGlyphCountFromMaxp() {
    let short_maxp = vec![0; 5];
    assert_eq!(get_glyph_count_from_maxp(&short_maxp), 0);
    let mut mock_maxp = vec![0; 6];
    mock_maxp[4] = 0x12;
    mock_maxp[5] = 0x34;
    assert_eq!(get_glyph_count_from_maxp(&mock_maxp), 0x1234);
}
