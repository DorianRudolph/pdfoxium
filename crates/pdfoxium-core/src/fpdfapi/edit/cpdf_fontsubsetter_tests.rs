// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Complete engine adaptations of the NoNewText/StandardFont embedder cases,
//! plus focused retained-GID, CFF, and override-lifetime regressions. The five
//! other upstream cases require the absent third_party/test_fonts fixtures and
//! the not-yet-ported FPDFText_LoadFont factory; these tests do not replace them.

#![allow(non_snake_case)]

use super::*;
use crate::fpdfapi::edit::cpdf_font_util::load_unicode;
use crate::fpdfapi::edit::cpdf_pagecontentgenerator::CpdfPageContentGenerator;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_pageobject::{CpdfPageObject, PageObject};
use crate::fpdfapi::page::cpdf_pathobject::CpdfPathObject;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use crate::fxge::cfx_face::CfxFace;
use read_fonts::{FontRef, TableProvider};

const ROBOTO: &[u8] = include_bytes!("../../../testdata/harfbuzz/Roboto-Regular.ttf");
const NOTO_CFF: &[u8] = include_bytes!(
    "../../../../../upstream/pdfium/third_party/NotoSansCJK/NotoSansSC-Regular.subset.otf"
);

fn new_document_and_page() -> (Rc<RefCell<CpdfDocument>>, CpdfPage) {
    let mut doc = CpdfDocument::new();
    doc.create_new_doc();
    let page = doc.create_new_page(0).unwrap();
    dict_mut(&page, |d| {
        let mut media_box = CpdfArray::new();
        for coordinate in [0, 0, 400, 400] {
            media_box.append_new(CpdfScalarObject::number_from_signed(coordinate));
        }
        d.set_new_for(b"MediaBox", media_box);
    });
    let page = CpdfPage::from_document(&mut doc, 0).unwrap();
    (Rc::new(RefCell::new(doc)), page)
}

#[test]
fn NoNewText() {
    let (doc, mut page) = new_document_and_page();
    let mut subsetter = CpdfFontSubsetter::new(Rc::clone(&doc));
    assert!(subsetter.generate_object_overrides(&[]).is_empty());
    let new_obj_nums: Vec<u32> = (1..=100).collect();
    assert!(subsetter
        .generate_object_overrides(&new_obj_nums)
        .is_empty());
    // The native case inserts a rectangle, without generating its content.
    let mut rectangle = CpdfPathObject::default();
    rectangle
        .mutable_path()
        .append_rect(20.0, 100.0, 70.0, 150.0);
    page.mutable_holder()
        .append_page_object(Box::new(rectangle));
    assert!(subsetter
        .generate_object_overrides(&new_obj_nums)
        .is_empty());
}

#[test]
fn StandardFont() {
    let (doc, mut page) = new_document_and_page();
    let font = CpdfFont::get_stock_font(&doc.borrow(), b"Helvetica").unwrap();
    let mut text = CpdfTextObject::new(CpdfPageObject::NO_CONTENT_STREAM);
    let font_dict = font.font_dict();
    text.base_mut()
        .mutable_text_state()
        .set_font(Some(font), font_dict);
    text.base_mut().mutable_text_state().set_font_size(20.0);
    text.set_text(b"Hello world");
    text.transform(CfxMatrix::new(1.0, 0.0, 0.0, 1.0, 50.0, 200.0));
    page.mutable_holder().append_page_object(Box::new(text));
    CpdfPageContentGenerator::new(page.mutable_holder(), Some(&mut doc.borrow_mut()))
        .generate_content();
    let new_obj_nums: Vec<u32> = (1..=100).collect();
    assert!(CpdfFontSubsetter::new(Rc::clone(&doc))
        .generate_object_overrides(&new_obj_nums)
        .is_empty());
}

#[test]
fn backend_retains_original_glyph_ids_and_notdef_outline() {
    let original = CfxFace::new(ROBOTO, 0).unwrap();
    let gid = original.unicode_char_index(u32::from('z'));
    let unused_gid = original.unicode_char_index(u32::from('B'));
    assert_ne!(gid, 0);
    assert!(unused_gid < gid);
    assert!(original.font_glyph_bbox(0).is_some());
    let subset_data = generate_font_subset(ROBOTO, &BTreeSet::from([gid]));
    assert!(!subset_data.is_empty());
    assert!(subset_data.len() < ROBOTO.len());
    let subset = CfxFace::new(&subset_data, 0).unwrap();
    assert_eq!(subset.unicode_char_index(u32::from('z')), gid);
    assert_eq!(subset.font_glyph_bbox(gid), original.font_glyph_bbox(gid));
    assert_eq!(subset.font_glyph_bbox(0), original.font_glyph_bbox(0));
    // The face facade accepts empty glyph slots and returns zero bounds for
    // them. Inspect loca/glyf to verify the unused outline was actually removed.
    let sfnt = FontRef::new(&subset_data).unwrap();
    assert!(sfnt
        .loca(None)
        .unwrap()
        .get_glyf(
            read_fonts::types::GlyphId::new(unused_gid),
            &sfnt.glyf().unwrap()
        )
        .unwrap()
        .is_none());
    assert!(
        u32::from(
            FontRef::new(&subset_data)
                .unwrap()
                .maxp()
                .unwrap()
                .num_glyphs()
        ) > gid
    );
}

#[test]
fn backend_preserves_original_cff_when_skera_drops_its_outlines() {
    assert!(generate_font_subset(&[], &BTreeSet::from([1])).is_empty());
    assert!(generate_font_subset(b"not a font", &BTreeSet::from([1])).is_empty());
    assert!(is_open_type_cff(NOTO_CFF));
    let original = CfxFace::new(NOTO_CFF, 0).unwrap();
    let gid = original.unicode_char_index(u32::from('这'));
    assert_ne!(gid, 0);
    // Skera 0.9 returns success after dropping CFF. The adapter must detect
    // that invalid result, rather than creating a font with no outlines.
    assert!(matches!(
        backend::subset_font_data(NOTO_CFF, &BTreeSet::from([gid])),
        Err(backend::SubsetError::InvalidOutput)
    ));
    assert!(generate_font_subset(NOTO_CFF, &BTreeSet::from([gid])).is_empty());
}

#[test]
fn generated_subset_names_replace_only_valid_existing_prefixes() {
    for name in [b"FixtureFont".as_slice(), b"AAAAAA+FixtureFont"] {
        let subset = generate_font_subset_name(name.to_vec());
        assert_eq!(subset.len(), 7 + b"FixtureFont".len());
        assert!(subset[..6].iter().all(u8::is_ascii_uppercase));
        assert_eq!(&subset[6..], b"+FixtureFont");
    }
    let subset = generate_font_subset_name(b"Abcdef+FixtureFont".to_vec());
    assert_eq!(&subset[6..], b"+Abcdef+FixtureFont");
}

struct EmbeddedFixture {
    root_font: ObjectRef,
    cid_font: ObjectRef,
    descriptor: ObjectRef,
    font_stream: ObjectRef,
    widths: ObjectRef,
    to_unicode: ObjectRef,
    gid: u32,
}

// Construct a real embedded CID resource with explicit metadata, then parse its
// text through the production page parser. This exercises the subsetter without
// pretending to port FPDFText_LoadFont's full font-import factory.
fn add_embedded_fixture(
    doc: &mut CpdfDocument,
    font_data: &[u8],
    unicode: char,
) -> EmbeddedFixture {
    add_embedded_fixture_with_code(doc, font_data, unicode, None)
}

fn add_embedded_fixture_with_code(
    doc: &mut CpdfDocument,
    font_data: &[u8],
    unicode: char,
    code: Option<u32>,
) -> EmbeddedFixture {
    let face = CfxFace::new(font_data, 0).unwrap();
    let gid = face.unicode_char_index(u32::from(unicode));
    assert!(gid != 0 && gid <= 0xffff);
    let code = code.unwrap_or(gid);
    let holder = doc.holder();
    let font_stream = retain_object(CpdfStream::from_bytes(font_data));
    holder
        .borrow_mut()
        .add_indirect_object(Rc::clone(&font_stream));
    let descriptor = holder.borrow_mut().new_indirect_dictionary();
    dict_mut(&descriptor, |d| {
        d.set_new_name_for(b"Type", b"FontDescriptor");
        d.set_new_name_for(b"FontName", b"FixtureFont");
        d.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(0x20));
        d.set_new_for(
            b"FontFile2",
            CpdfReference::new(&holder, font_stream.borrow().obj_num()),
        );
    });
    let widths = retain_object(create_widths_array(&BTreeMap::from([(
        code,
        face.glyph_tt_width(gid) as u32,
    )])));
    holder.borrow_mut().add_indirect_object(Rc::clone(&widths));
    let cid_font = holder.borrow_mut().new_indirect_dictionary();
    dict_mut(&cid_font, |d| {
        d.set_new_name_for(b"Type", b"Font");
        d.set_new_name_for(b"Subtype", b"CIDFontType2");
        d.set_new_name_for(b"BaseFont", b"FixtureFont");
        let mut info = CpdfDictionary::new();
        info.set_new_name_for(b"Registry", b"Adobe");
        info.set_new_name_for(b"Ordering", b"Identity");
        info.set_new_for(b"Supplement", CpdfScalarObject::number_from_signed(0));
        d.set_new_for(b"CIDSystemInfo", info);
        d.set_new_for(
            b"FontDescriptor",
            CpdfReference::new(&holder, descriptor.borrow().obj_num()),
        );
        d.set_new_for(b"W", CpdfReference::new(&holder, widths.borrow().obj_num()));
        if code == gid {
            d.set_new_name_for(b"CIDToGIDMap", b"Identity");
        } else {
            let mut mapping = vec![0; (code as usize + 1) * 2];
            mapping[code as usize * 2..].copy_from_slice(&(gid as u16).to_be_bytes());
            let map = holder.borrow_mut().new_indirect_stream_from_bytes(&mapping);
            d.set_new_for(
                b"CIDToGIDMap",
                CpdfReference::new(&holder, map.borrow().obj_num()),
            );
        }
    });
    let to_unicode = retain_object(load_unicode(&BTreeMap::from([(
        code,
        vec![u32::from(unicode)],
    )])));
    holder
        .borrow_mut()
        .add_indirect_object(Rc::clone(&to_unicode));
    let root_font = holder.borrow_mut().new_indirect_dictionary();
    dict_mut(&root_font, |d| {
        d.set_new_name_for(b"Type", b"Font");
        d.set_new_name_for(b"Subtype", b"Type0");
        d.set_new_name_for(b"BaseFont", b"FixtureFont");
        d.set_new_name_for(b"Encoding", b"Identity-H");
        let mut descendants = CpdfArray::new();
        descendants.append_new(CpdfReference::new(&holder, cid_font.borrow().obj_num()));
        d.set_new_for(b"DescendantFonts", descendants);
        d.set_new_for(
            b"ToUnicode",
            CpdfReference::new(&holder, to_unicode.borrow().obj_num()),
        );
    });
    let page = doc.page_dictionary(0).unwrap();
    dict_mut(&page, |d| {
        let mut resources = CpdfDictionary::new();
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_for(
            b"F",
            CpdfReference::new(&holder, root_font.borrow().obj_num()),
        );
        resources.set_new_for(b"Font", fonts);
        d.set_new_for(b"Resources", resources);
        let content = retain_object(CpdfStream::from_bytes(
            format!("BT /F 20 Tf 1 0 0 1 50 200 Tm <{code:04X}{code:04X}> Tj ET").as_bytes(),
        ));
        let content_num = holder.borrow_mut().add_indirect_object(content);
        d.set_new_for(b"Contents", CpdfReference::new(&holder, content_num));
    });
    EmbeddedFixture {
        root_font,
        cid_font,
        descriptor,
        font_stream,
        widths,
        to_unicode,
        gid,
    }
}

#[test]
fn embedded_font_overrides_preserve_originals_and_cff_failure_makes_no_overrides() {
    for (data, unicode) in [(ROBOTO, 'z'), (NOTO_CFF, '这')] {
        let (doc, _page) = new_document_and_page();
        let fixture = add_embedded_fixture(&mut doc.borrow_mut(), data, unicode);
        let root_num = fixture.root_font.borrow().obj_num();
        let cid_num = fixture.cid_font.borrow().obj_num();
        let descriptor_num = fixture.descriptor.borrow().obj_num();
        let file_num = fixture.font_stream.borrow().obj_num();
        let widths_num = fixture.widths.borrow().obj_num();
        let unicode_num = fixture.to_unicode.borrow().obj_num();
        let numbers: Vec<u32> = (1..=100).collect();
        let mut subsetter = CpdfFontSubsetter::new(Rc::clone(&doc));
        let overrides = subsetter.generate_object_overrides(&numbers);
        if is_open_type_cff(data) {
            assert!(overrides.is_empty());
            assert_eq!(
                dict(&fixture.root_font, |d| d.get_name_for(b"BaseFont")),
                b"FixtureFont"
            );
            assert_eq!(
                dict(&fixture.cid_font, |d| d.get_name_for(b"Subtype")),
                b"CIDFontType2"
            );
            assert!(dict(&fixture.descriptor, |d| d.key_exist(b"FontFile2")));
            assert_eq!(
                fixture
                    .font_stream
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .get_in_memory_raw_data(),
                data
            );
            continue;
        }
        assert_eq!(
            overrides.keys().copied().collect::<BTreeSet<_>>(),
            BTreeSet::from([
                root_num,
                cid_num,
                descriptor_num,
                file_num,
                widths_num,
                unicode_num
            ])
        );
        assert_eq!(
            subsetter.candidates[&file_num].used_gids,
            BTreeSet::from([fixture.gid])
        );
        assert_eq!(
            subsetter.candidates[&file_num].char_code_to_unicode[&fixture.gid],
            vec![u32::from(unicode)]
        );
        assert!(overrides.values().all(|o| o.borrow().is_inline()));
        assert!(subsetter.generate_object_overrides(&[100]).is_empty());

        let cff = is_open_type_cff(data);
        let override_stream = overrides[&file_num].borrow();
        let override_stream = override_stream
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap();
        assert!(!override_stream.get_in_memory_raw_data().is_empty());
        assert_eq!(
            is_open_type_cff(override_stream.get_in_memory_raw_data()),
            cff
        );
        let subset_name = dict(&overrides[&root_num], |d| d.get_name_for(b"BaseFont"));
        assert_eq!(&subset_name[6..], b"+FixtureFont");
        assert!(subset_name[..6].iter().all(u8::is_ascii_uppercase));
        assert_eq!(
            dict(&overrides[&cid_num], |d| d.get_name_for(b"BaseFont")),
            subset_name
        );
        assert_eq!(
            dict(&overrides[&descriptor_num], |d| d.get_name_for(b"FontName")),
            subset_name
        );
        dict(&override_stream.get_dict(), |d| {
            if cff {
                assert_eq!(d.get_name_for(b"Subtype"), b"OpenType");
                assert!(!d.key_exist(b"Length1"));
            } else {
                assert!(!d.key_exist(b"Subtype"));
                assert_eq!(
                    d.get_integer_for(b"Length1", 0),
                    override_stream.get_raw_size() as i32
                );
            }
        });
        assert_eq!(
            dict(&fixture.root_font, |d| d.get_name_for(b"BaseFont")),
            b"FixtureFont"
        );
        assert_eq!(
            dict(&fixture.cid_font, |d| d.get_name_for(b"Subtype")),
            b"CIDFontType2"
        );
        assert_eq!(
            dict(&fixture.descriptor, |d| d.get_integer_for(b"Flags", 0)),
            0x20
        );
        assert!(dict(&fixture.descriptor, |d| d.key_exist(b"FontFile2")));
        let original = fixture.font_stream.borrow();
        assert_eq!(
            original
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_in_memory_raw_data(),
            data
        );
    }
}

#[test]
fn subset_save_reload_preserves_text_mappings_and_rendering_including_cff_fallback() {
    use crate::fpdfapi::edit::cpdf_creator::CpdfCreator;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_parser::ParserError;
    use crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions;
    use crate::fpdfapi::render::cpdf_renderstatus::render_page_objects;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
    use crate::fxcrt::fx_coordinates::FxRect;
    use crate::fxcrt::fx_stream::WriteStream;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;

    #[derive(Default)]
    struct Sink(Vec<u8>);
    impl WriteStream for Sink {
        fn write_block(&mut self, bytes: &[u8]) -> bool {
            self.0.extend_from_slice(bytes);
            true
        }
    }
    fn render(doc: &Rc<RefCell<CpdfDocument>>) -> (Vec<u8>, Vec<u32>) {
        let mut page = CpdfPage::from_document(&mut doc.borrow_mut(), 0).unwrap();
        page.parse_path_content();
        let mut codes = Vec::new();
        for object in page.holder().iter() {
            if let Some(text) = object.as_any().downcast_ref::<CpdfTextObject>() {
                codes.extend_from_slice(text.char_codes());
            }
        }
        let mut bitmap = CfxDibitmap::create(400, 400, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0xffffffff);
        let matrix = page.display_matrix_for_rect(FxRect::new(0, 0, 400, 400), 0);
        assert!(render_page_objects(
            &page,
            &mut bitmap,
            matrix,
            &CpdfRenderOptions::new(),
            false
        ));
        let pixels: Vec<u8> = (0..400)
            .flat_map(|row| bitmap.scanline(row).unwrap().to_vec())
            .collect();
        assert!(
            pixels.iter().any(|&pixel| pixel != 255),
            "fixture must paint visible glyphs"
        );
        (pixels, codes)
    }
    for (data, unicode, code) in [
        (ROBOTO, 'z', None),
        (ROBOTO, 'z', Some(17)),
        (NOTO_CFF, '这', None),
    ] {
        for flags in [
            CpdfCreator::SUBSET_NEW_FONTS,
            CpdfCreator::SUBSET_NEW_FONTS | CpdfCreator::INCREMENTAL,
        ] {
            let (doc, _page) = new_document_and_page();
            let fixture =
                add_embedded_fixture_with_code(&mut doc.borrow_mut(), data, unicode, code);
            let expected = render(&doc);
            let file_num = fixture.font_stream.borrow().obj_num();
            let cid_num = fixture.cid_font.borrow().obj_num();
            let mut sink = Sink::default();
            {
                let mut creator = CpdfCreator::new(Rc::clone(&doc), &mut sink);
                assert!(creator.create(flags, 0));
            }
            let mut reloaded = CpdfDocument::new();
            assert_eq!(
                reloaded.load_doc(Rc::new(CfxReadOnlyContainerStream::new(sink.0)), b""),
                ParserError::Success
            );
            assert!(!reloaded.parser().xref_table_rebuilt());
            let holder = reloaded.holder();
            let font =
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, file_num).unwrap();
            let font = CpdfRetainedStreamAcc::load_filtered(font).unwrap();
            if is_open_type_cff(data) {
                // Fallback preserves the complete original font, not just a
                // readable replacement or a stream with the same byte length.
                assert_eq!(&*font.get_span(), data);
            } else {
                assert!(font.get_span().len() < data.len());
            }
            let cid =
                CpdfIndirectObjectHolder::get_or_parse_indirect_object(&holder, cid_num).unwrap();
            if let Some(code) = code {
                let mapping = dict(&cid, |d| d.get_stream_for(b"CIDToGIDMap")).unwrap();
                let mapping = CpdfRetainedStreamAcc::load_filtered(mapping).unwrap();
                let bytes = mapping.get_span();
                assert_eq!(bytes.len(), (code as usize + 1) * 2);
                assert!(bytes[..code as usize * 2].iter().all(|&byte| byte == 0));
                assert_eq!(
                    &bytes[code as usize * 2..],
                    &(fixture.gid as u16).to_be_bytes()
                );
            } else {
                assert_eq!(dict(&cid, |d| d.get_name_for(b"CIDToGIDMap")), b"Identity");
            }
            assert_eq!(dict(&cid, |d| d.get_name_for(b"Subtype")), b"CIDFontType2");
            assert_eq!(render(&Rc::new(RefCell::new(reloaded))), expected);
            assert_eq!(
                fixture
                    .font_stream
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .get_in_memory_raw_data(),
                data
            );
        }
    }
}

#[test]
fn validation_accepts_valid_cff_without_a_format_blacklist_and_rejects_missing_outlines() {
    use read_fonts::types::Tag;
    use read_fonts::{FontRef, TableProvider};
    for data in [ROBOTO, NOTO_CFF] {
        let original = FontRef::new(data).unwrap();
        assert!(backend::validate_subset(&original, data, &BTreeSet::from([0])).is_some());
        // A structurally readable SFNT is insufficient. Remove only the table
        // directory's outline tag, reproducing the unsupported-outline hazard.
        let tag = if is_open_type_cff(data) {
            b"CFF "
        } else {
            b"glyf"
        };
        let mut missing = data.to_vec();
        let count = u16::from_be_bytes([data[4], data[5]]) as usize;
        let offset = (0..count)
            .map(|i| 12 + 16 * i)
            .find(|&offset| &data[offset..offset + 4] == tag)
            .unwrap();
        missing[offset..offset + 4].copy_from_slice(b"gone");
        let readable = FontRef::new(&missing).unwrap();
        assert!(readable.maxp().is_ok());
        assert!(readable.data_for_tag(Tag::new(tag)).is_none());
        assert!(backend::validate_subset(&original, &missing, &BTreeSet::from([0])).is_none());
    }
}
