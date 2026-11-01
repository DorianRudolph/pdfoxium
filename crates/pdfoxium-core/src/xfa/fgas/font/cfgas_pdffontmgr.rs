// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Embedded AcroForm/DR font matching from `cfgas_pdffontmgr.{h,cpp}`.
//! A weak catalog and retained page-data cache replace the unowned document
//! pointer. No whole-document borrow is held during font loading or callbacks.
use super::cfgas_gefont::CfgasGeFont;
use crate::{
    fpdfapi::{
        font::cpdf_font::CpdfFont,
        page::cpdf_docpagedata::CpdfDocPageData,
        parser::{
            cpdf_document::CpdfDocument,
            cpdf_object::{get_dict_object, ObjectRef, PdfObject},
            fpdf_parser_utility::validate_dict_type,
        },
    },
    fxcrt::fx_codepage::{wide_char_to_multi_byte, FxCodePage},
    fxge::{
        cfx_gemodule::with_ge_module,
        fx_font::{font_style_is_force_bold, font_style_is_italic},
    },
};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    rc::{Rc, Weak},
};
type FontKey = (Vec<u32>, u32, bool);
pub struct CfgasPdfFontMgr {
    root: Option<Weak<RefCell<dyn PdfObject>>>,
    page_data: Rc<CpdfDocPageData>,
    fonts: RefCell<BTreeMap<FontKey, Rc<CfgasGeFont>>>,
}
pub fn ps_name_to_font_name(name: &[u8], bold: bool, italic: bool) -> Vec<u8> {
    const TABLE: [[&[u8]; 5]; 2] = [
        [
            b"Adobe PI Std",
            b"AdobePIStd",
            b"AdobePIStd",
            b"AdobePIStd",
            b"AdobePIStd",
        ],
        [
            b"Myriad Pro Light",
            b"MyriadPro-Light",
            b"MyriadPro-Semibold",
            b"MyriadPro-LightIt",
            b"MyriadPro-SemiboldIt",
        ],
    ];
    TABLE.iter().find(|entry| entry[0] == name).map_or_else(
        || name.to_vec(),
        |entry| entry[1 + usize::from(bold) + 2 * usize::from(italic)].to_vec(),
    )
}
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        Some(0)
    } else {
        haystack
            .windows(needle.len())
            .position(|part| part == needle)
    }
}
pub fn ps_name_matches_dr_font_name(
    name: &[u8],
    bold: bool,
    italic: bool,
    resource_name: &[u8],
    strict: bool,
) -> bool {
    if name.is_empty() {
        return false;
    }
    let resource: Vec<u8> = resource_name
        .iter()
        .copied()
        .filter(|&ch| ch != b'-')
        .collect();
    let Some(index) = find(&resource, name) else {
        return false;
    };
    if !strict {
        return true;
    }
    if index != 0 {
        return false;
    }
    let mut difference = resource.len() - name.len();
    if difference > 1 || bold || italic {
        let bold_index = find(&resource, b"Bold");
        if bold != bold_index.is_some() {
            return false;
        }
        if let Some(index) = bold_index {
            let Some(remaining) = difference.checked_sub(4) else {
                return false;
            };
            difference = remaining.min(resource.len() - index - 4);
        }
        let italic_length = if find(&resource, b"Italic").is_some() {
            6
        } else if find(&resource, b"It").is_some() {
            2
        } else if find(&resource, b"Oblique").is_some() {
            7
        } else {
            0
        };
        if italic != (italic_length != 0) {
            return false;
        }
        let Some(remaining) = difference.checked_sub(italic_length) else {
            return false;
        };
        difference = remaining;
        if difference > 1 {
            let tail = &resource[resource.len() - difference..];
            if matches!(tail, b"MT" | b"PSMT" | b"Regular" | b"Reg") {
                return true;
            }
            if bold_index.is_some() || italic_length != 0 {
                return false;
            }
            return match name.last() {
                Some(b'L') => resource.ends_with(b"Light"),
                Some(b'R') => resource.ends_with(b"Regular") || resource.ends_with(b"Reg"),
                Some(b'M') => resource.ends_with(b"Medium"),
                _ => false,
            };
        }
    }
    true
}
fn dict_child(parent: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let value = {
        let object = parent.borrow();
        object.as_dictionary()?.get_object_for(key)
    }?;
    get_dict_object(&value)
}
impl CfgasPdfFontMgr {
    pub fn new(document: &CpdfDocument) -> Self {
        Self {
            root: document.root().map(|root| Rc::downgrade(&root)),
            page_data: document.page_data_ref(),
            fonts: RefCell::new(BTreeMap::new()),
        }
    }
    pub fn get_font(&self, family: &[u32], styles: u32, strict: bool) -> Option<Rc<CfgasGeFont>> {
        let key = (family.to_vec(), styles, strict);
        if let Some(font) = self.fonts.borrow().get(&key) {
            return Some(font.clone());
        }
        let length = wide_char_to_multi_byte(FxCodePage::DefAnsi, family, &mut []);
        let mut name = vec![0; length];
        wide_char_to_multi_byte(FxCodePage::DefAnsi, family, &mut name);
        let bold = font_style_is_force_bold(styles);
        let italic = font_style_is_italic(styles);
        let mapped = ps_name_to_font_name(&name, bold, italic);
        let alias = mapped != name;
        let font = self.find_font(&mapped, bold, italic, strict, alias)?;
        self.fonts.borrow_mut().insert(key, font.clone());
        Some(font)
    }
    fn find_font(
        &self,
        name: &[u8],
        bold: bool,
        italic: bool,
        strict: bool,
        alias: bool,
    ) -> Option<Rc<CfgasGeFont>> {
        let root = self.root.as_ref()?.upgrade()?;
        let acroform = dict_child(&root, b"AcroForm")?;
        let resources = dict_child(&acroform, b"DR")?;
        let fonts = dict_child(&resources, b"Font")?;
        // Normalize both the requested alias and resource spelling. PDFium only
        // strips resource hyphens, which makes its MyriadPro-Light alias table
        // unreachable. Matching an empty family is likewise invalid.
        let name: Vec<u8> = name
            .iter()
            .copied()
            .filter(|ch| !matches!(ch, b' ' | b'-'))
            .collect();
        let candidate = {
            let object = fonts.borrow();
            let dict = object.as_dictionary()?;
            dict.with_locked_entries(|entries| {
                entries
                    .iter()
                    .find(|(key, _)| {
                        // The alias table already selects the styled face, including
                        // Semibold/LightIt spellings that contain no literal "Bold".
                        (alias
                            && key
                                .iter()
                                .copied()
                                .filter(|&ch| ch != b'-')
                                .eq(name.iter().copied()))
                            || ps_name_matches_dr_font_name(&name, bold, italic, key, strict)
                    })
                    .map(|(_, font)| font.clone())
            })
        }?;
        let dict = get_dict_object(&candidate)?;
        let valid = {
            let object = dict.borrow();
            validate_dict_type(object.as_dictionary(), b"Font")
        };
        if !valid {
            return None;
        }
        let font = with_ge_module(|module| {
            self.page_data
                .get_font(&dict, &mut module.font_mgr().builtin_mapper_mut())
        })?;
        if !font.is_embedded() {
            return None;
        }
        CfgasGeFont::from_pdf_font(font)
    }
    pub fn load_stock_font(&self, family: &[u8]) -> Option<Rc<CfgasGeFont>> {
        let font = with_ge_module(|module| {
            self.page_data
                .get_stock_font(family, &mut module.font_mgr().builtin_mapper_mut())
        })?;
        CfgasGeFont::from_pdf_font(CpdfFont::Simple(font))
    }
}
#[cfg(test)]
#[path = "cfgas_pdffontmgr_tests.rs"]
mod tests;
