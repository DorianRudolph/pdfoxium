// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/cfx_fontmapper_unittest.cpp`, including XFA.
//! A shared expectation queue checks the GMock call arguments and sequence.
#![allow(non_snake_case)]

use super::*;
use std::cell::RefCell;
use std::collections::VecDeque;

const FONT_HANDLE: u64 = 12345;

enum Call {
    EnumFontList,
    MapFont(i32, bool, FxCharset, u32, &'static [u8]),
    MapFontFailure,
    GetFaceName,
    GetFontData {
        table: u32,
        size: usize,
        result: usize,
        data: Option<Rc<Vec<u8>>>,
    },
    DeleteFont,
}

struct MockSystemFontInfo {
    calls: Rc<RefCell<VecDeque<Call>>>,
}
impl MockSystemFontInfo {
    fn next(&self) -> Call {
        self.calls
            .borrow_mut()
            .pop_front()
            .expect("unexpected provider call")
    }
}
impl SystemFontInfo for MockSystemFontInfo {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        assert!(matches!(self.next(), Call::EnumFontList));
        Vec::new()
    }
    fn map_font(
        &mut self,
        weight: i32,
        italic: bool,
        charset: FxCharset,
        pitch: u32,
        family: &[u8],
    ) -> Option<u64> {
        let expected = self.next();
        if matches!(expected, Call::MapFontFailure) {
            return None;
        }
        let Call::MapFont(w, i, c, p, f) = expected else {
            panic!("expected MapFont")
        };
        assert_eq!((w, i, c, p, f), (weight, italic, charset, pitch, family));
        Some(FONT_HANDLE)
    }
    fn get_font(&mut self, _: &[u8]) -> Option<u64> {
        None
    }
    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
        assert_eq!(FONT_HANDLE, font);
        let Call::GetFontData {
            table: t,
            size,
            result,
            data,
        } = self.next()
        else {
            panic!("expected GetFontData")
        };
        assert_eq!(t, table);
        assert_eq!(size, buffer.len());
        if let Some(data) = data {
            buffer.copy_from_slice(&data);
        } else if result != 0 && !buffer.is_empty() {
            buffer.fill(0);
        }
        result
    }
    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
        assert_eq!(FONT_HANDLE, font);
        assert!(matches!(self.next(), Call::GetFaceName));
        None
    }
    fn get_font_charset(&mut self, _: u64) -> Option<FxCharset> {
        None
    }
    fn delete_font(&mut self, font: u64) {
        assert_eq!(FONT_HANDLE, font);
        assert!(matches!(self.next(), Call::DeleteFont));
    }
}

struct CFXFontMapperSystemFontInfoTest {
    // The mapper owns the provider; the shared queue is only expectation state.
    font_mapper: CfxFontMapper,
    calls: Rc<RefCell<VecDeque<Call>>>,
}
impl CFXFontMapperSystemFontInfoTest {
    fn new() -> Self {
        let mut font_mapper = CfxFontMapper::new();
        let calls = Rc::new(RefCell::new(VecDeque::new()));
        font_mapper.set_system_font_info(Some(Box::new(MockSystemFontInfo {
            calls: calls.clone(),
        })));
        font_mapper.add_installed_font(b"dummy", FxCharset::Ansi);
        Self { font_mapper, calls }
    }
    fn expect(&self, calls: impl IntoIterator<Item = Call>) {
        self.calls.borrow_mut().extend(calls);
    }
    fn verify(&self) {
        assert!(
            self.calls.borrow().is_empty(),
            "unconsumed provider expectations"
        );
    }
}
fn data_call(table: u32, size: usize, result: usize) -> Call {
    Call::GetFontData {
        table,
        size,
        result,
        data: None,
    }
}

#[test]
fn MakeTag() {
    // Native MakeTag is represented by u32::from_be_bytes in production.
    assert_eq!(0x61626364u32, u32::from_be_bytes(*b"abcd"));
    assert_eq!(0x00000000u32, u32::from_be_bytes([0, 0, 0, 0]));
    assert_eq!(0xfffe0a08u32, u32::from_be_bytes([0xff, 0xfe, b'\n', 8]));
    assert_eq!(0xffffffffu32, u32::from_be_bytes([0xff, 0xff, 0xff, 0xff]));
    // Keep the native non-constant argument without a mutable global.
    let g_maybe_changes = std::hint::black_box(0xffu8);
    assert_eq!(
        0xffffffffu32,
        u32::from_be_bytes([g_maybe_changes, 0xff, 0xff, 0xff])
    );
    assert_eq!(0x6e616d65u32, u32::from_be_bytes(*b"name"));
    assert_eq!(0x4f532f32u32, u32::from_be_bytes(*b"OS/2"));
}

#[test]
fn AddInstalledFontBasic() {
    let mut font_mapper = CfxFontMapper::new();
    let calls = Rc::new(RefCell::new(VecDeque::new()));
    font_mapper.set_system_font_info(Some(Box::new(MockSystemFontInfo {
        calls: calls.clone(),
    })));
    let font_name = b"dummy";
    font_mapper.add_installed_font(font_name, FxCharset::Ansi);
    assert_eq!(1usize, font_mapper.face_count());
    assert_eq!(font_name, font_mapper.face_name(0).unwrap());
    assert!(calls.borrow().is_empty());
}

// The four native PDF_ENABLE_XFA RawBytesForIndex cases retain their literal
// two-byte payload, opaque handle and exact Map/GetData/Delete sequence.
#[test]
fn RawBytesForIndex() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    fixture.expect([
        Call::MapFont(0, false, FxCharset::Default, 0, b"dummy"),
        data_call(TABLE_NONE, 0, 2),
        Call::GetFontData {
            table: TABLE_NONE,
            size: 2,
            result: 2,
            data: Some(Rc::new(b"01".to_vec())),
        },
        Call::DeleteFont,
    ]);
    assert_eq!(fixture.font_mapper.raw_bytes_for_index(0), b"01");
    fixture.verify();
}
#[test]
fn RawBytesForIndexFailToMap() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    fixture.expect([Call::MapFontFailure]);
    assert!(fixture.font_mapper.raw_bytes_for_index(0).is_empty());
    fixture.verify();
}
#[test]
fn RawBytesForIndexFailToGetDataSize() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    fixture.expect([
        Call::MapFont(0, false, FxCharset::Default, 0, b"dummy"),
        data_call(TABLE_NONE, 0, 0),
        Call::DeleteFont,
    ]);
    assert!(fixture.font_mapper.raw_bytes_for_index(0).is_empty());
    fixture.verify();
}
#[test]
fn RawBytesForIndexFailToGetData() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    fixture.expect([
        Call::MapFont(0, false, FxCharset::Default, 0, b"dummy"),
        data_call(TABLE_NONE, 0, 2),
        data_call(TABLE_NONE, 2, 0),
        Call::DeleteFont,
    ]);
    assert!(fixture.font_mapper.raw_bytes_for_index(0).is_empty());
    fixture.verify();
}

// Regression test for crbug.com/1372234 - should not crash.
#[test]
fn GetCachedTTCFaceFailToGetData() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    let ttc_size = 1024;
    let data_size = 2;
    fixture.expect([data_call(TABLE_TTCF, ttc_size, 0)]);
    assert!(fixture
        .font_mapper
        .get_cached_ttc_face(FONT_HANDLE, ttc_size, data_size)
        .is_none());
    fixture.verify();
}

// Regression test for crbug.com/565837689 - should not crash.
#[test]
fn GetCachedTTCFaceWithLargeTtc() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    let ttc_size = 2048;
    let data_size = 2;
    for _ in 0..2 {
        // Read all kTtcSize zero bytes again after the failed face construction.
        // Native leaves an expired cache entry, exercising checksum comparisons
        // on the second iteration. Rust retains its corresponding cache key.
        fixture.expect([data_call(TABLE_TTCF, ttc_size, ttc_size)]);
        assert!(fixture
            .font_mapper
            .get_cached_ttc_face(FONT_HANDLE, ttc_size, data_size)
            .is_none());
        fixture.verify();
    }
}

// Regression test for crbug.com/1372234 - should not crash.
#[test]
fn GetCachedFaceFailToGetData() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    let subst_name = b"placeholder_font";
    let weight = 400;
    let italic = false;
    let data_size = 2;
    fixture.expect([data_call(TABLE_NONE, data_size, 0)]);
    assert!(fixture
        .font_mapper
        .get_cached_face(FONT_HANDLE, subst_name, weight, italic, data_size)
        .is_none());
    fixture.verify();
}

fn font_data() -> Rc<Vec<u8>> {
    let font_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf"
    ));
    assert!(!font_path.as_os_str().is_empty());
    let data = std::fs::read(font_path).unwrap();
    assert!(!data.is_empty());
    Rc::new(data)
}
fn expect_external_subst(fixture: &CFXFontMapperSystemFontInfoTest, data: Rc<Vec<u8>>) {
    fixture.expect([
        Call::GetFaceName,
        data_call(TABLE_TTCF, 0, 0),
        data_call(TABLE_NONE, 0, data.len()),
        Call::GetFontData {
            table: TABLE_NONE,
            size: data.len(),
            result: data.len(),
            data: Some(data),
        },
        Call::DeleteFont,
    ]);
}

#[test]
fn SetSubstFontNameWhenGetFaceNameFails() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    let font_data = font_data();
    expect_external_subst(&fixture, font_data);
    let subst_name = b"placeholder_font";
    let weight = 400;
    let italic = false;
    let italic_angle = 0;
    let charset = FxCharset::Ansi;
    let mut subst_font = CfxSubstFont::new();
    assert!(fixture
        .font_mapper
        .use_external_subst(
            FONT_HANDLE,
            subst_name.to_vec(),
            weight,
            italic,
            italic_angle,
            charset,
            &mut subst_font
        )
        .is_some());
    assert_eq!(b"Noto Sans SC Regular", subst_font.family());
    fixture.verify();
}

#[test]
fn FindSubstFaceForRegularStandardFontWithBoldWeight() {
    let mut fixture = CFXFontMapperSystemFontInfoTest::new();
    let font_data = font_data();
    // TODO(crbug.com/500640684): Should be 400.
    fixture.expect([
        Call::EnumFontList,
        Call::MapFont(700, true, FxCharset::Ansi, 0, b"Helvetica-Oblique"),
    ]);
    expect_external_subst(&fixture, font_data);
    let face_name = b"Arial-ItalicMT";
    let bold_weight = 700;
    let italic_angle = 0;
    let code_page = FxCodePage::DefAnsi;
    let mut subst_font = CfxSubstFont::new();
    let face = fixture.font_mapper.find_subst_face(
        face_name,
        true,
        FONT_USE_EXTERN_ATTR,
        bold_weight,
        italic_angle,
        code_page,
        &mut subst_font,
    );
    assert!(face.is_some());
    assert_eq!(b"Noto Sans SC Regular", subst_font.family());
    fixture.verify();
}

fn mapper_with_mock() -> (CfxFontMapper, Rc<RefCell<VecDeque<Call>>>) {
    let mut font_mapper = CfxFontMapper::new();
    let calls = Rc::new(RefCell::new(VecDeque::new()));
    font_mapper.set_system_font_info(Some(Box::new(MockSystemFontInfo {
        calls: calls.clone(),
    })));
    (font_mapper, calls)
}
#[test]
fn LoadInstalledFontsWithEnumeration() {
    let (mut font_mapper, calls) = mapper_with_mock();
    calls.borrow_mut().push_back(Call::EnumFontList);
    font_mapper.load_installed_fonts();
    assert!(calls.borrow().is_empty());
}
#[test]
fn LoadInstalledFontsSkipEnumeration() {
    let (mut font_mapper, calls) = mapper_with_mock();
    font_mapper.set_skip_font_enumeration(true);
    font_mapper.load_installed_fonts();
    assert!(calls.borrow().is_empty());
}
#[test]
fn LoadInstalledFontsCalledOnlyOnce() {
    let (mut font_mapper, calls) = mapper_with_mock();
    calls.borrow_mut().push_back(Call::EnumFontList);
    font_mapper.load_installed_fonts();
    font_mapper.load_installed_fonts();
    font_mapper.load_installed_fonts();
    assert!(calls.borrow().is_empty());
}
