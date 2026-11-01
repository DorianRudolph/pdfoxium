// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Annotation construction, popup dictionaries and static display from
//! `core/fpdfdoc/cpdf_annotlist.{h,cpp}`. Popups stay closed until an SDK hover
//! opens their shared state; their transient dictionaries are never serialized.

use super::cpdf_annot::{AppearanceMode, CpdfAnnot, Subtype};
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
use crate::fpdfapi::parser::fpdf_parser_decode::pdf_decode_text;
use crate::fpdfapi::render::cpdf_rendercontext::CpdfRenderContext;
use crate::fxcrt::fx_coordinates::CfxFloatRect;
use crate::fxcrt::fx_coordinates::CfxMatrix;
use std::rc::Rc;

pub struct CpdfAnnotList<'p> {
    page: &'p CpdfPage,
    annot_list: Vec<CpdfAnnot>,
}
impl<'p> CpdfAnnotList<'p> {
    pub fn into_annotations(self) -> Vec<CpdfAnnot> {
        self.annot_list
    }
    pub fn iter(&self) -> impl Iterator<Item = &CpdfAnnot> {
        self.annot_list.iter()
    }
    pub fn new(page: &'p CpdfPage) -> Self {
        Self::new_with_widget_ap(page, true)
    }
    /// SDK loading disables the default widget generator while constructing
    /// its own wrappers. Keep that policy local instead of temporarily mutating
    /// the process-wide AP setting during callbacks or parallel page loading.
    pub fn new_for_sdk(page: &'p CpdfPage) -> Self {
        Self::new_with_widget_ap(page, false)
    }
    pub fn into_annots(self) -> Vec<CpdfAnnot> {
        self.annot_list
    }
    fn new_with_widget_ap(page: &'p CpdfPage, widget_ap: bool) -> Self {
        let mut list = Self {
            page,
            annot_list: Vec::new(),
        };
        let Some(annots) = page.annots_array() else {
            return list;
        };
        let data = page.page_data_ref();
        let objects = data.document_objects();
        let regenerate = data
            .document_root()
            .and_then(|root| super::cpdf_formfield::dict_get(&root, b"AcroForm"))
            .is_some_and(|form| {
                form.borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .is_some_and(|d| d.get_boolean_for(b"NeedAppearances", false))
            });
        let mut i = 0;
        loop {
            // Release the array borrow before constructing an annotation:
            // generation can re-enter the document's object/resource tables.
            let dict = {
                let borrowed = annots.borrow();
                let annots = borrowed
                    .as_any()
                    .downcast_ref::<CpdfArray>()
                    .expect("Annots array");
                if i >= annots.size() {
                    break;
                }
                annots.get_direct_object_at(i)
            };
            if let Some(dict) = dict {
                let included = {
                    let borrowed = dict.borrow();
                    borrowed
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .is_some_and(|dict| {
                            !crate::fxcrt::bytestring::eq_cstr(
                                &dict.get_byte_string_for(b"Subtype", b""),
                                b"Popup",
                            )
                        })
                };
                if included {
                    // Detached Rust test pages have no native document; real
                    // document pages always carry this borrowed object table.
                    if let Some(objects) = &objects {
                        annots
                            .borrow_mut()
                            .as_any_mut()
                            .downcast_mut::<CpdfArray>()
                            .expect("Annots array")
                            .convert_to_indirect_object_at(i, objects);
                    }
                    let annot = CpdfAnnot::new(Rc::clone(&dict), &data);
                    if widget_ap
                        && regenerate
                        && annot.subtype() == Subtype::Widget
                        && super::cpdf_interactiveform::CpdfInteractiveForm::is_update_ap_enabled()
                        && super::cpdf_formfield::dict_get(&dict, b"AP")
                            .and_then(|ap| {
                                crate::fpdfapi::parser::cpdf_object::get_dict_object(&ap)
                            })
                            .is_none()
                    {
                        generate_widget_ap(page, &dict);
                    }
                    list.annot_list.push(annot);
                }
            }
            i += 1;
        }
        let count = list.annot_list.len();
        for index in 0..count {
            if let Some(popup) = create_popup_annot(page, &list.annot_list[index]) {
                list.annot_list[index].set_popup_annot(&popup);
                list.annot_list.push(popup);
            }
        }
        list
    }

    /// DisplayPass(..., bWidgetPass=false). The static bitmap render caller
    /// passes bShowWidget=false; form rendering has its own deferred entry point.
    pub fn display_annots<'a>(
        &'a mut self,
        context: &mut CpdfRenderContext<'a>,
        printing: bool,
        page_to_device: CfxMatrix,
    ) {
        Self::display_owned_annots(
            self.page,
            &mut self.annot_list,
            context,
            printing,
            page_to_device,
        );
    }
    /// Progressive owners retain annotations and parsed AP forms between calls.
    /// Only these short-lived layer views borrow the page and appearance owners.
    pub fn display_owned_annots<'a>(
        page: &'a CpdfPage,
        annotations: &'a mut [CpdfAnnot],
        context: &mut CpdfRenderContext<'a>,
        printing: bool,
        page_to_device: CfxMatrix,
    ) {
        Self::display_owned_annots_with_options(
            page,
            annotations,
            context,
            printing,
            page_to_device,
            None,
        );
    }
    pub fn display_owned_annots_with_options<'a>(
        page: &'a CpdfPage,
        annotations: &'a mut [CpdfAnnot],
        context: &mut CpdfRenderContext<'a>,
        printing: bool,
        page_to_device: CfxMatrix,
        options: Option<&crate::fpdfapi::render::cpdf_renderoptions::CpdfRenderOptions>,
    ) {
        for annot in annotations {
            let oc = annot
                .dict()
                .borrow()
                .as_dictionary()
                .and_then(|d| d.get_dict_for(b"OC"));
            if options.is_some_and(|o| !o.check_ocg_dict_visible(oc.as_ref())) {
                continue;
            }
            if annot.subtype() == Subtype::Widget {
                continue;
            }
            let flags = annot.flags();
            if flags & 2 != 0 {
                continue;
            }
            if printing && flags & 4 == 0 {
                continue;
            }
            if !printing && flags & 32 != 0 {
                continue;
            }
            annot.draw_in_context(page, context, page_to_device, AppearanceMode::Normal);
        }
    }
}

/// `CreatePopupAnnot` and `PopupAppearsForAnnotType` from the native file.
/// The weak document-resource edge is retained by CpdfAnnot; popup relationships
/// share only a tiny open-state cell and dictionary, with no parent cycle.
pub fn create_popup_annot(page: &CpdfPage, parent: &CpdfAnnot) -> Option<CpdfAnnot> {
    if !matches!(
        parent.subtype(),
        Subtype::Text
            | Subtype::Line
            | Subtype::Square
            | Subtype::Circle
            | Subtype::Polygon
            | Subtype::PolyLine
            | Subtype::Highlight
            | Subtype::Underline
            | Subtype::Squiggly
            | Subtype::StrikeOut
            | Subtype::Stamp
            | Subtype::Caret
            | Subtype::Ink
            | Subtype::FileAttachment
            | Subtype::Redact
    ) {
        return None;
    }
    let borrowed = parent.dict().borrow();
    let original = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
    let contents = original.get_byte_string_for(b"Contents", b"");
    if pdf_decode_text(&contents).is_empty() {
        return None;
    }
    let mut dict = CpdfDictionary::new();
    dict.set_new_name_for(b"Type", b"Annot");
    dict.set_new_name_for(b"Subtype", b"Popup");
    dict.set_new_string_for(b"T", &original.get_byte_string_for(b"T", b""));
    dict.set_new_string_for(b"Contents", &contents);
    let mut rect = original.get_rect_for(b"Rect");
    rect.normalize();
    let (x, y) = if rect.left + 200.0 > page.width() && rect.bottom - 200.0 < 0.0 {
        (rect.right - 200.0, rect.top)
    } else {
        (
            rect.left.min(page.width() - 200.0),
            (rect.bottom - 200.0).max(0.0),
        )
    };
    dict.set_rect_for(b"Rect", CfxFloatRect::new(x, y, x + 200.0, y + 200.0));
    dict.set_new_for(b"F", CpdfScalarObject::number_from_signed(0));
    Some(CpdfAnnot::new(retain_object(dict), &page.page_data_ref()))
}

fn generate_widget_ap(page: &CpdfPage, annot: &ObjectRef) {
    use super::cpdf_formfield::{dict_get, dict_mut, field_attr};
    use super::cpdf_generateap::{generate_form_ap_for_object, FormType};
    let Some(kind) = field_attr(annot, b"FT") else {
        return;
    };
    let kind = kind.borrow().get_string();
    let flags = field_attr(annot, b"Ff").map_or(0, |f| f.borrow().get_integer() as u32);
    let form_type = match kind.as_slice() {
        b"Tx" => Some(FormType::TextField),
        b"Ch" => Some(if flags & (1 << 17) != 0 {
            FormType::ComboBox
        } else {
            FormType::ListBox
        }),
        _ => None,
    };
    if let Some(form_type) = form_type {
        let data = page.page_data_ref();
        let Some(objects) = data.document_objects() else {
            return;
        };
        let root = data.document_root();
        generate_form_ap_for_object(&objects, root.as_ref(), &data, annot, form_type);
        return;
    }
    if kind != b"Btn" || flags & (1 << 16) != 0 || dict_get(annot, b"AS").is_some() {
        return;
    }
    let Some(parent) = dict_get(annot, b"Parent") else {
        return;
    };
    let Some(state) = dict_get(&parent, b"AS") else {
        return;
    };
    let state = state.borrow().get_string();
    dict_mut(annot, |dict| {
        dict.set_new_string_for(b"AS", &state);
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fxcrt::fx_coordinates::CfxFloatRect;
    use crate::fxge::agg::cfx_agg_devicedriver::CfxAggDeviceDriver;
    use crate::fxge::dib::cfx_dibitmap::CfxDibitmap;
    use crate::fxge::dib::fx_dib::FxdibFormat;
    use std::rc::Rc;

    fn annotation(subtype: &[u8], flags: i32) -> ObjectRef {
        let mut dict = CpdfDictionary::new();
        dict.set_new_name_for(b"Subtype", subtype);
        dict.set_new_for(b"F", CpdfScalarObject::number_from_signed(flags));
        dict.set_rect_for(b"Rect", CfxFloatRect::new(1.0, 1.0, 6.0, 6.0));
        if subtype == b"FreeText" {
            dict.set_rect_for(b"Rect", CfxFloatRect::new(10.0, 10.0, 20.0, 20.0));
            dict.set_new_for(b"Contents", CpdfString::from_bytes(None, b"A"));
        }
        let mut fill = CpdfArray::new();
        for color in [1, 0, 0] {
            fill.append_new(CpdfScalarObject::number_from_signed(color));
        }
        dict.set_new_for(b"IC", fill);
        retain_object(dict)
    }
    fn has_ap(dict: &ObjectRef) -> bool {
        dict.borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .key_exist(b"AP")
    }
    #[test]
    fn construction_generates_before_display_filtering_and_caches_forms() {
        let objects = CpdfIndirectObjectHolder::new();
        let root = retain_object(CpdfDictionary::new());
        let annots = [
            annotation(b"Square", 4),
            annotation(b"Square", 4 | 32),
            annotation(b"Square", 2),
            annotation(b"Popup", 4),
            annotation(b"FreeText", 4),
        ];
        let mut array = CpdfArray::new();
        for annot in &annots {
            array.append(Rc::clone(annot));
        }
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 20.0, 20.0));
        dict.set_new_for(b"Annots", array);
        let page = CpdfPage::new(retain_object(dict)).unwrap();
        page.page_data()
            .bind_document_resources(&objects, Some(&root));
        let mut list = CpdfAnnotList::new(&page);
        assert_eq!(list.annot_list.len(), 4);
        assert!(has_ap(&annots[0]));
        assert!(has_ap(&annots[1]));
        assert!(!has_ap(&annots[2]));
        assert!(!has_ap(&annots[3]));
        assert!(has_ap(&annots[4]));
        assert_eq!(annots[3].borrow().obj_num(), 0);
        assert_ne!(annots[2].borrow().obj_num(), 0);
        let mut bitmap = CfxDibitmap::create(20, 20, FxdibFormat::Bgra, 0).unwrap();
        bitmap.clear(0);
        {
            let mut context = CpdfRenderContext::new(&page);
            list.display_annots(&mut context, false, CfxMatrix::default());
            assert_eq!(context.layers().len(), 2);
            let mut driver = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            assert!(context.render(&mut driver, None, None, None));
        }
        assert_eq!(&bitmap.scanline(3).unwrap()[12..16], &[0, 0, 255, 255]);
        let parsed = page.page_data().parsed_page_count();
        {
            let mut context = CpdfRenderContext::new(&page);
            list.display_annots(&mut context, false, CfxMatrix::default());
            assert_eq!(context.layers().len(), 2);
        }
        assert_eq!(page.page_data().parsed_page_count(), parsed);
        {
            let mut context = CpdfRenderContext::new(&page);
            list.display_annots(&mut context, true, CfxMatrix::default());
            assert_eq!(context.layers().len(), 3);
        }
        // PageData's document links do not keep the object graph alive.
        let data = page.page_data_ref();
        drop(objects);
        drop(root);
        assert!(data.document_objects().is_none());
        assert!(data.document_root().is_none());
    }
    #[test]
    fn need_appearances_replaces_malformed_widget_ap() {
        use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
        use crate::fpdfdoc::cpdf_annot::get_annot_ap;
        use crate::fpdfdoc::cpdf_interactiveform::init_acro_form_dict;
        use crate::fxge::cfx_fontmapper::CfxFontMapper;

        let objects = CpdfIndirectObjectHolder::new();
        let root = retain_object(CpdfDictionary::new());
        let widget = annotation(b"Widget", 0);
        {
            let mut borrowed = widget.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_new_name_for(b"FT", b"Tx");
            dict.set_rect_for(b"Rect", CfxFloatRect::new(10.0, 20.0, 110.0, 60.0));
            dict.set_new_string_for(b"V", b"Regenerated");
            dict.set_new_for(b"AP", CpdfScalarObject::number_from_signed(7));
        }
        let mut array = CpdfArray::new();
        array.append(widget.clone());
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 200.0, 100.0));
        dict.set_new_for(b"Annots", array);
        let page = CpdfPage::new(retain_object(dict)).unwrap();
        let data = page.page_data_ref();
        data.bind_document_resources(&objects, Some(&root));
        let form = init_acro_form_dict(&objects, &root, &data, &mut CfxFontMapper::new());
        form.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"NeedAppearances", CpdfScalarObject::boolean(true));

        // SDK construction leaves widget generation to the SDK owner, while
        // ordinary construction must treat a non-dictionary AP as missing.
        let sdk_list = CpdfAnnotList::new_for_sdk(&page);
        assert_eq!(sdk_list.annot_list.len(), 1);
        assert!(super::super::cpdf_formfield::dict_get(&widget, b"AP")
            .unwrap()
            .borrow()
            .as_any()
            .is::<CpdfScalarObject>());
        let list = CpdfAnnotList::new(&page);
        assert_eq!(list.annot_list.len(), 1);
        let appearance = {
            let borrowed = widget.borrow();
            get_annot_ap(
                borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap(),
                AppearanceMode::Normal,
                false,
            )
            .expect("NeedAppearances creates a normal appearance stream")
        };
        let borrowed = appearance.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        assert!(stream
            .get_in_memory_raw_data()
            .windows(b"(Regenerated) Tj".len())
            .any(|part| part == b"(Regenerated) Tj"));
    }
    #[test]
    fn popup_is_transient_closed_until_hover_and_shares_open_state() {
        let parent = annotation(b"Square", 0);
        {
            let mut borrowed = parent.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_rect_for(b"Rect", CfxFloatRect::new(350.0, 10.0, 380.0, 40.0));
            dict.set_new_for(b"Contents", CpdfString::from_bytes(None, b"Comment"));
        }
        let mut array = CpdfArray::new();
        array.append(parent.clone());
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 400.0, 400.0));
        dict.set_new_for(b"Annots", array);
        let page = CpdfPage::new(retain_object(dict)).unwrap();
        let list = CpdfAnnotList::new_for_sdk(&page);
        assert_eq!(list.annot_list.len(), 2);
        let original = &list.annot_list[0];
        let popup = &list.annot_list[1];
        assert_eq!(popup.subtype(), Subtype::Popup);
        assert_eq!(popup.rect(), CfxFloatRect::new(180.0, 40.0, 380.0, 240.0));
        assert!(!popup.is_open());
        assert!(!has_ap(popup.dict()));
        assert_eq!(popup.dict().borrow().obj_num(), 0);
        original.set_popup_annot_open_state(true);
        assert!(popup.is_open());
        original.set_popup_annot_open_state(false);
        assert!(!popup.is_open());
        assert_eq!(
            page.annots_array()
                .unwrap()
                .borrow()
                .as_any()
                .downcast_ref::<CpdfArray>()
                .unwrap()
                .size(),
            1
        );
    }
    #[test]
    fn popup_requires_eligible_parent_and_nonempty_decoded_contents() {
        let mut dict = CpdfDictionary::new();
        dict.set_rect_for(b"MediaBox", CfxFloatRect::new(0.0, 0.0, 400.0, 400.0));
        let page = CpdfPage::new(retain_object(dict)).unwrap();
        let empty = annotation(b"Square", 0);
        let empty = CpdfAnnot::new(empty, &page.page_data_ref());
        assert!(create_popup_annot(&page, &empty).is_none());
        let link = annotation(b"Link", 0);
        link.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Contents", CpdfString::from_bytes(None, b"Comment"));
        assert!(create_popup_annot(&page, &CpdfAnnot::new(link, &page.page_data_ref())).is_none());
        let square = annotation(b"Square", 0);
        square
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"Contents", CpdfString::from_bytes(None, b"Comment"));
        let popup =
            create_popup_annot(&page, &CpdfAnnot::new(square, &page.page_data_ref())).unwrap();
        assert_eq!(popup.rect(), CfxFloatRect::new(1.0, 0.0, 201.0, 200.0));
    }
}

#[cfg(test)]
#[path = "cpdf_annotlist_native_tests.rs"]
mod native_popup_tests;
