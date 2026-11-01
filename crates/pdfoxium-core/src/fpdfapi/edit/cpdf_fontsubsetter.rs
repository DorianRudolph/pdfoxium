// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! `core/fpdfapi/edit/cpdf_fontsubsetter.{h,cpp}`. Saving creates override
//! objects without changing the original font objects. The safe Rust backend
//! retains glyph IDs and the .notdef outline, validating output before replacing
//! a font. Unsupported outlines retain the original embedded font.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use super::cpdf_font_util::{create_widths_array, load_unicode_sequences};
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::page::cpdf_textobject::CpdfTextObject;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fxcrt::fx_random;
use crate::fxge::fx_font::{
    is_open_type_cff, maybe_remove_subsetted_font_prefix, SUBSETTED_FONT_PREFIX_LENGTH,
};

fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let borrowed = object.borrow();
    f(borrowed.as_any().downcast_ref().expect("font dictionary"))
}

fn dict_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> R {
    let mut borrowed = object.borrow_mut();
    f(borrowed
        .as_any_mut()
        .downcast_mut()
        .expect("font dictionary"))
}

#[path = "cpdf_fontsubsetter_backend.rs"]
mod backend;

fn generate_font_subset(font_data: &[u8], gids: &BTreeSet<u32>) -> Vec<u8> {
    // Native narrows the span length to uint32_t before constructing its blob.
    let native_length = font_data.len() as u32 as usize;
    backend::subset_font_data(&font_data[..native_length], gids).unwrap_or_default()
}

#[cfg(test)]
#[path = "cpdf_fontsubsetter_tests.rs"]
mod tests;

// ISO 32000-1:2008 section 9.6.4: a subset tag is six uppercase letters and '+'.
fn generate_font_subset_name(mut base_font_name: Vec<u8>) -> Vec<u8> {
    maybe_remove_subsetted_font_prefix(&mut base_font_name);
    let mut subset_font_name =
        Vec::with_capacity(SUBSETTED_FONT_PREFIX_LENGTH + base_font_name.len() + 1);
    let mut random_nums = [0; SUBSETTED_FONT_PREFIX_LENGTH];
    fx_random::fill(&mut random_nums);
    for number in random_nums {
        subset_font_name.push(b'A' + (number % 26) as u8);
    }
    subset_font_name.push(b'+');
    subset_font_name.extend_from_slice(&base_font_name);
    subset_font_name
}

struct SubsetCandidate {
    subset_font_name: Vec<u8>,
    root_font: ObjectRef,
    cid_font: Option<ObjectRef>,
    descriptor: ObjectRef,
    font_stream: ObjectRef,
    used_gids: BTreeSet<u32>,
    char_code_to_width: BTreeMap<u32, u32>,
    char_code_to_unicode: BTreeMap<u32, Vec<u32>>,
}

/// Font-subsetting overrides for saving newly added embedded fonts. Type 1
/// fonts are deliberately unsupported, matching PDFium.
pub struct CpdfFontSubsetter {
    document: Rc<RefCell<CpdfDocument>>,
    candidates: BTreeMap<u32, SubsetCandidate>,
}

impl CpdfFontSubsetter {
    pub fn new(document: Rc<RefCell<CpdfDocument>>) -> Self {
        Self {
            document,
            candidates: BTreeMap::new(),
        }
    }

    /// `new_obj_nums` contains all new object numbers, sorted ascending.
    pub fn generate_object_overrides(&mut self, new_obj_nums: &[u32]) -> BTreeMap<u32, ObjectRef> {
        if new_obj_nums.is_empty() {
            return BTreeMap::new();
        }
        self.candidates.clear();
        self.collect_subset_candidates(new_obj_nums);
        let mut overrides = BTreeMap::new();
        for (&obj_num, candidate) in &self.candidates {
            let original_stream_acc =
                CpdfRetainedStreamAcc::load_filtered(Rc::clone(&candidate.font_stream))
                    .expect("font stream");
            let original_stream_span = original_stream_acc.get_span();
            let subsetted_font_data =
                generate_font_subset(&original_stream_span, &candidate.used_gids);
            if subsetted_font_data.is_empty() {
                continue;
            }
            let is_opentype_cff = is_open_type_cff(&original_stream_span);
            // Override the font file stream. Only Type 1 and TrueType need Length1.
            let mut subsetted_font_dict = CpdfDictionary::new();
            if is_opentype_cff {
                subsetted_font_dict.set_new_name_for(b"Subtype", b"OpenType");
            } else {
                subsetted_font_dict.set_new_for(
                    b"Length1",
                    CpdfScalarObject::number_from_signed(subsetted_font_data.len() as i32),
                );
            }
            overrides.insert(
                obj_num,
                retain_object(CpdfStream::from_data_and_dict(
                    subsetted_font_data,
                    retain_object(subsetted_font_dict),
                )),
            );

            // These dictionary copies are the save overrides required upstream;
            // the original graph stays unchanged.
            let new_root_font = candidate.root_font.borrow().clone_object();
            dict_mut(&new_root_font, |d| {
                d.set_new_name_for(b"BaseFont", &candidate.subset_font_name)
            });
            overrides.insert(candidate.root_font.borrow().obj_num(), new_root_font);

            if let Some(cid_font) = &candidate.cid_font {
                let new_cid_font = cid_font.borrow().clone_object();
                dict_mut(&new_cid_font, |d| {
                    d.set_new_name_for(b"BaseFont", &candidate.subset_font_name);
                    if is_opentype_cff {
                        d.set_new_name_for(b"Subtype", b"CIDFontType0");
                    }
                });
                overrides.insert(cid_font.borrow().obj_num(), new_cid_font);
                if let Some(original_widths) = dict(cid_font, |d| d.get_array_for(b"W")) {
                    overrides.insert(
                        original_widths.borrow().obj_num(),
                        retain_object(create_widths_array(&candidate.char_code_to_width)),
                    );
                }
            }

            let new_descriptor = candidate.descriptor.borrow().clone_object();
            dict_mut(&new_descriptor, |d| {
                d.set_new_name_for(b"FontName", &candidate.subset_font_name);
                if is_opentype_cff {
                    // Subsetted CFF glyphs must use the symbolic flag, preserving
                    // the source's removal of NONSYMBOLIC and FontFile2.
                    let flags = (d.get_integer_for(b"Flags", 0) | 0x04) & !0x20;
                    d.set_new_for(b"Flags", CpdfScalarObject::number_from_signed(flags));
                    d.remove_for(b"FontFile2");
                    d.set_new_for(
                        b"FontFile3",
                        CpdfReference::new(&self.document.borrow().holder(), obj_num),
                    );
                }
            });
            overrides.insert(candidate.descriptor.borrow().obj_num(), new_descriptor);
            if let Some(to_unicode) = dict(&candidate.root_font, |d| d.get_stream_for(b"ToUnicode"))
            {
                overrides.insert(
                    to_unicode.borrow().obj_num(),
                    retain_object(load_unicode_sequences(&candidate.char_code_to_unicode)),
                );
            }
        }
        overrides
    }

    fn collect_subset_candidates(&mut self, new_obj_nums: &[u32]) {
        let count = self.document.borrow().page_count();
        for index in 0..count {
            let (page_dict, page_data) = {
                let mut document = self.document.borrow_mut();
                (document.page_dictionary(index), document.page_data_ref())
            };
            let Some(page_dict) = page_dict else { continue };
            // Parsing content and loading embedded font streams can invoke the
            // caller's read callback. Release the document borrow beforehand.
            let Some(mut page) = CpdfPage::new_with_page_data(page_dict, page_data) else {
                continue;
            };
            page.parse_path_content();
            self.collect_subset_candidates_from_page(&page, new_obj_nums);
        }
    }

    fn collect_subset_candidates_from_page(&mut self, page: &CpdfPage, new_obj_nums: &[u32]) {
        for page_object in page.holder().iter() {
            let Some(text) = page_object.as_any().downcast_ref::<CpdfTextObject>() else {
                continue;
            };
            let font = text.font().expect("parsed text font");
            let root_font = font.font_dict().expect("font dictionary");
            if new_obj_nums
                .binary_search(&root_font.borrow().obj_num())
                .is_err()
            {
                continue;
            }
            let (cid_font, descriptor) = if font.is_cid_font() {
                let descendants = dict(&root_font, |d| d.get_array_for(b"DescendantFonts"))
                    .expect("CID descendant fonts");
                let borrowed = descendants.borrow();
                let cid_font = borrowed
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .expect("descendant array")
                    .get_dict_at(0)
                    .expect("CID font");
                let descriptor = dict(&cid_font, |d| d.get_dict_for(b"FontDescriptor"));
                (Some(cid_font), descriptor)
            } else {
                (
                    None,
                    dict(&root_font, |d| d.get_dict_for(b"FontDescriptor")),
                )
            };
            let Some(descriptor) = descriptor else {
                continue;
            };
            // Internal embedded streams use FontFile/FontFile2; Type 1's
            // FontFile is ignored because HarfBuzz cannot subset it.
            let Some(font_stream) = dict(&descriptor, |d| d.get_stream_for(b"FontFile2")) else {
                continue;
            };
            let obj_num = font_stream.borrow().obj_num();
            let candidate = self
                .candidates
                .entry(obj_num)
                .or_insert_with(|| SubsetCandidate {
                    subset_font_name: generate_font_subset_name(font.base_font_name().to_vec()),
                    root_font,
                    cid_font: cid_font.as_ref().map(Rc::clone),
                    descriptor,
                    font_stream,
                    used_gids: BTreeSet::new(),
                    char_code_to_width: BTreeMap::new(),
                    char_code_to_unicode: BTreeMap::new(),
                });
            let subset_widths = cid_font
                .as_ref()
                .is_some_and(|f| dict(f, |d| d.get_array_for(b"W")).is_some());
            Self::add_used_text(text, candidate, subset_widths);
        }
    }

    fn add_used_text(text: &CpdfTextObject, candidate: &mut SubsetCandidate, subset_widths: bool) {
        let font = text.font().expect("text font");
        for &char_code in text.char_codes() {
            let gid = font.glyph_from_char_code(char_code);
            if gid != -1 {
                candidate.used_gids.insert(gid as u32);
            }
            if subset_widths {
                let width = font.get_char_width(char_code);
                if width >= 0 {
                    candidate.char_code_to_width.insert(char_code, width as u32);
                }
            }
            let unicode = font.unicode_from_char_code(char_code);
            if !unicode.is_empty() {
                candidate
                    .char_code_to_unicode
                    .entry(char_code)
                    .or_insert(unicode);
            }
        }
    }
}
