// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/edit/cpdf_font_util.{h,cpp}`. Widths and Unicode streams are
//! returned inline; callers add them to a document's indirect object holder.
//! A `BTreeMap<u32, Vec<u32>>` represents the ordered multimap, retaining the
//! insertion order of Unicode values with the same charcode.

use std::collections::BTreeMap;
use std::fmt::Write;

use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::retain_object;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_extension::{int_to_four_hex_chars, to_utf16be};
use crate::fxcrt::utf16::{is_high_surrogate, is_low_surrogate};

const MAX_BFCHAR_BFRANGE_ENTRIES: u32 = 100;

const TO_UNICODE_START: &str = concat!(
    "/CIDInit /ProcSet findresource begin\n",
    "12 dict begin\n",
    "begincmap\n",
    "/CIDSystemInfo\n",
    "<</Registry (Adobe)\n",
    "/Ordering (Identity)\n",
    "/Supplement 0\n",
    ">> def\n",
    "/CMapName /Adobe-Identity-H def\n",
    "/CMapType 2 def\n",
    "1 begincodespacerange\n",
    "<0000> <FFFF>\n",
    "endcodespacerange\n",
);

const TO_UNICODE_END: &str = concat!(
    "endcmap\n",
    "CMapName currentdict /CMap defineresource pop\n",
    "end\n",
    "end\n",
);

fn add_charcode(buffer: &mut String, number: u32) {
    assert!(number <= 0xffff);
    buffer.push('<');
    let mut ans = [0; 4];
    int_to_four_hex_chars(number as u16, &mut ans);
    for c in ans {
        buffer.push(char::from(c));
    }
    buffer.push('>');
}

// PDF spec 1.7 Section 5.9.2: "Unicode character sequences as expressed in
// UTF-16BE encoding." See https://en.wikipedia.org/wiki/UTF-16#Description
fn add_unicode(buffer: &mut String, mut unicode: u32) {
    if is_high_surrogate(unicode) || is_low_surrogate(unicode) {
        unicode = 0;
    }
    let mut unicode_buf = [0; 8];
    let unicode_span = to_utf16be(unicode, &mut unicode_buf);
    assert!(!unicode_span.is_empty());
    buffer.push('<');
    for &c in unicode_span {
        buffer.push(char::from(c));
    }
    buffer.push('>');
}

/// Creates the CID widths array. The caller adds it to a document if needed.
pub fn create_widths_array(widths: &BTreeMap<u32, u32>) -> CpdfArray {
    let mut widths_array = CpdfArray::new();
    let mut entries = widths.iter().map(|(&cid, &width)| (cid, width)).peekable();
    while let Some((mut cid, width)) = entries.next() {
        let first_cid = cid;
        if entries.peek().is_some_and(|&(next_cid, next_width)| {
            next_cid == cid.wrapping_add(1) && next_width == width
        }) {
            // The array can have a group c_first c_last w: all CIDs in the
            // range from c_first to c_last will have width w.
            widths_array.append_new(CpdfScalarObject::number_from_signed(first_cid as i32));
            while entries.peek().is_some_and(|&(next_cid, next_width)| {
                next_cid == cid.wrapping_add(1) && next_width == width
            }) {
                cid = entries.next().unwrap().0;
            }
            widths_array.append_new(CpdfScalarObject::number_from_signed(cid as i32));
            widths_array.append_new(CpdfScalarObject::number_from_signed(width as i32));
            continue;
        }
        // Otherwise we can have a group of the form c [w1 w2 ...]: c has
        // width w1, c+1 has width w2, etc. A group may contain a single item.
        widths_array.append_new(CpdfScalarObject::number_from_signed(first_cid as i32));
        let mut current_width_array = CpdfArray::new();
        current_width_array.append_new(CpdfScalarObject::number_from_signed(width as i32));
        while entries
            .peek()
            .is_some_and(|&(next_cid, _)| next_cid == cid.wrapping_add(1))
        {
            let (next_cid, next_width) = entries.next().unwrap();
            cid = next_cid;
            current_width_array.append_new(CpdfScalarObject::number_from_signed(next_width as i32));
        }
        widths_array.append(retain_object(current_width_array));
    }
    widths_array
}

/// Serialize complete ToUnicode destinations when retaining an existing font
/// mapping. Unlike `load_unicode`'s ordered multimap of alternative scalar
/// mappings, each value here is one Unicode sequence for one source code.
pub fn load_unicode_sequences(to_unicode: &BTreeMap<u32, Vec<u32>>) -> CpdfStream {
    let mut buffer = String::from(TO_UNICODE_START);
    let entries: Vec<_> = to_unicode.iter().collect();
    for chunk in entries.chunks(MAX_BFCHAR_BFRANGE_ENTRIES as usize) {
        writeln!(buffer, "{} beginbfchar", chunk.len()).unwrap();
        for (&charcode, sequence) in chunk {
            add_charcode(&mut buffer, charcode);
            buffer.push_str(" <");
            for &value in sequence.iter() {
                if let Ok(unit) = u16::try_from(value) {
                    // Font UnicodeFromCharCode returns UTF-16 code units,
                    // including surrogate pairs and multi-character ligatures.
                    let mut hex = [0; 4];
                    int_to_four_hex_chars(unit, &mut hex);
                    for byte in hex {
                        buffer.push(char::from(byte));
                    }
                } else {
                    let mut hex = [0; 8];
                    for &byte in to_utf16be(value, &mut hex) {
                        buffer.push(char::from(byte));
                    }
                }
            }
            buffer.push_str(">\n");
        }
        buffer.push_str("endbfchar\n");
    }
    buffer.push_str(TO_UNICODE_END);
    CpdfStream::from_data_and_dict(buffer.into_bytes(), retain_object(CpdfDictionary::new()))
}

/// Loads the charcode-to-Unicode ordered multimap into an inline stream.
pub fn load_unicode(to_unicode: &BTreeMap<u32, Vec<u32>>) -> CpdfStream {
    // A map charcode->unicode.
    let mut char_to_unicode_map = BTreeMap::new();
    // A map <char_start, char_end> to vector v, abbreviating start->v[0],
    // start+1->v[1], etc. Only the last byte of the charcode may change.
    let mut char_range_to_unicodes_map = BTreeMap::new();
    // A map <start, end> -> unicode, abbreviating start->unicode,
    // start+1->unicode+1, etc.
    let mut char_range_to_consecutive_unicodes_map = BTreeMap::new();

    // Calculate the maps.
    let mut entries = to_unicode
        .iter()
        .flat_map(|(&charcode, values)| values.iter().map(move |&unicode| (charcode, unicode)))
        .peekable();
    while let Some((first_charcode, first_unicode)) = entries.next() {
        if entries
            .peek()
            .is_none_or(|&(next_charcode, _)| first_charcode.wrapping_add(1) != next_charcode)
        {
            char_to_unicode_map.insert(first_charcode, first_unicode);
            continue;
        }
        let (mut current_charcode, mut current_unicode) = entries.next().unwrap();
        if current_charcode % 256 == 0 {
            char_to_unicode_map.insert(first_charcode, first_unicode);
            char_to_unicode_map.insert(current_charcode, current_unicode);
            continue;
        }
        let max_extra = 255 - current_charcode % 256;
        if first_unicode.wrapping_add(1) != current_unicode {
            // Consecutive charcodes mapping to non-consecutive Unicodes.
            let mut unicodes = vec![first_unicode, current_unicode];
            for _ in 0..max_extra {
                if entries.peek().is_none_or(|&(next_charcode, _)| {
                    current_charcode.wrapping_add(1) != next_charcode
                }) {
                    break;
                }
                let (next_charcode, next_unicode) = entries.next().unwrap();
                current_charcode = next_charcode;
                unicodes.push(next_unicode);
            }
            assert_eq!(
                current_charcode
                    .wrapping_sub(first_charcode)
                    .wrapping_add(1) as usize,
                unicodes.len()
            );
            char_range_to_unicodes_map.insert((first_charcode, current_charcode), unicodes);
            continue;
        }
        // Consecutive charcodes mapping to consecutive Unicodes.
        for _ in 0..max_extra {
            if entries.peek().is_none_or(|&(next_charcode, next_unicode)| {
                current_charcode.wrapping_add(1) != next_charcode
                    || current_unicode.wrapping_add(1) != next_unicode
            }) {
                break;
            }
            (current_charcode, current_unicode) = entries.next().unwrap();
        }
        char_range_to_consecutive_unicodes_map
            .insert((first_charcode, current_charcode), first_unicode);
    }

    let mut buffer = String::from(TO_UNICODE_START);
    {
        // Add char_to_unicode_map to buffer.
        let mut to_process = u32::try_from(char_to_unicode_map.len()).expect("bfchar count");
        let mut entries = char_to_unicode_map.iter();
        while to_process != 0 {
            let this_iteration = to_process.min(MAX_BFCHAR_BFRANGE_ENTRIES);
            writeln!(buffer, "{this_iteration} beginbfchar").unwrap();
            for _ in 0..this_iteration {
                let (&charcode, &unicode) = entries.next().expect("bfchar entry");
                add_charcode(&mut buffer, charcode);
                buffer.push(' ');
                add_unicode(&mut buffer, unicode);
                buffer.push('\n');
            }
            buffer.push_str("endbfchar\n");
            to_process -= this_iteration;
        }
    }
    {
        // Add char_range_to_unicodes_map to buffer.
        let mut to_process =
            u32::try_from(char_range_to_unicodes_map.len()).expect("bfrange count");
        let mut entries = char_range_to_unicodes_map.iter();
        while to_process != 0 {
            let this_iteration = to_process.min(MAX_BFCHAR_BFRANGE_ENTRIES);
            writeln!(buffer, "{this_iteration} beginbfrange").unwrap();
            for _ in 0..this_iteration {
                let (&(first, last), unicodes) = entries.next().expect("bfrange entry");
                add_charcode(&mut buffer, first);
                buffer.push(' ');
                add_charcode(&mut buffer, last);
                buffer.push_str(" [");
                add_unicode(&mut buffer, unicodes[0]);
                for &unicode in &unicodes[1..] {
                    buffer.push(' ');
                    add_unicode(&mut buffer, unicode);
                }
                buffer.push_str("]\n");
            }
            buffer.push_str("endbfrange\n");
            to_process -= this_iteration;
        }
    }
    {
        // Add char_range_to_consecutive_unicodes_map to buffer.
        let mut to_process = u32::try_from(char_range_to_consecutive_unicodes_map.len())
            .expect("consecutive bfrange count");
        let mut entries = char_range_to_consecutive_unicodes_map.iter();
        while to_process != 0 {
            let this_iteration = to_process.min(MAX_BFCHAR_BFRANGE_ENTRIES);
            writeln!(buffer, "{this_iteration} beginbfrange").unwrap();
            for _ in 0..this_iteration {
                let (&(first, last), &unicode) = entries.next().expect("consecutive bfrange entry");
                add_charcode(&mut buffer, first);
                buffer.push(' ');
                add_charcode(&mut buffer, last);
                buffer.push(' ');
                add_unicode(&mut buffer, unicode);
                buffer.push('\n');
            }
            buffer.push_str("endbfrange\n");
            to_process -= this_iteration;
        }
    }
    buffer.push_str(TO_UNICODE_END);
    CpdfStream::from_data_and_dict(buffer.into_bytes(), retain_object(CpdfDictionary::new()))
}

#[cfg(test)]
#[path = "cpdf_font_util_tests.rs"]
mod tests;
