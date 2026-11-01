// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! InitAcroFormDict and its font-resource helpers from
//! `core/fpdfdoc/cpdf_interactiveform.cpp`, including ordinary AcroForm field
//! trees, controls, notification, reset and FDF export. Platform native fonts
//! remain outside the portable configuration.

use crate::fpdfapi::font::cpdf_facebasedsimplefont::CpdfLoadedSimpleFont;
use crate::fpdfapi::font::cpdf_fontencoding::{CpdfFontEncoding, FontEncoding};
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::fpdf_parser_utility::{
    pdf_name_encode, validate_dict_type, validate_font_resource_dict,
};
use crate::fxge::cfx_font::DEFAULT_ANSI_FONT_NAME;
use crate::fxge::cfx_fontmapper::CfxFontMapper;
use crate::fxge::cfx_fontmgr::with_font_manager;
use std::rc::Rc;

pub fn generate_new_font_resource_name(resources: &CpdfDictionary, prefix: &[u8]) -> Vec<u8> {
    let prefix = if prefix.is_empty() { b"ZiTi" } else { prefix };
    let length = prefix.len();
    let mut m = 0;
    let mut actual = Vec::new();
    while m < 4 && m < length {
        actual.push(prefix[m]);
        m += 1;
    }
    while m < 4 {
        actual.push(b'0' + (m % 10) as u8);
        m += 1;
    }
    let fonts = resources
        .get_dict_for(b"Font")
        .expect("font resource dictionary");
    let borrowed = fonts.borrow();
    let fonts = borrowed
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .expect("dictionary");
    let mut number = 0i32;
    let mut suffix = Vec::new();
    loop {
        let mut key = actual.clone();
        key.extend_from_slice(&suffix);
        if !fonts.key_exist(&key) {
            return key;
        }
        if m < length {
            actual.push(prefix[m]);
            m += 1;
        } else {
            suffix = number.to_string().into_bytes();
            number = number.checked_add(1).expect("font name suffix overflow");
        }
        m += 1;
    }
}

fn find_font(form: &CpdfDictionary, font: &CpdfLoadedSimpleFont) -> Option<Vec<u8>> {
    let resources = form.get_dict_for(b"DR")?;
    let resources = resources.borrow();
    let fonts = resources
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_dict_for(b"Font")?;
    let fonts = fonts.borrow();
    let fonts = fonts.as_any().downcast_ref::<CpdfDictionary>()?;
    if !validate_font_resource_dict(Some(fonts)) {
        return None;
    }
    let font_dict = font.font_dict()?;
    fonts.with_locked_entries(|entries| {
        for (key, value) in entries {
            let Some(dict) = get_direct_object(value) else {
                continue;
            };
            let borrowed = dict.borrow();
            if !validate_dict_type(borrowed.as_any().downcast_ref::<CpdfDictionary>(), b"Font") {
                continue;
            }
            if Rc::ptr_eq(&dict, &font_dict) {
                return Some(key.to_vec());
            }
        }
        None
    })
}

fn add_font(
    form: &mut CpdfDictionary,
    objects: &HolderRef,
    font: &CpdfLoadedSimpleFont,
    alias: &mut Vec<u8>,
) {
    if let Some(existing) = find_font(form, font) {
        *alias = existing;
        return;
    }
    let resources = form.get_or_create_dict_for(b"DR");
    let fonts = resources
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("DR dictionary")
        .get_or_create_dict_for(b"Font");
    if alias.is_empty() {
        *alias = font.base_font_name().to_vec();
    }
    alias.retain(|&byte| byte != b' ');
    *alias = generate_new_font_resource_name(
        resources
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .expect("DR dictionary"),
        alias,
    );
    let dict = font.font_dict().expect("font dictionary");
    let number = dict.borrow().obj_num();
    fonts
        .borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("font dictionary")
        .set_new_for(alias, CpdfReference::new(objects, number));
}

pub fn init_acro_form_dict(
    objects: &HolderRef,
    root: &ObjectRef,
    data: &CpdfDocPageData,
    mapper: &mut CfxFontMapper,
) -> ObjectRef {
    let dict = retain_object(CpdfDictionary::with_pool(
        objects.borrow().byte_string_pool(),
    ));
    let number = objects.borrow_mut().add_indirect_object(Rc::clone(&dict));
    root.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("root dictionary")
        .set_new_for(b"AcroForm", CpdfReference::new(objects, number));
    let encoding = CpdfFontEncoding::new(FontEncoding::WinAnsi);
    let font = data.add_standard_font(
        DEFAULT_ANSI_FONT_NAME.as_bytes(),
        Some(&encoding),
        objects,
        mapper,
    );
    let mut alias = Vec::new();
    if let Some(font) = &font {
        add_font(
            dict.borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .expect("AcroForm dictionary"),
            objects,
            font,
            &mut alias,
        );
    }
    // Non-Windows AddNativeFont returns nullptr, so that optional source branch
    // cannot replace the standard font in this portable configuration.
    let da = if font.is_some() {
        let mut value = vec![b'/'];
        value.extend_from_slice(&pdf_name_encode(&alias));
        value.extend_from_slice(b" 12 Tf 0 g");
        value
    } else {
        b"0 g".to_vec()
    };
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .expect("AcroForm dictionary")
        .set_new_string_for(b"DA", &da);
    dict
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::PdfObject;

    #[test]
    fn static_acroform_initialization_reuses_a_standard_font_and_owns_indirect_resources() {
        let objects =
            crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder::new();
        let root = objects.borrow().new_dictionary();
        let data = CpdfDocPageData::new();
        let mut mapper = CfxFontMapper::new();
        let encoding = CpdfFontEncoding::new(FontEncoding::WinAnsi);
        let first = data
            .add_standard_font(b"Arial", Some(&encoding), &objects, &mut mapper)
            .unwrap();
        let second = data
            .add_standard_font(b"Helvetica", Some(&encoding), &objects, &mut mapper)
            .unwrap();
        assert!(Rc::ptr_eq(&first, &second));
        assert_ne!(first.font_dict().unwrap().borrow().obj_num(), 0);
        let form = init_acro_form_dict(&objects, &root, &data, &mut mapper);
        let borrowed = form.borrow();
        let form = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_ne!(form.obj_num(), 0);
        assert_eq!(form.get_byte_string_for(b"DA", b""), b"/Helv 12 Tf 0 g");
        let resources = form.get_dict_for(b"DR").unwrap();
        let resources = resources.borrow();
        let fonts = resources
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"Font")
            .unwrap();
        let fonts = fonts.borrow();
        let font = fonts
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"Helv")
            .unwrap();
        assert!(Rc::ptr_eq(&font, &first.font_dict().unwrap()));
    }

    #[test]
    fn resource_name_collision_keeps_native_double_increment() {
        let mut fonts = CpdfDictionary::new();
        fonts.set_new_name_for(b"Helv", b"Used");
        fonts.set_new_name_for(b"Helve", b"Used");
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        assert_eq!(
            generate_new_font_resource_name(&resources, b"Helvetica"),
            b"Helvei"
        );
    }
}

// Ordinary interactive-field model. The preceding helpers are shared with
// static FreeText appearance generation and retain their existing signatures.
use super::cpdf_defaultappearance::CpdfDefaultAppearance;
use super::cpdf_filespec::CpdfFileSpec;
use super::cpdf_formcontrol::CpdfFormControl;
use super::cpdf_formfield::{
    array_at, array_len, dict_bytes, dict_get, dict_has, dict_int, dict_mut, field_attr,
    get_full_name_for_dict, typed, CpdfFormField, FieldType,
};
use crate::fpdfapi::font::cpdf_font::CpdfFont;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cfdf_document::CfdfDocument;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::clone_direct_object;
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_encode_text;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_name_decode;
use crate::fxcrt::fx_coordinates::CfxPointF;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Weak;
use std::sync::atomic::{AtomicBool, Ordering};

static UPDATE_AP: AtomicBool = AtomicBool::new(true);

/// The source notifier is non-owning. Forms retain a Weak observer and upgrade
/// it outside storage borrows for each callback, permitting removal/reentrancy.
pub trait FormNotifier {
    fn before_value_change(&self, field: &CpdfFormField, value: &[u32]) -> bool;
    fn after_value_change(&self, field: &CpdfFormField);
    fn before_selection_change(&self, field: &CpdfFormField, value: &[u32]) -> bool;
    fn after_selection_change(&self, field: &CpdfFormField);
    fn after_checked_status_change(&self, field: &CpdfFormField);
    fn after_form_reset(&self, form: &CpdfInteractiveForm);
}
#[derive(Default)]
struct FieldTreeNode {
    short_name: Vec<u32>,
    field: Option<Rc<CpdfFormField>>,
    children: Vec<FieldTreeNode>,
}
impl FieldTreeNode {
    fn find(&self, name: &[u32]) -> Option<&Self> {
        let mut node = self;
        for part in name
            .split(|&c| c == u32::from(b'.'))
            .take_while(|p| !p.is_empty())
        {
            node = node.children.iter().find(|n| n.short_name == part)?;
        }
        Some(node)
    }
    fn set(&mut self, name: &[u32], field: Rc<CpdfFormField>) -> bool {
        let mut node = self;
        let mut level = 0;
        for part in name
            .split(|&c| c == u32::from(b'.'))
            .take_while(|p| !p.is_empty())
        {
            level += 1;
            if level > 32 {
                return false;
            }
            let index = if let Some(index) = node.children.iter().position(|n| n.short_name == part)
            {
                index
            } else {
                node.children.push(Self {
                    short_name: part.to_vec(),
                    ..Self::default()
                });
                node.children.len() - 1
            };
            node = &mut node.children[index];
        }
        if level == 0 {
            return false;
        }
        node.field = Some(field);
        true
    }
    fn count(&self) -> usize {
        usize::from(self.field.is_some()) + self.children.iter().map(Self::count).sum::<usize>()
    }
    fn field_at(&self, index: &mut usize) -> Option<Rc<CpdfFormField>> {
        if let Some(field) = &self.field {
            if *index == 0 {
                return Some(Rc::clone(field));
            }
            *index -= 1;
        }
        self.children.iter().find_map(|c| c.field_at(index))
    }
    fn snapshot_fields(&self, out: &mut Vec<Rc<CpdfFormField>>) {
        if let Some(field) = &self.field {
            out.push(Rc::clone(field));
        }
        for child in &self.children {
            child.snapshot_fields(out);
        }
    }
}
pub struct CpdfInteractiveForm {
    document: Weak<RefCell<CpdfDocument>>,
    form_dict: Option<ObjectRef>,
    field_tree: RefCell<FieldTreeNode>,
    controls: RefCell<HashMap<usize, Rc<CpdfFormControl>>>,
    control_lists: RefCell<HashMap<usize, Vec<Rc<CpdfFormControl>>>>,
    notifier: RefCell<Option<Weak<dyn FormNotifier>>>,
}
fn object_id(object: &ObjectRef) -> usize {
    Rc::as_ptr(object) as *const () as usize
}
impl CpdfInteractiveForm {
    pub fn new(document: &Rc<RefCell<CpdfDocument>>) -> Rc<Self> {
        let root = document.borrow().root();
        let form_dict = root
            .as_ref()
            .and_then(|root| typed(dict_get(root, b"AcroForm"), ObjectType::Dictionary));
        let form = Rc::new(Self {
            document: Rc::downgrade(document),
            form_dict,
            field_tree: RefCell::new(FieldTreeNode::default()),
            controls: RefCell::new(HashMap::new()),
            control_lists: RefCell::new(HashMap::new()),
            notifier: RefCell::new(None),
        });
        let fields = form
            .form_dict
            .as_ref()
            .and_then(|d| typed(dict_get(d, b"Fields"), ObjectType::Array));
        if let Some(fields) = fields {
            for i in 0..array_len(&fields) {
                if let Some(field) = typed(array_at(&fields, i), ObjectType::Dictionary) {
                    form.load_field(field, 0);
                }
            }
        }
        form
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.document.upgrade()
    }
    pub fn form_dict(&self) -> Option<ObjectRef> {
        self.form_dict.clone()
    }
    pub fn is_update_ap_enabled() -> bool {
        UPDATE_AP.load(Ordering::Relaxed)
    }
    pub fn set_update_ap(enabled: bool) {
        UPDATE_AP.store(enabled, Ordering::Relaxed);
    }
    pub fn init_acro_form_dict(document: &Rc<RefCell<CpdfDocument>>) -> Option<ObjectRef> {
        let (objects, root, data) = {
            let doc = document.borrow();
            (doc.holder(), doc.root()?, doc.page_data_ref())
        };
        Some(with_font_manager(|manager| {
            init_acro_form_dict(&objects, &root, &data, &mut manager.builtin_mapper_mut())
        }))
    }
    /// The portable native-font path may reuse an existing substituted ANSI
    /// resource. Native platform enumeration/AddNativeFont is unselected.
    pub fn add_native_interactive_form_font(
        document: &Rc<RefCell<CpdfDocument>>,
        tag: &mut Vec<u8>,
    ) -> Option<CpdfFont> {
        let root = document.borrow().root()?;
        let form = typed(dict_get(&root, b"AcroForm"), ObjectType::Dictionary)
            .or_else(|| Self::init_acro_form_dict(document))?;
        let resources = typed(dict_get(&form, b"DR"), ObjectType::Dictionary)?;
        let fonts = typed(dict_get(&resources, b"Font"), ObjectType::Dictionary)?;
        {
            let b = fonts.borrow();
            if !validate_font_resource_dict(b.as_any().downcast_ref::<CpdfDictionary>()) {
                return None;
            }
        }
        let keys = fonts
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_keys();
        let data = document.borrow().page_data_ref();
        for key in keys {
            let Some(dict) = typed(dict_get(&fonts, &key), ObjectType::Dictionary) else {
                continue;
            };
            {
                let b = dict.borrow();
                if !validate_dict_type(b.as_any().downcast_ref::<CpdfDictionary>(), b"Font") {
                    continue;
                }
            }
            let Some(font) = with_font_manager(|manager| {
                data.get_font(&dict, &mut manager.builtin_mapper_mut())
            }) else {
                continue;
            };
            let charset = font
                .font()
                .and_then(|f| f.subst_font())
                .map(|f| f.charset());
            if charset == Some(0) {
                *tag = key;
                return Some(font);
            }
        }
        None
    }
    pub fn count_fields(&self, name: &[u32]) -> usize {
        let tree = self.field_tree.borrow();
        if name.is_empty() {
            tree.count()
        } else {
            tree.find(name).map_or(0, FieldTreeNode::count)
        }
    }
    pub fn get_field(&self, index: usize, name: &[u32]) -> Option<Rc<CpdfFormField>> {
        let tree = self.field_tree.borrow();
        let node = if name.is_empty() {
            &*tree
        } else {
            tree.find(name)?
        };
        let mut remaining = index;
        node.field_at(&mut remaining)
    }
    pub fn get_field_by_dict(&self, dict: &ObjectRef) -> Option<Rc<CpdfFormField>> {
        let name = get_full_name_for_dict(dict);
        if name.is_empty() {
            return None;
        }
        self.field_tree.borrow().find(&name)?.field.clone()
    }
    pub fn get_control_by_dict(&self, dict: &ObjectRef) -> Option<Rc<CpdfFormControl>> {
        self.controls.borrow().get(&object_id(dict)).cloned()
    }
    pub fn get_control_at_point(
        &self,
        page: &CpdfPage,
        point: CfxPointF,
    ) -> Option<(Rc<CpdfFormControl>, i32)> {
        let annots = page.annots_array()?;
        for index in (0..array_len(&annots)).rev() {
            let Some(dict) = typed(array_at(&annots, index), ObjectType::Dictionary) else {
                continue;
            };
            let Some(control) = self.get_control_by_dict(&dict) else {
                continue;
            };
            if control.rect().contains_point(point) {
                return Some((control, index as i32));
            }
        }
        None
    }
    pub fn need_construct_ap(&self) -> bool {
        self.form_dict
            .as_ref()
            .and_then(|d| dict_get(d, b"NeedAppearances"))
            .is_some_and(|o| {
                o.borrow().object_type() == ObjectType::Boolean && o.borrow().get_integer() != 0
            })
    }
    pub fn count_fields_in_calculation_order(&self) -> usize {
        self.form_dict
            .as_ref()
            .and_then(|d| typed(dict_get(d, b"CO"), ObjectType::Array))
            .map_or(0, |a| array_len(&a))
    }
    pub fn field_in_calculation_order(&self, index: i32) -> Option<Rc<CpdfFormField>> {
        if index < 0 {
            return None;
        }
        let array = typed(dict_get(self.form_dict.as_ref()?, b"CO"), ObjectType::Array)?;
        self.get_field_by_dict(&typed(
            array_at(&array, index as usize),
            ObjectType::Dictionary,
        )?)
    }
    pub fn find_field_in_calculation_order(&self, field: &CpdfFormField) -> i32 {
        let Some(array) = self
            .form_dict
            .as_ref()
            .and_then(|d| typed(dict_get(d, b"CO"), ObjectType::Array))
        else {
            return -1;
        };
        let dict = field.field_dict();
        (0..array_len(&array))
            .find(|&i| array_at(&array, i).is_some_and(|o| Rc::ptr_eq(&dict, &o)))
            .map_or(-1, |i| i as i32)
    }
    pub fn font_for_element(&self, element: ObjectRef) -> Option<CpdfFont> {
        let document = self.document()?;
        let data = document.borrow().page_data_ref();
        with_font_manager(|manager| data.get_font(&element, &mut manager.builtin_mapper_mut()))
    }
    pub fn form_font(&self, name: &[u8]) -> Option<CpdfFont> {
        let name = pdf_name_decode(name);
        if name.is_empty() {
            return None;
        }
        let resources = typed(
            dict_get(self.form_dict.as_ref()?, b"DR"),
            ObjectType::Dictionary,
        )?;
        let fonts = typed(dict_get(&resources, b"Font"), ObjectType::Dictionary)?;
        {
            let b = fonts.borrow();
            if !validate_font_resource_dict(b.as_any().downcast_ref::<CpdfDictionary>()) {
                return None;
            }
        }
        let font = typed(dict_get(&fonts, &name), ObjectType::Dictionary)?;
        {
            let b = font.borrow();
            if !validate_dict_type(b.as_any().downcast_ref::<CpdfDictionary>(), b"Font") {
                return None;
            }
        }
        self.font_for_element(font)
    }
    pub fn default_appearance(&self) -> CpdfDefaultAppearance {
        CpdfDefaultAppearance::new(
            self.form_dict
                .as_ref()
                .map_or_else(Vec::new, |d| dict_bytes(d, b"DA")),
        )
    }
    pub fn form_alignment(&self) -> i32 {
        self.form_dict.as_ref().map_or(0, |d| dict_int(d, b"Q", 0))
    }
    fn fields_snapshot(&self) -> Vec<Rc<CpdfFormField>> {
        let mut fields = Vec::new();
        self.field_tree.borrow().snapshot_fields(&mut fields);
        fields
    }
    pub fn controls_for_field(&self, field: &CpdfFormField) -> Vec<Rc<CpdfFormControl>> {
        self.control_lists
            .borrow()
            .get(&(std::ptr::from_ref(field) as usize))
            .cloned()
            .unwrap_or_default()
    }
    pub fn reset_form(&self) {
        self.reset_form_fields(&[], false);
    }
    pub fn reset_form_fields(&self, fields: &[Rc<CpdfFormField>], include: bool) {
        for field in self.fields_snapshot() {
            if include == fields.iter().any(|f| Rc::ptr_eq(f, &field)) {
                field.reset_field();
            }
        }
        if let Some(notifier) = self.notifier() {
            notifier.after_form_reset(self);
        }
    }
    pub fn check_required_fields(
        &self,
        fields: Option<&[Rc<CpdfFormField>]>,
        include: bool,
    ) -> bool {
        for field in self.fields_snapshot() {
            if matches!(
                field.get_type(),
                FieldType::PushButton | FieldType::CheckBox | FieldType::ListBox
            ) || field.is_no_export()
            {
                continue;
            }
            let found = fields.is_none_or(|fields| fields.iter().any(|f| Rc::ptr_eq(f, &field)));
            if include == found
                && field.is_required()
                && dict_bytes(&field.field_dict(), b"V").is_empty()
            {
                return false;
            }
        }
        true
    }
    pub fn export_to_fdf(&self, path: &[u32]) -> CfdfDocument {
        self.export_fields_to_fdf(path, &self.fields_snapshot(), true)
    }
    pub fn export_fields_to_fdf(
        &self,
        path: &[u32],
        fields: &[Rc<CpdfFormField>],
        include: bool,
    ) -> CfdfDocument {
        let document = CfdfDocument::create_new_doc();
        let root = document.root().expect("new FDF root");
        let main =
            typed(dict_get(&root, b"FDF"), ObjectType::Dictionary).expect("new FDF dictionary");
        if !path.is_empty() {
            let mut filespec = CpdfDictionary::new();
            filespec.set_new_name_for(b"Type", b"Filespec");
            let name = CpdfFileSpec::encode_file_name(path);
            let mut ansi = vec![0; name.len()];
            let length = crate::fxcrt::fx_codepage::wide_char_to_multi_byte(
                crate::fxcrt::fx_codepage::FxCodePage::DefAnsi,
                &name,
                &mut ansi,
            );
            ansi.truncate(length);
            filespec.set_new_for(b"F", CpdfString::from_bytes(None, &ansi));
            filespec.set_new_for(b"UF", CpdfString::from_unicode(None, &name));
            dict_mut(&main, |d| {
                d.set_new_for(b"F", filespec);
            });
        }
        let mut out = CpdfArray::new();
        for field in self.fields_snapshot() {
            let flags = field.field_flags();
            if field.get_type() == FieldType::PushButton
                || flags & 4 != 0
                || include != fields.iter().any(|f| Rc::ptr_eq(f, &field))
                || flags & 2 != 0 && dict_bytes(&field.field_dict(), b"V").is_empty()
            {
                continue;
            }
            let mut exported = CpdfDictionary::new();
            exported.set_new_for(b"T", CpdfString::from_unicode(None, &field.full_name()));
            if matches!(
                field.get_type(),
                FieldType::CheckBox | FieldType::RadioButton
            ) {
                let value = pdf_encode_text(&field.check_value(false));
                if field.get_field_attr(b"Opt").is_some() {
                    exported.set_new_for(b"V", CpdfString::from_bytes(None, &value));
                } else {
                    exported.set_new_name_for(b"V", &value);
                }
            } else if let Some(value) = field.get_field_attr(b"V") {
                exported.set_for(b"V", clone_direct_object(&value));
            }
            out.append_new(exported);
        }
        dict_mut(&main, |d| {
            d.set_new_for(b"Fields", out);
        });
        document
    }
    pub fn set_notifier_iface(&self, notifier: Option<&Rc<dyn FormNotifier>>) {
        *self.notifier.borrow_mut() = notifier.map(Rc::downgrade);
    }
    fn notifier(&self) -> Option<Rc<dyn FormNotifier>> {
        self.notifier.borrow().as_ref().and_then(Weak::upgrade)
    }
    pub fn notify_before_value_change(&self, field: &CpdfFormField, value: &[u32]) -> bool {
        self.notifier()
            .is_none_or(|n| n.before_value_change(field, value))
    }
    pub fn notify_after_value_change(&self, field: &CpdfFormField) {
        if let Some(n) = self.notifier() {
            n.after_value_change(field);
        }
    }
    pub fn notify_before_selection_change(&self, field: &CpdfFormField, value: &[u32]) -> bool {
        self.notifier()
            .is_none_or(|n| n.before_selection_change(field, value))
    }
    pub fn notify_after_selection_change(&self, field: &CpdfFormField) {
        if let Some(n) = self.notifier() {
            n.after_selection_change(field);
        }
    }
    pub fn notify_after_checked_status_change(&self, field: &CpdfFormField) {
        if let Some(n) = self.notifier() {
            n.after_checked_status_change(field);
        }
    }
    pub fn fix_page_fields(self: &Rc<Self>, page: &CpdfPage) {
        if let Some(annots) = page.annots_array() {
            for i in 0..array_len(&annots) {
                if let Some(annot) = typed(array_at(&annots, i), ObjectType::Dictionary) {
                    if dict_bytes(&annot, b"Subtype") == b"Widget" {
                        self.load_field(annot, 0);
                    }
                }
            }
        }
    }
    fn load_field(self: &Rc<Self>, dict: ObjectRef, level: u32) {
        if level > 32 {
            return;
        }
        let Some(kids) = typed(dict_get(&dict, b"Kids"), ObjectType::Array) else {
            self.add_terminal_field(dict);
            return;
        };
        let Some(first) = typed(array_at(&kids, 0), ObjectType::Dictionary) else {
            return;
        };
        if !dict_has(&first, b"T") && !dict_has(&first, b"Kids") {
            self.add_terminal_field(dict);
            return;
        }
        let number = dict.borrow().obj_num();
        for i in 0..array_len(&kids) {
            if let Some(child) = typed(array_at(&kids, i), ObjectType::Dictionary) {
                // Direct children have object number zero too; compare allocation
                // identity so valid direct field trees are not discarded.
                if !Rc::ptr_eq(&dict, &child) && (number == 0 || child.borrow().obj_num() != number)
                {
                    self.load_field(child, level + 1);
                }
            }
        }
    }
    fn add_terminal_field(self: &Rc<Self>, dict: ObjectRef) {
        if !dict_has(&dict, b"FT") && field_attr(&dict, b"FT").is_none() {
            return;
        }
        let name = get_full_name_for_dict(&dict);
        if name.is_empty() {
            return;
        }
        let existing = {
            self.field_tree
                .borrow()
                .find(&name)
                .and_then(|n| n.field.clone())
        };
        let field = if let Some(field) = existing {
            field
        } else {
            let parent = if !dict_has(&dict, b"T") && dict_bytes(&dict, b"Subtype") == b"Widget" {
                typed(dict_get(&dict, b"Parent"), ObjectType::Dictionary)
                    .unwrap_or_else(|| Rc::clone(&dict))
            } else {
                Rc::clone(&dict)
            };
            if !Rc::ptr_eq(&parent, &dict) && !dict_has(&parent, b"FT") {
                for key in [b"FT".as_slice(), b"Ff".as_slice()] {
                    if let Some(value) = dict_get(&dict, key) {
                        let clone = crate::fpdfapi::parser::cpdf_object::clone_non_cyclic(
                            &value,
                            false,
                            &std::collections::HashSet::new(),
                        );
                        dict_mut(&parent, |d| d.set_for(key, clone));
                    }
                }
            }
            let field = CpdfFormField::new(Rc::downgrade(self), parent);
            let raw = dict
                .borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_object_for(b"T");
            if let Some(raw) = raw {
                if raw.borrow().object_type() == ObjectType::Reference {
                    let clone = clone_direct_object(&raw)
                        .filter(|o| o.borrow().object_type() == ObjectType::String)
                        .unwrap_or_else(|| retain_object(CpdfString::from_bytes(None, b"")));
                    dict_mut(&dict, |d| d.set_for(b"T", Some(clone)));
                }
            }
            if !self.field_tree.borrow_mut().set(&name, Rc::clone(&field)) {
                return;
            }
            field
        };
        if let Some(kids) = typed(dict_get(&dict, b"Kids"), ObjectType::Array) {
            for i in 0..array_len(&kids) {
                if let Some(kid) = typed(array_at(&kids, i), ObjectType::Dictionary) {
                    if dict_bytes(&kid, b"Subtype") == b"Widget" {
                        self.add_control(&field, kid);
                    }
                }
            }
        } else if dict_bytes(&dict, b"Subtype") == b"Widget" {
            self.add_control(&field, dict);
        }
    }
    fn add_control(
        self: &Rc<Self>,
        field: &Rc<CpdfFormField>,
        widget: ObjectRef,
    ) -> Rc<CpdfFormControl> {
        let identity = object_id(&widget);
        if let Some(control) = self.controls.borrow().get(&identity).cloned() {
            return control;
        }
        let control = CpdfFormControl::new(field, widget, Rc::downgrade(self));
        self.controls
            .borrow_mut()
            .insert(identity, Rc::clone(&control));
        self.control_lists
            .borrow_mut()
            .entry(Rc::as_ptr(field) as usize)
            .or_default()
            .push(Rc::clone(&control));
        control
    }
}

#[cfg(test)]
mod form_model_tests {
    use super::super::cpdf_formfield::{dict_unicode, NotificationOption};
    use super::*;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn document() -> Rc<RefCell<CpdfDocument>> {
        crate::fpdfsdk::fpdf_editpage::create_new_document(false, || None)
    }
    fn add_fields(doc: &Rc<RefCell<CpdfDocument>>, fields: CpdfArray) -> ObjectRef {
        let root = doc.borrow().root().unwrap();
        let mut form = CpdfDictionary::new();
        form.set_new_for(b"Fields", fields);
        dict_mut(&root, |d| d.set_new_for(b"AcroForm", form))
    }
    fn widget(name: &str, kind: &[u8]) -> CpdfDictionary {
        let mut d = CpdfDictionary::new();
        d.set_new_name_for(b"Type", b"Annot");
        d.set_new_name_for(b"Subtype", b"Widget");
        d.set_new_name_for(b"FT", kind);
        d.set_new_for(b"T", CpdfString::from_unicode(None, &wide(name)));
        d
    }
    #[test]
    fn load_fields_with_referenced_names_retains_complete_source_case() {
        // CPDFInteractiveFormTest.LoadFieldsWithReferencedNames.
        let doc = document();
        let objects = doc.borrow().holder();
        let good = retain_object(CpdfString::from_bytes(None, b"good_string"));
        let bad_name = retain_object(crate::fpdfapi::parser::cpdf_name::CpdfName::new(
            None,
            b"bad_name",
        ));
        let bad_stream = retain_object(
            crate::fpdfapi::parser::cpdf_stream::CpdfStream::from_bytes(b"bad_stream"),
        );
        let mut fields = CpdfArray::new();
        let mut dictionaries = Vec::new();
        for object in [good, bad_name, bad_stream] {
            let n = objects.borrow_mut().add_indirect_object(object);
            let mut d = widget("", b"Btn");
            d.set_new_for(b"T", CpdfReference::new(&objects, n));
            dictionaries.push(fields.append_new(d));
        }
        add_fields(&doc, fields);
        let _form = CpdfInteractiveForm::new(&doc);
        for (d, expected) in dictionaries
            .iter()
            .zip([b"good_string".as_slice(), b"", b""])
        {
            let value = dict_get(d, b"T").unwrap();
            assert_eq!(value.borrow().object_type(), ObjectType::String);
            assert_eq!(value.borrow().get_string(), expected);
        }
    }
    struct Notifier {
        form: Weak<CpdfInteractiveForm>,
        events: RefCell<Vec<&'static str>>,
        veto: std::cell::Cell<bool>,
    }
    impl FormNotifier for Notifier {
        fn before_value_change(&self, field: &CpdfFormField, _value: &[u32]) -> bool {
            self.events.borrow_mut().push("before");
            assert!(self.form.upgrade().unwrap().count_fields(&[]) > 0);
            assert!(!field.full_name().is_empty());
            !self.veto.get()
        }
        fn after_value_change(&self, _field: &CpdfFormField) {
            self.events.borrow_mut().push("after");
        }
        fn before_selection_change(&self, _field: &CpdfFormField, _value: &[u32]) -> bool {
            true
        }
        fn after_selection_change(&self, _field: &CpdfFormField) {}
        fn after_checked_status_change(&self, _field: &CpdfFormField) {
            self.events.borrow_mut().push("checked");
        }
        fn after_form_reset(&self, _form: &CpdfInteractiveForm) {
            self.events.borrow_mut().push("reset");
        }
    }
    #[test]
    fn notification_veto_reentry_reset_fdf_and_weak_ownership() {
        let doc = document();
        let mut d = widget("parent.text", b"Tx");
        d.set_new_for(b"DV", CpdfString::from_unicode(None, &wide("default")));
        let mut fields = CpdfArray::new();
        fields.append_new(d);
        add_fields(&doc, fields);
        let form = CpdfInteractiveForm::new(&doc);
        assert_eq!(form.count_fields(&wide("parent")), 1);
        let field = form.get_field(0, &[]).unwrap();
        let notifier = Rc::new(Notifier {
            form: Rc::downgrade(&form),
            events: RefCell::new(Vec::new()),
            veto: std::cell::Cell::new(false),
        });
        let iface: Rc<dyn FormNotifier> = notifier.clone();
        form.set_notifier_iface(Some(&iface));
        assert!(field.set_value(&wide("edited"), NotificationOption::Notify));
        assert_eq!(field.value(), wide("edited"));
        notifier.veto.set(true);
        assert!(!field.set_value(&wide("vetoed"), NotificationOption::Notify));
        assert_eq!(field.value(), wide("edited"));
        notifier.veto.set(false);
        form.reset_form();
        assert_eq!(field.value(), wide("default"));
        assert_eq!(
            &*notifier.events.borrow(),
            &["before", "after", "before", "before", "after", "reset"]
        );
        let exported = form.export_to_fdf(&wide("/tmp/form.pdf"));
        let bytes = exported.write_to_string();
        let parsed = CfdfDocument::parse_memory(&bytes).unwrap();
        let root = parsed.root().unwrap();
        let main = typed(dict_get(&root, b"FDF"), ObjectType::Dictionary).unwrap();
        let fields = typed(dict_get(&main, b"Fields"), ObjectType::Array).unwrap();
        assert_eq!(array_len(&fields), 1);
        let value = array_at(&fields, 0).unwrap();
        assert_eq!(dict_unicode(&value, b"T"), wide("parent.text"));
        assert_eq!(dict_unicode(&value, b"V"), wide("default"));
        let weak = Rc::downgrade(&form);
        drop(form);
        assert!(weak.upgrade().is_none());
        assert_eq!(field.value(), wide("default"));
    }
    #[test]
    fn direct_nested_fields_radio_controls_and_multiselect_edits() {
        let doc = document();
        let mut fields = CpdfArray::new();
        let mut parent = CpdfDictionary::new();
        parent.set_new_for(b"T", CpdfString::from_unicode(None, &wide("group")));
        parent.set_new_name_for(b"FT", b"Tx");
        let mut children = CpdfArray::new();
        let mut child = widget("child", b"Tx");
        child.set_new_for(b"V", CpdfString::from_unicode(None, &wide("value")));
        children.append_new(child);
        parent.set_new_for(b"Kids", children);
        fields.append_new(parent);
        let mut radio = widget("radio", b"Btn");
        radio.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(1 << 15));
        let mut controls = CpdfArray::new();
        for name in [b"A".as_slice(), b"B"] {
            let mut control = CpdfDictionary::new();
            control.set_new_name_for(b"Subtype", b"Widget");
            let mut ap = CpdfDictionary::new();
            let mut normal = CpdfDictionary::new();
            normal.set_new_name_for(name, b"stream");
            normal.set_new_name_for(b"Off", b"stream");
            ap.set_new_for(b"N", normal);
            control.set_new_for(b"AP", ap);
            control.set_new_name_for(b"AS", b"Off");
            controls.append_new(control);
        }
        radio.set_new_for(b"Kids", controls);
        fields.append_new(radio);
        let mut choice = widget("list", b"Ch");
        choice.set_new_for(b"Ff", CpdfScalarObject::number_from_signed(1 << 21));
        let mut options = CpdfArray::new();
        for value in ["one", "two", "three"] {
            options.append_new(CpdfString::from_unicode(None, &wide(value)));
        }
        choice.set_new_for(b"Opt", options);
        fields.append_new(choice);
        add_fields(&doc, fields);
        let form = CpdfInteractiveForm::new(&doc);
        assert_eq!(form.count_fields(&[]), 3);
        let radio = form.get_field(0, &wide("radio")).unwrap();
        assert_eq!(radio.count_controls(), 2);
        radio.check_control(1, true, NotificationOption::DoNotNotify);
        assert_eq!(radio.value(), wide("B"));
        radio.check_control(0, true, NotificationOption::DoNotNotify);
        assert_eq!(radio.value(), wide("A"));
        assert!(!radio.get_control(1).unwrap().is_checked());
        let list = form.get_field(0, &wide("list")).unwrap();
        list.set_item_selection(2, NotificationOption::DoNotNotify);
        list.set_item_selection(0, NotificationOption::DoNotNotify);
        assert!(list.is_item_selected(0));
        assert!(list.is_item_selected(2));
        assert!(!list.is_item_selected(1));
        assert_eq!(list.selected_index(0), 0);
        assert_eq!(list.selected_index(1), 2);
    }
}

#[cfg(test)]
#[path = "cpdf_interactiveform_font_resource_tests.rs"]
mod font_resource_tests;
