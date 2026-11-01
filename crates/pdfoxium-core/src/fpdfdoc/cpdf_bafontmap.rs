// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Annotation appearance fonts from `core/fpdfdoc/cpdf_bafontmap.{h,cpp}`.
//! The map retains fonts and annotation dictionaries, with an unowned document
//! backedge. Native font creation uses the portable DocPageData::AddFont
//! path; it creates PDF font resources, without invoking platform APIs.
use super::cpdf_defaultappearance::CpdfDefaultAppearance;
use super::cpdf_formfield::{dict_bytes, dict_get, dict_mut, field_attr, typed};
use super::ipvt_fontmap::IpvtFontMap;
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::font::cpdf_fontencoding::{CpdfFontEncoding, FontEncoding};
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{get_dict_object, retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_codepage::{
    charset_from_codepage, charset_from_int, codepage_from_charset, get_acp, FxCharset,
};
use crate::fxge::cfx_font::{
    charset_from_unicode, default_font_name_by_charset, CfxFont, DEFAULT_ANSI_FONT_NAME,
    UNIVERSAL_DEFAULT_FONT_NAME,
};
use crate::fxge::cfx_gemodule::with_ge_module;
use crate::fxge::cfx_standardfont::get_standard_font_index;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

struct Data {
    charset: FxCharset,
    font: Option<CpdfFont>,
    name: Vec<u8>,
}
pub struct CpdfBaFontMap {
    document: Weak<RefCell<CpdfDocument>>,
    annot_dict: ObjectRef,
    ap_type: Vec<u8>,
    data: RefCell<Vec<Data>>,
    native_fonts: RefCell<Vec<(FxCharset, Vec<u8>)>>,
}
fn dictionary(object: Option<ObjectRef>) -> Option<ObjectRef> {
    typed(object, ObjectType::Dictionary)
}
fn child_dict(parent: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    dictionary(dict_get(parent, key))
}
#[cfg(test)]
fn ensure_dict(parent: &ObjectRef, key: &[u8]) -> ObjectRef {
    if let Some(dict) = child_dict(parent, key) {
        return dict;
    }
    dict_mut(parent, |dict| dict.set_new_dictionary_for(key))
}
fn subst_charset(font: &CpdfFont) -> Option<FxCharset> {
    let subst = match font {
        CpdfFont::Simple(font) => font.font().subst_font(),
        CpdfFont::Cid(font) => font.font().subst_font(),
        CpdfFont::Type3(_) => None,
    }?;
    Some(charset_from_int(i32::from(subst.charset())))
}
fn unicode_compatible(font: &CpdfFont) -> bool {
    match font {
        CpdfFont::Simple(font) => font.base().simple.is_unicode_compatible(),
        CpdfFont::Cid(font) => font.metrics.borrow().is_unicode_compatible(),
        CpdfFont::Type3(font) => font.simple.is_unicode_compatible(),
    }
}
fn shallow_dictionary(object: &ObjectRef) -> ObjectRef {
    let borrowed = object.borrow();
    let old = borrowed
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .expect("dictionary");
    let mut dict = CpdfDictionary::with_pool(old.byte_string_pool());
    old.with_locked_entries(|entries| {
        for (key, value) in entries {
            dict.set_for(key, Some(Rc::clone(value)));
        }
    });
    retain_object(dict)
}
fn encode_font_alias(name: &[u8], charset: FxCharset) -> Vec<u8> {
    let mut alias: Vec<u8> = name.iter().copied().filter(|&c| c != b' ').collect();
    alias.extend_from_slice(format!("_{:02X}", charset as u8).as_bytes());
    alias
}
impl CpdfBaFontMap {
    pub fn new(
        document: &Rc<RefCell<CpdfDocument>>,
        annot_dict: ObjectRef,
        ap_type: &[u8],
    ) -> Self {
        let map = Self {
            document: Rc::downgrade(document),
            annot_dict,
            ap_type: ap_type.to_vec(),
            data: RefCell::new(Vec::new()),
            native_fonts: RefCell::new(Vec::new()),
        };
        let mut charset = FxCharset::Default;
        if let Some((font, name)) = map.annot_default_font() {
            charset = subst_charset(&font).unwrap_or_else(|| {
                if [
                    b"Wingdings".as_slice(),
                    b"Wingdings2",
                    b"Wingdings3",
                    b"Webdings",
                ]
                .contains(&name.as_slice())
                {
                    FxCharset::Symbol
                } else {
                    FxCharset::Ansi
                }
            });
            map.add_font_data(Some(font.clone()), name.clone(), charset);
            map.add_font_to_annot_dict(Some(&font), &name);
        }
        if charset != FxCharset::Ansi {
            map.font_index(DEFAULT_ANSI_FONT_NAME.as_bytes(), FxCharset::Ansi, false);
        }
        map
    }
    pub fn native_charset() -> FxCharset {
        charset_from_codepage(get_acp())
    }
    fn document_parts(&self) -> Option<(HolderRef, Rc<CpdfDocPageData>, Option<ObjectRef>)> {
        let document = self.document.upgrade()?;
        let doc = document.borrow();
        Some((doc.holder(), doc.page_data_ref(), doc.root()))
    }
    fn load_font(&self, dict: &ObjectRef) -> Option<CpdfFont> {
        let (_, data, _) = self.document_parts()?;
        with_ge_module(|module| data.get_font(dict, &mut module.font_mgr().builtin_mapper_mut()))
    }
    fn annot_default_font(&self) -> Option<(CpdfFont, Vec<u8>)> {
        let (_, _, root) = self.document_parts()?;
        let widget = dict_bytes(&self.annot_dict, b"Subtype") == b"Widget";
        let acroform = if widget {
            root.and_then(|root| child_dict(&root, b"AcroForm"))
        } else {
            None
        };
        let mut da =
            field_attr(&self.annot_dict, b"DA").map_or_else(Vec::new, |o| o.borrow().get_string());
        if widget && da.is_empty() {
            da = acroform
                .as_ref()
                .and_then(|d| field_attr(d, b"DA"))
                .map_or_else(Vec::new, |o| o.borrow().get_string());
        }
        if da.is_empty() {
            return None;
        }
        let name = CpdfDefaultAppearance::new(da)
            .font()
            .map_or_else(Vec::new, |f| f.name);
        let normal_font = child_dict(&self.annot_dict, b"AP")
            .and_then(|d| child_dict(&d, b"N"))
            .and_then(|d| child_dict(&d, b"Resources"))
            .and_then(|d| child_dict(&d, b"Font"))
            .and_then(|d| child_dict(&d, &name));
        let font_dict = normal_font.or_else(|| {
            if widget {
                acroform
                    .and_then(|d| child_dict(&d, b"DR"))
                    .and_then(|d| child_dict(&d, b"Font"))
                    .and_then(|d| child_dict(&d, &name))
            } else {
                None
            }
        })?;
        Some((self.load_font(&font_dict)?, name))
    }
    fn font_same_charset(&self, charset: FxCharset) -> Option<(CpdfFont, Vec<u8>)> {
        if dict_bytes(&self.annot_dict, b"Subtype") != b"Widget" {
            return None;
        }
        let (_, _, root) = self.document_parts()?;
        let fonts = root
            .and_then(|d| child_dict(&d, b"AcroForm"))
            .and_then(|d| child_dict(&d, b"DR"))
            .and_then(|d| child_dict(&d, b"Font"))?;
        // Snapshot handles only; resource loading can invoke a document loader.
        let entries: Vec<_> = {
            let borrow = fonts.borrow();
            let dict = borrow.as_any().downcast_ref::<CpdfDictionary>()?;
            dict.with_locked_entries(|entries| {
                entries
                    .iter()
                    .map(|(key, value)| (key.to_vec(), Rc::clone(value)))
                    .collect()
            })
        };
        let mut found = None;
        for (name, value) in entries {
            let Some(dict) = get_dict_object(&value) else {
                continue;
            };
            let kind = dict_bytes(&dict, b"Type");
            if !kind.is_empty() && kind != b"Font" {
                continue;
            }
            if let Some(font) = self.load_font(&dict) {
                if subst_charset(&font) == Some(charset) {
                    found = Some((font, name));
                }
            }
        }
        found
    }
    fn add_font_to_annot_dict(&self, font: Option<&CpdfFont>, name: &[u8]) {
        let Some(font) = font else { return };
        let Some((objects, _, _)) = self.document_parts() else {
            return;
        };
        let old_ap = child_dict(&self.annot_dict, b"AP");
        let raw = old_ap.as_ref().and_then(|ap| {
            ap.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .and_then(|d| d.get_object_for(&self.ap_type))
        });
        // A dictionary is a checkbox/radio state map, never an AP stream.
        if raw
            .as_ref()
            .is_some_and(|o| o.borrow().object_type() == ObjectType::Dictionary)
        {
            return;
        }
        let old_stream = old_ap
            .as_ref()
            .and_then(|ap| typed(dict_get(ap, &self.ap_type), ObjectType::Stream));
        let old_stream_dict = old_stream.as_ref().and_then(get_dict_object);
        let old_resources = old_stream_dict
            .as_ref()
            .and_then(|d| child_dict(d, b"Resources"));
        let old_fonts = old_resources.as_ref().and_then(|d| child_dict(d, b"Font"));
        if old_fonts.as_ref().is_some_and(|fonts| {
            fonts
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .is_some_and(|d| d.key_exist(name))
        }) {
            return;
        }
        // Shared AP streams are legal. Isolate just the containers that will
        // change; keep their immutable bytes and referenced assets shared.
        let ap = old_ap
            .as_ref()
            .map_or_else(|| retain_object(CpdfDictionary::new()), shallow_dictionary);
        let stream_dict = old_stream_dict
            .as_ref()
            .map_or_else(|| retain_object(CpdfDictionary::new()), shallow_dictionary);
        let resources = old_resources
            .as_ref()
            .map_or_else(|| retain_object(CpdfDictionary::new()), shallow_dictionary);
        let fonts = old_fonts
            .as_ref()
            .map_or_else(|| retain_object(CpdfDictionary::new()), shallow_dictionary);
        objects.borrow_mut().add_indirect_object(Rc::clone(&fonts));
        let number = fonts.borrow().obj_num();
        dict_mut(&resources, |d| {
            d.set_new_for(b"Font", CpdfReference::new(&objects, number));
        });
        dict_mut(&stream_dict, |d| d.set_for(b"Resources", Some(resources)));
        let stream = if let Some(old) = old_stream {
            let borrowed = old.borrow();
            let old = borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .expect("appearance stream");
            retain_object(old.with_shared_data_and_dict(stream_dict))
        } else {
            retain_object(CpdfStream::from_dict(stream_dict))
        };
        objects.borrow_mut().add_indirect_object(Rc::clone(&stream));
        let number = stream.borrow().obj_num();
        dict_mut(&ap, |d| {
            d.set_new_for(&self.ap_type, CpdfReference::new(&objects, number));
        });
        dict_mut(&self.annot_dict, |d| d.set_for(b"AP", Some(ap)));
        if let Some(font_dict) = font.font_dict() {
            let number = font_dict.borrow().obj_num();
            let object = if number == 0 {
                shallow_dictionary(&font_dict)
            } else {
                retain_object(CpdfReference::new(&objects, number))
            };
            dict_mut(&fonts, |dict| dict.set_for(name, Some(object)));
        }
    }

    fn know_word(&self, index: i32, word: u16) -> bool {
        index >= 0 && self.char_code_from_unicode(index, word) >= 0
    }
    fn add_font_data(&self, font: Option<CpdfFont>, name: Vec<u8>, charset: FxCharset) -> i32 {
        let mut data = self.data.borrow_mut();
        let index = i32::try_from(data.len()).unwrap_or(-1);
        if index >= 0 {
            data.push(Data {
                font,
                name,
                charset,
            });
        }
        index
    }
    fn font_index(&self, name: &[u8], charset: FxCharset, find: bool) -> i32 {
        let alias = encode_font_alias(name, charset);
        if let Some(index) = self.data.borrow().iter().position(|entry| {
            (charset == FxCharset::Default || charset == entry.charset)
                && (alias.is_empty() || alias == entry.name)
        }) {
            return index as i32;
        }
        let found = if find {
            self.font_same_charset(charset)
        } else {
            None
        };
        let (font, alias) = if let Some((font, alias)) = found {
            (Some(font), alias)
        } else {
            (self.add_font_to_document(name, charset), alias)
        };
        self.add_font_to_annot_dict(font.as_ref(), &alias);
        self.add_font_data(font, alias, charset)
    }
    fn native_font_name(&self, mut charset: FxCharset) -> Vec<u8> {
        if charset == FxCharset::Default {
            charset = Self::native_charset();
        }
        let name = default_font_name_by_charset(charset).as_bytes();
        with_ge_module(|module| {
            let mut mapper = module.font_mgr().builtin_mapper_mut();
            mapper.load_installed_fonts();
            if mapper.has_installed_font(name) || mapper.has_localized_font(name) {
                name.to_vec()
            } else {
                Vec::new()
            }
        })
    }
    fn cached_native_font_name(&self, charset: FxCharset) -> Vec<u8> {
        if let Some((_, name)) = self
            .native_fonts
            .borrow()
            .iter()
            .find(|(c, _)| *c == charset)
        {
            return name.clone();
        }
        let name = self.native_font_name(charset);
        if !name.is_empty() {
            self.native_fonts.borrow_mut().push((charset, name.clone()));
        }
        name
    }
    fn add_font_to_document(&self, name: &[u8], mut charset: FxCharset) -> Option<CpdfFont> {
        let (objects, data, _) = self.document_parts()?;
        if get_standard_font_index(name).is_some() {
            return with_ge_module(|module| {
                let encoding = CpdfFontEncoding::new(FontEncoding::WinAnsi);
                data.add_standard_font(
                    name,
                    if name == b"ZapfDingbats" {
                        None
                    } else {
                        Some(&encoding)
                    },
                    &objects,
                    &mut module.font_mgr().builtin_mapper_mut(),
                )
                .map(CpdfFont::Simple)
            });
        }
        let native = if name.is_empty() {
            self.native_font_name(charset)
        } else {
            name.to_vec()
        };
        if charset == FxCharset::Default {
            charset = Self::native_charset();
        }
        with_ge_module(|module| {
            let mut mapper = module.font_mgr().builtin_mapper_mut();
            let mut font = CfxFont::default();
            font.load_subst_face(
                &mut mapper,
                &native,
                true,
                0,
                0,
                0,
                codepage_from_charset(charset),
                false,
            );
            // Missing substitution faces cannot produce usable font resources.
            font.face()?;
            data.add_font(font, charset, &objects, &mut mapper)
        })
    }
}
impl IpvtFontMap for CpdfBaFontMap {
    fn get_pdf_font(&self, index: i32) -> Option<CpdfFont> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.data.borrow().get(i).and_then(|d| d.font.clone()))
    }
    fn get_pdf_font_alias(&self, index: i32) -> Vec<u8> {
        usize::try_from(index)
            .ok()
            .and_then(|i| self.data.borrow().get(i).map(|d| d.name.clone()))
            .unwrap_or_default()
    }
    fn get_word_font_index(&self, word: u16, charset: FxCharset, index: i32) -> i32 {
        if index > 0 {
            if self.know_word(index, word) {
                return index;
            }
        } else {
            let eligible = self.data.borrow().first().is_some_and(|d| {
                charset == FxCharset::Default
                    || d.charset == FxCharset::Symbol
                    || charset == d.charset
            });
            if eligible && self.know_word(0, word) {
                return 0;
            }
        }
        let index = self.font_index(&self.cached_native_font_name(charset), charset, true);
        if index >= 0 && self.know_word(index, word) {
            return index;
        }
        let index = self.font_index(
            UNIVERSAL_DEFAULT_FONT_NAME.as_bytes(),
            FxCharset::Default,
            false,
        );
        if index >= 0 && self.know_word(index, word) {
            return index;
        }
        -1
    }
    fn char_code_from_unicode(&self, index: i32, word: u16) -> i32 {
        let Some(font) = self.get_pdf_font(index) else {
            return -1;
        };
        if unicode_compatible(&font) {
            font.char_code_from_unicode(u32::from(word)) as i32
        } else if word < 0xff {
            i32::from(word)
        } else {
            -1
        }
    }
    fn charset_from_unicode(&self, word: u16, old: FxCharset) -> FxCharset {
        if word < 0x7f {
            FxCharset::Ansi
        } else if old != FxCharset::Default {
            old
        } else {
            charset_from_unicode(word)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn document() -> Rc<RefCell<CpdfDocument>> {
        let mut doc = CpdfDocument::new();
        doc.create_new_doc();
        Rc::new(RefCell::new(doc))
    }
    fn annotation() -> ObjectRef {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", b"Widget");
        dict.set_new_string_for(b"DA", b"0 0 0 rg /F1 12 Tf");
        retain_object(dict)
    }
    #[test]
    fn default_font() {
        let doc = document();
        let annot = annotation();
        let map = CpdfBaFontMap::new(&doc, Rc::clone(&annot), b"N");
        assert_eq!(map.get_pdf_font_alias(0), b"Helvetica_00");
        let font = map.get_pdf_font(0).expect("default font");
        assert!(font.is_type1());
        assert_eq!(font.base_font_name(), b"Helvetica");
        assert_eq!(map.get_word_font_index(b'A' as u16, FxCharset::Ansi, -1), 0);
        assert_eq!(map.char_code_from_unicode(0, b'A' as u16), b'A' as i32);
        let resources = child_dict(&annot, b"AP")
            .and_then(|d| child_dict(&d, b"N"))
            .and_then(|d| child_dict(&d, b"Resources"))
            .and_then(|d| child_dict(&d, b"Font"))
            .expect("appearance resources");
        assert!(child_dict(&resources, b"Helvetica_00").is_some());
        assert!(map.get_pdf_font(-1).is_none());
        assert!(map.get_pdf_font_alias(42).is_empty());
    }
    #[test]
    fn bug853238() {
        let doc = document();
        let root = doc.borrow().root().unwrap();
        let acroform = ensure_dict(&root, b"AcroForm");
        let dr = ensure_dict(&acroform, b"DR");
        let fonts = ensure_dict(&dr, b"Font");
        let font = ensure_dict(&fonts, b"F1");
        dict_mut(&font, |d| {
            d.set_new_name_for(b"Type", b"Font");
            d.set_new_name_for(b"Subtype", b"Type1");
            d.set_new_name_for(b"BaseFont", b"Times-Roman");
        });
        let map = CpdfBaFontMap::new(&doc, annotation(), b"N");
        assert_eq!(map.get_pdf_font_alias(0), b"F1");
        let font = map.get_pdf_font(0).expect("AcroForm font");
        assert!(font.is_type1());
        assert_eq!(font.base_font_name(), b"Times-Roman");
    }
    #[test]
    fn adding_alias_isolates_shared_appearance_resources_without_copying_bytes() {
        let doc = document();
        let first = annotation();
        let second = annotation();
        let old_stream = retain_object(CpdfStream::from_bytes(b"q 1 0 0 1 2 3 cm Q\n"));
        let old_stream_dict = get_dict_object(&old_stream).unwrap();
        let old_resources = ensure_dict(&old_stream_dict, b"Resources");
        let old_fonts = ensure_dict(&old_resources, b"Font");
        let old_font = ensure_dict(&old_fonts, b"Existing");
        dict_mut(&old_font, |d| {
            d.set_new_name_for(b"Type", b"Font");
            d.set_new_name_for(b"Subtype", b"Type1");
            d.set_new_name_for(b"BaseFont", b"Times-Roman");
        });
        let shared_ap = retain_object(CpdfDictionary::new());
        let holder = doc.borrow().holder();
        holder
            .borrow_mut()
            .add_indirect_object(Rc::clone(&old_stream));
        let stream_number = old_stream.borrow().obj_num();
        dict_mut(&shared_ap, |d| {
            d.set_new_for(b"N", CpdfReference::new(&holder, stream_number));
        });
        for annot in [&first, &second] {
            dict_mut(annot, |d| d.set_for(b"AP", Some(Rc::clone(&shared_ap))));
        }
        let original_data = old_stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .share_in_memory_raw_data()
            .unwrap();

        // Missing /F1 forces an actual new alias insertion into the first AP.
        let map = CpdfBaFontMap::new(&doc, Rc::clone(&first), b"N");
        assert_eq!(map.get_pdf_font_alias(0), b"Helvetica_00");
        let first_ap = child_dict(&first, b"AP").unwrap();
        assert!(!Rc::ptr_eq(&first_ap, &shared_ap));
        assert!(Rc::ptr_eq(&child_dict(&second, b"AP").unwrap(), &shared_ap));
        assert!(Rc::ptr_eq(
            &dict_get(&shared_ap, b"N").unwrap(),
            &old_stream
        ));
        assert!(Rc::ptr_eq(
            &child_dict(&old_stream_dict, b"Resources").unwrap(),
            &old_resources
        ));
        assert!(Rc::ptr_eq(
            &child_dict(&old_resources, b"Font").unwrap(),
            &old_fonts
        ));
        assert!(child_dict(&old_fonts, b"Helvetica_00").is_none());
        assert!(Rc::ptr_eq(
            &child_dict(&old_fonts, b"Existing").unwrap(),
            &old_font
        ));

        let new_stream = dict_get(&first_ap, b"N").unwrap();
        assert!(!Rc::ptr_eq(&new_stream, &old_stream));
        let new_stream_dict = get_dict_object(&new_stream).unwrap();
        let new_fonts = child_dict(&new_stream_dict, b"Resources")
            .and_then(|d| child_dict(&d, b"Font"))
            .unwrap();
        assert!(!Rc::ptr_eq(&new_fonts, &old_fonts));
        assert!(child_dict(&new_fonts, b"Helvetica_00").is_some());
        assert!(Rc::ptr_eq(
            &child_dict(&new_fonts, b"Existing").unwrap(),
            &old_font
        ));
        let new_data = new_stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .share_in_memory_raw_data()
            .unwrap();
        assert!(Rc::ptr_eq(&original_data, &new_data));
        assert_eq!(original_data.as_slice(), b"q 1 0 0 1 2 3 cm Q\n");
    }
    #[test]
    fn state_dictionaries_are_not_replaced_with_streams() {
        let doc = document();
        let annot = annotation();
        let ap = ensure_dict(&annot, b"AP");
        let state = ensure_dict(&ap, b"N");
        let map = CpdfBaFontMap::new(&doc, Rc::clone(&annot), b"N");
        assert!(map.get_pdf_font(0).is_some());
        assert!(Rc::ptr_eq(&child_dict(&ap, b"N").unwrap(), &state));
        assert_eq!(
            map.charset_from_unicode(65, FxCharset::ShiftJis),
            FxCharset::Ansi
        );
        assert_eq!(
            map.charset_from_unicode(0x3042, FxCharset::ShiftJis),
            FxCharset::ShiftJis
        );
    }
}
