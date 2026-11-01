// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Page-label formatting from `core/fpdfdoc/cpdf_pagelabel.{h,cpp}`.
use super::cpdf_numbertree::CpdfNumberTree;
use crate::fpdfapi::parser::{
    cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::get_direct_object,
};
pub struct CpdfPageLabel<'a> {
    document: Option<&'a CpdfDocument>,
}
impl<'a> CpdfPageLabel<'a> {
    pub fn new(document: Option<&'a CpdfDocument>) -> Self {
        Self { document }
    }
    pub fn get_label(&self, page_index: i32) -> Option<Vec<u32>> {
        let document = self.document?;
        if page_index < 0 || page_index as usize >= document.page_count() {
            return None;
        }
        let root = document.root()?;
        let labels = root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_dict_for(b"PageLabels")?;
        let bound = CpdfNumberTree::new(labels).get_lower_bound(page_index);
        let label = bound
            .as_ref()
            .and_then(|v| v.value.as_ref())
            .and_then(get_direct_object);
        let Some(label) = label else {
            return Some(wide(&(i64::from(page_index) + 1).to_string()));
        };
        let borrowed = label.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return Some(wide(&(i64::from(page_index) + 1).to_string()));
        };
        let mut result = if dict.key_exist(b"P") {
            dict.get_unicode_text_for(b"P")
        } else {
            Vec::new()
        };
        let style = dict.get_byte_string_for(b"S", b"");
        // Widen before arithmetic: invalid St values must not overflow signed
        // C++ integers or panic in Rust; valid labels retain exact formatting.
        let number = i64::from(page_index) - i64::from(bound?.key)
            + i64::from(dict.get_integer_for(b"St", 1));
        result.extend(label_num_portion(number, &style));
        Some(result)
    }
}
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn make_roman(mut num: i64) -> Vec<u32> {
    const ARABIC: [i64; 13] = [1000, 900, 500, 400, 100, 90, 50, 40, 10, 9, 5, 4, 1];
    const ROMAN: [&str; 13] = [
        "m", "cm", "d", "cd", "c", "xc", "l", "xl", "x", "ix", "v", "iv", "i",
    ];
    num %= 1_000_000;
    let mut result = Vec::with_capacity(10);
    for (amount, letters) in ARABIC.into_iter().zip(ROMAN) {
        while num >= amount {
            num -= amount;
            result.extend(letters.chars().map(u32::from));
        }
    }
    result
}
fn make_letters(num: i64) -> Vec<u32> {
    // Native code assumes a nonnegative value. Invalid negative St values
    // cannot form a meaningful repeat count, so safely produce no letters.
    if num <= 0 {
        return Vec::new();
    }
    let num = num - 1;
    let count = (num / 26 + 1) % 1000;
    let ch = u32::from(b'a') + (num % 26) as u32;
    vec![ch; count as usize]
}
fn label_num_portion(num: i64, style: &[u8]) -> Vec<u32> {
    let mut result = match style {
        b"D" => wide(&num.to_string()),
        b"R" | b"r" => make_roman(num),
        b"A" | b"a" => make_letters(num),
        _ => Vec::new(),
    };
    if matches!(style, b"R" | b"A") {
        for ch in &mut result {
            if (u32::from(b'a')..=u32::from(b'z')).contains(ch) {
                *ch -= 32;
            }
        }
    }
    result
}
#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::{
        cpdf_array::CpdfArray,
        cpdf_object::{retain_object, ObjectRef},
        cpdf_scalar_object::CpdfScalarObject,
    };
    fn limits(node: &mut CpdfDictionary, least: i32, greatest: i32) {
        let mut a = CpdfArray::new();
        a.append_new(CpdfScalarObject::number_from_signed(least));
        a.append_new(CpdfScalarObject::number_from_signed(greatest));
        node.set_new_for(b"Limits", a);
    }
    fn leaf(
        least: i32,
        greatest: i32,
        values: &[(i32, Option<&[u8]>, Option<&[u8]>, Option<i32>)],
    ) -> ObjectRef {
        let mut node = CpdfDictionary::new();
        limits(&mut node, least, greatest);
        let mut nums = CpdfArray::new();
        for &(key, style, prefix, start) in values {
            nums.append_new(CpdfScalarObject::number_from_signed(key));
            let mut label = CpdfDictionary::new();
            label.set_new_name_for(b"Type", b"PageLabel");
            if let Some(style) = style {
                label.set_new_name_for(b"S", style);
            }
            if let Some(prefix) = prefix {
                label.set_new_string_for(b"P", prefix);
            }
            if let Some(start) = start {
                label.set_new_for(b"St", CpdfScalarObject::number_from_signed(start));
            }
            nums.append_new(label);
        }
        node.set_new_for(b"Nums", nums);
        retain_object(node)
    }
    fn branch(least: i32, greatest: i32, kids: Vec<ObjectRef>) -> ObjectRef {
        let mut node = CpdfDictionary::new();
        limits(&mut node, least, greatest);
        let mut array = CpdfArray::new();
        for kid in kids {
            array.append(kid);
        }
        node.set_new_for(b"Kids", array);
        retain_object(node)
    }
    fn fixture() -> CpdfDocument {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        for index in 0..10001 {
            assert!(document.create_new_page(index).is_some());
        }
        let grand2 = branch(
            0,
            5000,
            vec![
                leaf(
                    0,
                    900,
                    &[
                        (0, Some(b"R"), None, None),
                        (100, Some(b"A"), Some(b"abc"), Some(5)),
                        (900, Some(b"D"), None, Some(999)),
                    ],
                ),
                leaf(
                    3000,
                    5000,
                    &[
                        (3000, Some(b"r"), None, None),
                        (5000, Some(b"a"), None, None),
                    ],
                ),
            ],
        );
        let kid1 = branch(
            0,
            8000,
            vec![grand2, leaf(8000, 8000, &[(8000, None, Some(b"x"), None)])],
        );
        let mut rootlabels = CpdfDictionary::new();
        let mut rootkids = CpdfArray::new();
        rootkids.append(kid1);
        rootlabels.set_new_for(b"Kids", rootkids);
        document
            .root()
            .unwrap()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"PageLabels", rootlabels);
        document
    }
    // Complete core/fpdfdoc/cpdf_pagelabel_unittest.cpp PageLabelTest.GetLabel.
    #[test]
    fn GetLabel() {
        let document = fixture();
        let labels = CpdfPageLabel::new(Some(&document));
        assert_eq!(labels.get_label(-1), None);
        assert_eq!(labels.get_label(10001), None);
        let values = [
            (0, "I"),
            (1, "II"),
            (37, "XXXVIII"),
            (99, "C"),
            (100, "abcE"),
            (101, "abcF"),
            (525, "abcNNNNNNNNNNNNNNNNN"),
            (899, "abcXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX"),
            (900, "999"),
            (901, "1000"),
            (1234, "1333"),
            (2999, "3098"),
            (3000, "i"),
            (3001, "ii"),
            (3579, "dlxxx"),
            (4999, "mm"),
            (5000, "a"),
            (5001, "b"),
            (8000, "x"),
            (8001, "x"),
            (10000, "x"),
        ];
        for (index, value) in values {
            assert_eq!(labels.get_label(index), Some(wide(value)), "page {index}");
        }
        assert_eq!(labels.get_label(7654), Some(wide(&"c".repeat(103))));
        assert_eq!(labels.get_label(7999), Some(wide(&"j".repeat(116))));
    }
    // Complete source GetLabelPerf range; no timing assertion is present.
    #[test]
    fn GetLabelPerf() {
        let document = fixture();
        let labels = CpdfPageLabel::new(Some(&document));
        for index in 0..10001 {
            labels.get_label(index);
        }
    }
    #[test]
    fn absent_labels_invalid_ranges_and_unknown_styles() {
        let mut doc = CpdfDocument::new();
        doc.create_new_doc();
        doc.create_new_page(0).unwrap();
        assert_eq!(CpdfPageLabel::new(None).get_label(0), None);
        assert_eq!(CpdfPageLabel::new(Some(&doc)).get_label(0), None);
        doc.root()
            .unwrap()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"PageLabels", CpdfDictionary::new());
        assert_eq!(CpdfPageLabel::new(Some(&doc)).get_label(0), Some(wide("1")));
        assert!(make_letters(-1).is_empty());
        assert!(make_letters(26000).is_empty());
        assert_eq!(make_roman(1_000_001), wide("i"));
        assert!(label_num_portion(3, b"unknown").is_empty());
    }
}
