// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Close translation of `core/fxge/skrifa/fx_skrifa_unittest.cpp`.
//! The Rust face consolidates the native Fontations FFI adapter. Direct
//! TableProvider/view operations retain tests for adapter-only scalar queries.
#![allow(non_snake_case)]
use super::*;
use crate::fxge::cfx_standardfont::{get_font_data, get_generic_sans_font_data, StandardFontIndex};
use read_fonts::ps::{cff::CffFontRef, PredefinedEncoding};

fn font_bytes(filename: &str) -> Vec<u8> {
    let font_path = std::path::Path::new(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../upstream/pdfium/testing/resources"
    ))
    .join(filename);
    std::fs::read(font_path).unwrap()
}

// The native run() fixture lives in skrifa/src/outlines.cpp. Keep its complete
// assertions and both outline draws; bytes outlive the borrowed face/font.
#[derive(Clone, Copy)]
enum Verb {
    MoveTo,
    LineTo,
    QuadTo,
    CurveTo,
    Close,
}
#[derive(Default)]
struct Outline {
    verbs: Vec<Verb>,
    points: Vec<[f32; 2]>,
    advance_width: f32,
}
impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.verbs.push(Verb::MoveTo);
        self.points.push([x, y]);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.verbs.push(Verb::LineTo);
        self.points.push([x, y]);
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.verbs.push(Verb::QuadTo);
        self.points.extend_from_slice(&[[cx, cy], [x, y]]);
    }
    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.verbs.push(Verb::CurveTo);
        self.points
            .extend_from_slice(&[[cx0, cy0], [cx1, cy1], [x, y]]);
    }
    fn close(&mut self) {
        self.verbs.push(Verb::Close);
    }
}
fn dump_outline(outline: &Outline) {
    let mut point_idx = 0;
    for verb in &outline.verbs {
        match verb {
            Verb::MoveTo | Verb::LineTo => {
                let p = outline.points[point_idx];
                point_idx += 1;
                print!(
                    "{}{:.6},{:.6} ",
                    if matches!(verb, Verb::MoveTo) {
                        "M"
                    } else {
                        "L"
                    },
                    p[0],
                    p[1]
                );
            }
            Verb::QuadTo => {
                let c0 = outline.points[point_idx];
                let p = outline.points[point_idx + 1];
                point_idx += 2;
                print!("Q{:.6},{:.6} {:.6},{:.6} ", c0[0], c0[1], p[0], p[1]);
            }
            Verb::CurveTo => {
                let c0 = outline.points[point_idx];
                let c1 = outline.points[point_idx + 1];
                let p = outline.points[point_idx + 2];
                point_idx += 3;
                print!(
                    "C{:.6},{:.6} {:.6},{:.6} {:.6},{:.6} ",
                    c0[0], c0[1], c1[0], c1[1], p[0], p[1]
                );
            }
            Verb::Close => print!("Z"),
        }
    }
    println!("\nadvance = {:.6}", outline.advance_width);
}
fn run(bytes: &[u8]) {
    let Ok(font) = CfxFace::new(bytes, 0) else {
        println!("Failed to load font!");
        return;
    };
    let name = font.postscript_name();
    let family_name = font.family_name();
    println!("ps name = {name}, family name = {family_name}");
    println!("Font has {} glyphs", font.glyph_count());
    let cff = CffFontRef::new(bytes, 0, None).unwrap();
    assert!(matches!(
        cff.encoding().unwrap().predefined(),
        Some(PredefinedEncoding::Standard)
    ));
    let gid_from_code = font.raw().unwrap().encoded_char_index(120);
    let gid = font.unicode_char_index(u32::from(b'x'));
    assert_eq!(gid_from_code, gid);
    let mut outline = Outline::default();
    if let Some(width) = font.draw_unscaled(gid, &mut outline) {
        outline.advance_width = width.unwrap_or_default();
    }
    dump_outline(&outline);
    outline = Outline::default();
    // Same Fontations scaled CFF draw used by native scaled_outline().
    let gid = GlyphId::new(gid);
    if let Some(subfont_index) = cff.subfont_index(gid) {
        if let Ok(subfont) = cff.subfont(subfont_index, &[]) {
            if let Ok(width) = cff.draw(&subfont, gid, &[], Some(16.0), &mut outline) {
                outline.advance_width = width.unwrap_or_default();
            }
        }
    }
    dump_outline(&outline);
    let period_unicode = read_fonts::ps::agl::name_to_char("period");
    assert!(period_unicode.is_some());
    assert_eq!(period_unicode.unwrap(), '.');
    let mut period_name = [0u8; 64];
    let period_slice = read_fonts::ps::agl::char_to_name(u32::from(b'.'), &mut period_name[..40])
        .unwrap_or_default();
    assert_eq!(period_slice, "period");
}

#[test]
fn TestFoxitFixedCff() {
    run(&font_bytes("fonts/foxit_fixed.cff"));
}

#[test]
fn TestGetOs2CodePageRange() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let range = font.os2_code_page_range();
    assert!(range.is_some());
    let range = range.unwrap();
    assert_eq!(range[0], 0u32);
    assert_eq!(range[1], 0u32);
}
#[test]
fn TestGetOs2Panose() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let panose = font.os2_panose();
    assert!(panose.is_some());
    let panose = panose.unwrap();
    assert_eq!(panose[0], 2);
    assert_eq!(panose[1], 0);
}
#[test]
fn TestGetOs2FsType() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let mut fs_type = 0x1234u16; // Show that it is updated.
    let success = if let Ok(os2) = font.sfnt().os2() {
        fs_type = os2.fs_type();
        true
    } else {
        false
    };
    assert!(success);
    assert_eq!(fs_type, 0u16);
}
#[test]
fn TestGetCharCodesAndIndices() {
    let bytes = font_bytes("fonts/ahem/Ahem.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let results = font.get_char_codes_and_indices(0xffff);
    assert_eq!(278usize, results.len());
    assert_eq!(
        CharCodeAndIndex {
            char_code: 32,
            glyph_index: 3
        },
        results[0]
    );
    assert_eq!(
        CharCodeAndIndex {
            char_code: 33,
            glyph_index: 4
        },
        results[1]
    );
    assert_eq!(
        CharCodeAndIndex {
            char_code: 65279,
            glyph_index: 245
        },
        results[277]
    );
}
#[test]
fn TestIsTricky() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    assert!(!font.is_tricky());
}
#[test]
fn TestGetNumFaces() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let count = if let Ok(collection) = read_fonts::CollectionRef::new(&bytes) {
        collection.len()
    } else if read_fonts::FontRef::new(&bytes).is_ok() {
        1
    } else {
        0
    };
    assert_eq!(count, 1u32);
}
#[test]
fn TestHasOutline() {
    let bytes = font_bytes("fonts/bug_2094.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    assert!(font.has_outline(0));
}
#[test]
fn TestRobotoGlyph167Bounds() {
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/harfbuzz/Roboto-Regular.ttf"
    ))
    .unwrap();
    assert!(!bytes.is_empty());
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    assert_eq!(font.units_per_em(), 2048);
    let bbox = font.unscaled_glyph_bounds(167);
    assert_eq!(bbox.0, 85.0f32);
    assert_eq!(bbox.1, -21.0f32);
    assert_eq!(bbox.2, 1135.0f32);
    assert_eq!(bbox.3, 1477.0f32);
    assert!(font.sfnt_opt().is_some());
    assert!(!font.is_italic());
    assert!(!font.is_bold());
    let font_bbox = font
        .sfnt()
        .metrics(Size::unscaled(), LocationRef::default())
        .bounds;
    assert!(font_bbox.is_some());
    let font_bbox = font_bbox.unwrap();
    assert_eq!(font_bbox.x_min, -1825.0f32);
    assert_eq!(font_bbox.y_min, -555.0f32);
    assert_eq!(font_bbox.x_max, 4188.0f32);
    assert_eq!(font_bbox.y_max, 2163.0f32);
}
#[test]
fn TestMinionCff() {
    let bytes = font_bytes("fonts/minion.cff");
    assert!(!bytes.is_empty());
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    assert_eq!(font.glyph_count(), 1250u32);
    let family_name = font.family_name();
    let ps_name = font.postscript_name();
    let mut sk_family = family_name.into_bytes();
    crate::fxge::fx_font::maybe_remove_subsetted_font_prefix(&mut sk_family);
    assert_eq!(sk_family, b"MinionPro-Regular");
    let mut sk_ps = ps_name.into_bytes();
    crate::fxge::fx_font::maybe_remove_subsetted_font_prefix(&mut sk_ps);
    assert_eq!(sk_ps, b"MinionPro-Regular");
    assert!(font.has_outline(55));
}
#[test]
fn TestTimesBoldGlyph104Bounds() {
    let bytes = font_bytes("fonts/times_bold.ttf");
    assert!(!bytes.is_empty());
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let bbox = font.unscaled_glyph_bounds(104);
    assert_eq!(bbox.0, 48.0f32);
    assert_eq!(bbox.1, -32.0f32);
    assert_eq!(bbox.2, 1444.0f32);
    assert_eq!(bbox.3, 1756.0f32);
    assert!(font.sfnt_opt().is_some());
    assert!(!font.is_italic());
    assert!(font.is_bold());
    let font_bbox = font
        .sfnt()
        .metrics(Size::unscaled(), LocationRef::default())
        .bounds;
    assert!(font_bbox.is_some());
    let font_bbox = font_bbox.unwrap();
    assert_eq!(font_bbox.x_min, -1143.0f32);
    assert_eq!(font_bbox.y_min, -628.0f32);
    assert_eq!(font_bbox.x_max, 4096.0f32);
    assert_eq!(font_bbox.y_max, 2101.0f32);
}
#[test]
fn TestRobotoGlyph2344Bounds() {
    let bytes = font_bytes("fonts/roboto.ttf");
    assert!(!bytes.is_empty());
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let bbox = font.unscaled_glyph_bounds(2344);
    assert_eq!(bbox.0, -74.0f32);
    assert_eq!(bbox.1, 0.0f32);
    assert_eq!(bbox.2, 634.0f32);
    assert_eq!(bbox.3, 1838.0f32);
}
#[test]
fn TestType1Face() {
    assert_eq!(CfxFace::unicode_from_adobe_name("period"), u32::from(b'.'));
    assert_eq!(CfxFace::unicode_from_adobe_name("A"), u32::from(b'A'));
    assert_eq!(CfxFace::adobe_name_from_unicode(u32::from(b'.')), b"period");
    assert_eq!(CfxFace::adobe_name_from_unicode(u32::from(b'A')), b"A");
    // Fontations is selected; the native non-Fontations early return is absent.
    let face = CfxFace::new(get_generic_sans_font_data(), 0);
    assert!(face.is_ok());
    let face = face.unwrap();
    assert_eq!(face.font_format(), "Type 1");
    assert_eq!(face.family_name(), "Chrome Sans MM");
    assert_eq!(face.postscript_name(), "ChromeSansMM");
    assert_eq!(face.glyph_count(), 230);
    assert_eq!(face.units_per_em(), 1000);
    assert!(!face.is_tt_ot());
    assert!(!face.is_italic());
    assert!(!face.is_bold());
    assert_ne!(face.bbox(), FxRect::new(-1000, -1000, 1000, 1000));
    assert!(face.select_charmap(FontEncoding::AdobeCustom));
    let gid_x = face.get_char_index(120);
    assert_eq!(gid_x, 103);
    assert!(face.select_charmap(FontEncoding::Unicode));
    assert_eq!(face.get_char_index(u32::from(b'x')), gid_x);
    let path = face.load_glyph_path(gid_x, 0, false, None);
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path.points().len(), 12usize);
    // Native bitmap-size assertions are PDF_ENABLE_FREETYPE only. The selected
    // AGG rasterizer retains its separate glyph coverage/placement regressions.
}
#[test]
fn TestBareCffFace() {
    // Fontations is selected; the native non-Fontations skip is absent.
    let face = CfxFace::new(get_font_data(StandardFontIndex::Helvetica), 0);
    assert!(face.is_ok());
    let face = face.unwrap();
    assert_eq!(face.font_format(), "CFF");
    assert!(!face.is_tt_ot());
    assert_eq!(face.charmap_count(), 2usize);
    let info = face.charmap_id_pair_by_index(0);
    assert_eq!(info.platform_id, PLATFORM_APPLE_UNICODE);
    assert_eq!(info.encoding_id, APPLE_UNICODE_ENCODING_UNICODE_2_0);
    assert_eq!(
        charmap_id_pair_to_font_encoding(info),
        FontEncoding::Unicode
    );
    let info = face.charmap_id_pair_by_index(1);
    assert_eq!(info.platform_id, PLATFORM_ADOBE);
    assert_eq!(info.encoding_id, ADOBE_ENCODING_CUSTOM);
    assert_eq!(
        charmap_id_pair_to_font_encoding(info),
        FontEncoding::AdobeCustom
    );
    assert!(face.select_charmap(FontEncoding::AdobeCustom));
    let gid_x = face.get_char_index(120);
    assert_eq!(gid_x, 222);
    assert!(face.select_charmap(FontEncoding::Unicode));
    assert_eq!(face.get_char_index(u32::from(b'x')), gid_x);
    let path = face.load_glyph_path(gid_x, 0, false, None);
    assert!(path.is_some());
    let path = path.unwrap();
    assert_eq!(path.points().len(), 12usize);
}
#[test]
fn TestGetSfntTable() {
    let bytes = font_bytes("fonts/ahem/Ahem.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let head_tag = 0x68656164;
    let length = font.get_sfnt_table(head_tag, &mut []);
    assert_eq!(54usize, length);
    let mut head_data = vec![0u8; length];
    let read_bytes = font.get_sfnt_table(head_tag, &mut head_data);
    assert_eq!(54usize, read_bytes);
    let missing_tag = 0x78797a77;
    assert_eq!(0usize, font.get_sfnt_table(missing_tag, &mut []));
}
#[test]
fn TestCharmaps() {
    let bytes = font_bytes("fonts/ahem/Ahem.ttf");
    let font = CfxFace::new(&bytes, 0);
    assert!(font.is_ok());
    let font = font.unwrap();
    let count = font.charmap_count();
    assert_eq!(count, 2usize);
    let mut found_unicode_times = 0;
    let cmap = font.sfnt().cmap().unwrap();
    for i in 0..count {
        let info = cmap.encoding_records().get(i);
        assert!(info.is_some());
        let info = info.unwrap();
        if info.platform_id() as u16 == PLATFORM_WINDOWS
            && info.encoding_id() == WINDOWS_ENCODING_UNICODE
        {
            found_unicode_times += 1;
            assert_eq!(3u32, font.cmap_char_to_gid(i, 32));
            assert_eq!(35u32, font.cmap_char_to_gid(i, 65));
        }
    }
    assert_eq!(found_unicode_times, 1usize);
    assert!(cmap.encoding_records().get(count).is_none());
    assert_eq!(0u32, font.cmap_char_to_gid(count, 32));
}
