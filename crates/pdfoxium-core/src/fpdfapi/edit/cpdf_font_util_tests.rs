// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Focused serialization regressions for `cpdf_font_util.cpp`. The pinned
//! upstream has no dedicated unit suite for this file. The duplicate mapping
//! fixture below comes from `FPDFEditEmbedderTest.LoadCidType2FontCustom`.

use super::*;
use crate::fpdfapi::font::cpdf_tounicodemap::CpdfToUnicodeMap;
use crate::fpdfapi::parser::cpdf_object::PdfObject;
use crate::fxcrt::utf16::{is_supplementary, SurrogatePair};

#[test]
fn retained_unicode_sequences_keep_surrogates_ligatures_and_scalar_destinations() {
    let mappings = BTreeMap::from([
        (1, vec![0x41]),
        (7, vec![0xd83d, 0xde00]),
        (9, vec![u32::from('f'), u32::from('f'), u32::from('i')]),
        (11, vec![0x1f642]),
    ]);
    let stream = load_unicode_sequences(&mappings);
    let parsed = CpdfToUnicodeMap::from_stream(&stream);
    assert_eq!(parsed.lookup(1), mappings[&1]);
    assert_eq!(parsed.lookup(7), mappings[&7]);
    assert_eq!(parsed.lookup(9), mappings[&9]);
    assert_eq!(parsed.lookup(11), vec![0xd83d, 0xde42]);
}

#[derive(Debug, PartialEq)]
enum WidthEntry {
    Number(i32),
    Array(Vec<i32>),
}

fn width_entries(widths: &CpdfArray) -> Vec<WidthEntry> {
    (0..widths.size())
        .map(|index| {
            let object = widths.get_object_at(index).unwrap();
            let object = object.borrow();
            match object.as_any().downcast_ref::<CpdfArray>() {
                Some(array) => WidthEntry::Array(
                    (0..array.size())
                        .map(|index| array.get_integer_at(index))
                        .collect(),
                ),
                None => WidthEntry::Number(object.get_integer()),
            }
        })
        .collect()
}

fn unicode_body(map: &BTreeMap<u32, Vec<u32>>) -> String {
    let stream = load_unicode(map);
    assert_eq!(stream.obj_num(), 0);
    let bytes = stream.get_in_memory_raw_data();
    let dictionary = stream.get_dict();
    assert_eq!(
        dictionary
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_integer_for(b"Length", 0),
        i32::try_from(bytes.len()).unwrap()
    );
    let text = std::str::from_utf8(bytes).unwrap();
    text.strip_prefix(TO_UNICODE_START)
        .unwrap()
        .strip_suffix(TO_UNICODE_END)
        .unwrap()
        .to_owned()
}

#[test]
fn widths_group_equal_prefixes_and_keep_later_equal_widths_in_variable_ranges() {
    use WidthEntry::{Array, Number};
    assert!(create_widths_array(&BTreeMap::new()).is_empty());
    let widths = BTreeMap::from([
        (1, 7),
        (2, 7),
        (3, 8),
        (4, 8),
        (8, 100),
        (9, 200),
        (10, 200),
        (u32::MAX, u32::MAX),
    ]);
    let widths_array = create_widths_array(&widths);
    assert_eq!(widths_array.obj_num(), 0);
    assert_eq!(
        width_entries(&widths_array),
        vec![
            Number(1),
            Number(2),
            Number(7),
            Number(3),
            Number(4),
            Number(8),
            Number(8),
            Array(vec![100, 200, 200]),
            Number(-1),
            Array(vec![-1]),
        ]
    );
}

#[test]
fn unicode_empty_map_keeps_complete_cmap_wrapper() {
    assert_eq!(unicode_body(&BTreeMap::new()), "");
    // An empty value vector represents no std::multimap entries.
    assert_eq!(unicode_body(&BTreeMap::from([(1, Vec::new())])), "");
}

#[test]
fn unicode_emits_bfchar_then_explicit_ranges_then_consecutive_ranges() {
    let mappings = BTreeMap::from([
        (0, vec![0x20]),
        (2, vec![0xd800]),
        (4, vec![0xdfff]),
        (6, vec![0x1f600]),
        (8, vec![0x10ffff]),
        (0x10, vec![0x40]),
        (0x11, vec![0x50]),
        (0x12, vec![0x51]),
        (0x20, vec![0x60]),
        (0x21, vec![0x61]),
        (0x22, vec![0x62]),
    ]);
    assert_eq!(
        unicode_body(&mappings),
        concat!(
            "5 beginbfchar\n",
            "<0000> <0020>\n",
            "<0002> <0000>\n",
            "<0004> <0000>\n",
            "<0006> <D83DDE00>\n",
            "<0008> <DBFFDFFF>\n",
            "endbfchar\n",
            "1 beginbfrange\n",
            "<0010> <0012> [<0040> <0050> <0051>]\n",
            "endbfrange\n",
            "1 beginbfrange\n",
            "<0020> <0022> <0060>\n",
            "endbfrange\n",
        )
    );
    let stream = load_unicode(&mappings);
    let decoded = CpdfToUnicodeMap::from_stream(&stream);
    for (&charcode, unicodes) in &mappings {
        let unicode = unicodes[0];
        let expected = if is_high_surrogate(unicode) || is_low_surrogate(unicode) {
            vec![0]
        } else if is_supplementary(unicode) {
            let pair = SurrogatePair::from_code_point(unicode);
            vec![u32::from(pair.high()), u32::from(pair.low())]
        } else {
            vec![unicode]
        };
        assert_eq!(decoded.lookup(charcode), expected);
    }
}

#[test]
fn unicode_charcode_byte_boundaries_split_ranges_and_preserve_singleton_pair() {
    let mappings = BTreeMap::from([
        (0xfe, vec![0xfe]),
        (0xff, vec![0xff]),
        (0x100, vec![0x100]),
        (0x101, vec![0x101]),
    ]);
    assert_eq!(
        unicode_body(&mappings),
        concat!(
            "2 beginbfrange\n",
            "<00FE> <00FF> <00FE>\n",
            "<0100> <0101> <0100>\n",
            "endbfrange\n",
        )
    );
    let mappings = BTreeMap::from([(0xff, vec![0x20]), (0x100, vec![0x21]), (0x101, vec![0x22])]);
    assert_eq!(
        unicode_body(&mappings),
        concat!(
            "3 beginbfchar\n",
            "<00FF> <0020>\n",
            "<0100> <0021>\n",
            "<0101> <0022>\n",
            "endbfchar\n",
        )
    );
    // The source constrains charcode byte boundaries, rather than Unicode
    // byte boundaries; retain that serialization even for this crossing.
    assert_eq!(
        unicode_body(&BTreeMap::from([(0x30, vec![0xff]), (0x31, vec![0x100])])),
        "1 beginbfrange\n<0030> <0031> <00FF>\nendbfrange\n"
    );
}

#[test]
fn duplicate_glyph_charcodes_keep_native_overlapping_ranges_and_order() {
    // These exact overlapping ranges appear in the upstream custom CID font
    // embedder fixture, reflecting LoadCompositeFont's glyph-keyed multimap.
    let mappings = BTreeMap::from([
        (1, vec![0x20]),
        (2, vec![0x3002]),
        (3, vec![0x2f00, 0x4e00]),
        (4, vec![0x2f06, 0x4e8c]),
        (5, vec![0x53e5, 0xf906]),
        (6, vec![0x662f]),
        (7, vec![0x7b2c]),
        (8, vec![0x884c, 0xfa08]),
        (9, vec![0x8fd9]),
    ]);
    assert_eq!(
        unicode_body(&mappings),
        concat!(
            "5 beginbfrange\n",
            "<0001> <0003> [<0020> <3002> <2F00>]\n",
            "<0003> <0004> [<4E00> <2F06>]\n",
            "<0004> <0005> [<4E8C> <53E5>]\n",
            "<0005> <0008> [<F906> <662F> <7B2C> <884C>]\n",
            "<0008> <0009> [<FA08> <8FD9>]\n",
            "endbfrange\n",
        )
    );
    // Repeated isolated charcodes use map assignment: the last value wins.
    assert_eq!(
        unicode_body(&BTreeMap::from([(7, vec![0x20, 0x30])])),
        "1 beginbfchar\n<0007> <0030>\nendbfchar\n"
    );
}

#[test]
fn unicode_blocks_split_after_one_hundred_entries_in_each_form() {
    // Gaps prevent the 101 singleton mappings from becoming ranges.
    let mappings = (0..101).map(|i| (2 * i, vec![i])).collect();
    let body = unicode_body(&mappings);
    assert_eq!(body.matches("100 beginbfchar\n").count(), 1);
    assert_eq!(body.matches("1 beginbfchar\n").count(), 1);
    assert_eq!(body.matches("endbfchar\n").count(), 2);
    assert!(body.ends_with("1 beginbfchar\n<00C8> <0064>\nendbfchar\n"));

    // Leave a gap after each two-character explicit range.
    let mappings = (0..101)
        .flat_map(|i| [(4 * i, vec![0x20]), (4 * i + 1, vec![0x40])])
        .collect();
    let body = unicode_body(&mappings);
    assert_eq!(body.matches("100 beginbfrange\n").count(), 1);
    assert_eq!(body.matches("1 beginbfrange\n").count(), 1);
    assert_eq!(body.matches("endbfrange\n").count(), 2);
    assert!(body.ends_with("1 beginbfrange\n<0190> <0191> [<0020> <0040>]\nendbfrange\n"));

    let mappings = (0..101)
        .flat_map(|i| [(4 * i, vec![0x20]), (4 * i + 1, vec![0x21])])
        .collect();
    let body = unicode_body(&mappings);
    assert_eq!(body.matches("100 beginbfrange\n").count(), 1);
    assert_eq!(body.matches("1 beginbfrange\n").count(), 1);
    assert_eq!(body.matches("endbfrange\n").count(), 2);
    assert!(body.ends_with("1 beginbfrange\n<0190> <0191> <0020>\nendbfrange\n"));
}

#[test]
#[should_panic]
fn unicode_rejects_charcodes_outside_the_upstream_codespace() {
    load_unicode(&BTreeMap::from([(0x10000, vec![0x20])]));
}
