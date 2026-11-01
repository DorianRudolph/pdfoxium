// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Form-dependent annotation APIs from `fpdfsdk/fpdf_annot.cpp`.
//! Caller-owned annotations borrow their live pages. Snapshot engine owners
//! before callbacks and keep raw pointer/buffer handling at this ABI boundary.
use super::unimplemented_abi::FS_POINTF;
use super::*;
use pdfoxium_core::fpdfapi::parser::cpdf_object::ObjectRef;
use pdfoxium_core::fpdfdoc::cpdf_aaction::AActionType;
use pdfoxium_core::fpdfdoc::cpdf_defaultappearance::CpdfDefaultAppearance;
use pdfoxium_core::fpdfdoc::cpdf_formfield::{CpdfFormField, FormFieldType};
use pdfoxium_core::fpdfdoc::cpdf_generateap::generate_default_appearance_with_color_for_object;
use pdfoxium_core::fpdfsdk::cpdfsdk_widget::CpdfSdkWidget;
use pdfoxium_core::fxcrt::fx_string::utf16_encode;
use pdfoxium_core::fxge::cfx_color::CfxColor;

unsafe fn annotation_dict(annotation: *mut c_void) -> Option<ObjectRef> {
    Some(
        unsafe { annotation.cast::<annot::AnnotHandle>().as_ref()? }
            .context
            .as_ref()?
            .dict()
            .clone(),
    )
}
unsafe fn form_field(handle: *mut c_void, annotation: *mut c_void) -> Option<Rc<CpdfFormField>> {
    let dict = unsafe { annotation_dict(annotation) }?;
    let env = unsafe { formfill::form_environment(handle) }?;
    env.interactive_form().core_form().get_field_by_dict(&dict)
}
unsafe fn widget(
    handle: *mut c_void,
    annotation: *mut c_void,
    buttons_only: bool,
) -> Option<Rc<CpdfSdkWidget>> {
    let dict = unsafe { annotation_dict(annotation) }?;
    unsafe {
        formfill::with_form_environment(handle, |env| {
            let form = env.interactive_form();
            let core = form.core_form();
            let field = core.get_field_by_dict(&dict)?;
            if buttons_only
                && !matches!(
                    field.field_type(),
                    FormFieldType::CheckBox | FormFieldType::RadioButton
                )
            {
                return None;
            }
            let control = core.get_control_by_dict(&dict)?;
            form.get_widget(&control)
        })
    }?
}
unsafe fn copy_wide(text: &[u32], buffer: *mut u16, capacity: c_ulong) -> c_ulong {
    let words: Vec<u16> = utf16_encode(text).into_iter().chain(Some(0)).collect();
    let bytes = words.len() * 2;
    if !buffer.is_null() && capacity as usize >= bytes {
        // Write bytes so the public UTF-16LE contract also holds on big endian
        // hosts and callers need not provide a naturally aligned byte buffer.
        for (index, word) in words.iter().enumerate() {
            unsafe {
                std::ptr::copy_nonoverlapping(
                    word.to_le_bytes().as_ptr(),
                    buffer.cast::<u8>().add(index * 2),
                    2,
                );
            }
        }
    }
    bytes as c_ulong
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormFieldFlags(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    unsafe { form_field(handle, annotation) }.map_or(0, |field| field.field_flags() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetFormFieldFlags(
    handle: *mut c_void,
    annotation: *mut c_void,
    flags: c_int,
) -> c_int {
    let Some(field) = (unsafe { form_field(handle, annotation) }) else {
        return 0;
    };
    field.set_field_flags(flags as u32);
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormFieldAtPoint(
    handle: *mut c_void,
    page: *mut c_void,
    point: *const FS_POINTF,
) -> *mut c_void {
    let Some(point) = (unsafe { point.as_ref() }) else {
        return std::ptr::null_mut();
    };
    let point = CfxPointF::new(point.x, point.y);
    let Some(env) = (unsafe { formfill::form_environment(handle) }) else {
        return std::ptr::null_mut();
    };
    let Some(page_handle) = (unsafe { pdf_handle(page) }) else {
        return std::ptr::null_mut();
    };
    let index = env
        .interactive_form()
        .core_form()
        .get_control_at_point(page_handle.pdf_page().unwrap(), point)
        .map(|(_, index)| index);
    index.map_or(std::ptr::null_mut(), |index| unsafe {
        FPDFPage_GetAnnot(page, index)
    })
}
macro_rules! field_text {
    ($name:ident,$method:ident) => {
        #[no_mangle]
        pub unsafe extern "C" fn $name(
            handle: *mut c_void,
            annotation: *mut c_void,
            buffer: *mut u16,
            capacity: c_ulong,
        ) -> c_ulong {
            let Some(field) = (unsafe { form_field(handle, annotation) }) else {
                return 0;
            };
            unsafe { copy_wide(&field.$method(), buffer, capacity) }
        }
    };
}
field_text!(FPDFAnnot_GetFormFieldName, full_name);
field_text!(FPDFAnnot_GetFormFieldAlternateName, alternate_name);
field_text!(FPDFAnnot_GetFormFieldValue, value);
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormFieldType(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    #[cfg(feature = "xfa")]
    if let Some(widget) = (unsafe { annotation.cast::<annot::AnnotHandle>().as_ref() })
        .and_then(annot::AnnotHandle::xfa_widget)
    {
        let Some(env) = (unsafe { formfill::form_environment(handle) }) else {
            return -1;
        };
        if !widget
            .page_view()
            .and_then(|view| view.environment())
            .is_some_and(|owner| Rc::ptr_eq(&owner, &env))
        {
            return -1;
        }
        return widget
            .ff_widget()
            .as_ff_widget()
            .unwrap()
            .get_form_field_type() as c_int;
    }

    unsafe { form_field(handle, annotation) }.map_or(-1, |field| field.field_type() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormAdditionalActionJavaScript(
    handle: *mut c_void,
    annotation: *mut c_void,
    event: c_int,
    buffer: *mut u16,
    capacity: c_ulong,
) -> c_ulong {
    let Some(field) = (unsafe { form_field(handle, annotation) }) else {
        return 0;
    };
    let kind = match event {
        12 => AActionType::KeyStroke,
        13 => AActionType::Format,
        14 => AActionType::Validate,
        15 => AActionType::Calculate,
        _ => return 0,
    };
    unsafe {
        copy_wide(
            &field.additional_action().action(kind).javascript(),
            buffer,
            capacity,
        )
    }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetOptionCount(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    unsafe { form_field(handle, annotation) }
        .filter(|field| field.has_opt_field())
        .map_or(-1, |field| field.count_options() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetOptionLabel(
    handle: *mut c_void,
    annotation: *mut c_void,
    index: c_int,
    buffer: *mut u16,
    capacity: c_ulong,
) -> c_ulong {
    if index < 0 {
        return 0;
    }
    let Some(field) = (unsafe { form_field(handle, annotation) }) else {
        return 0;
    };
    if !field.has_opt_field() || index as usize >= field.count_options() {
        return 0;
    }
    unsafe { copy_wide(&field.option_label(index), buffer, capacity) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_IsOptionSelected(
    handle: *mut c_void,
    annotation: *mut c_void,
    index: c_int,
) -> c_int {
    if index < 0 {
        return 0;
    }
    let Some(field) = (unsafe { form_field(handle, annotation) }) else {
        return 0;
    };
    (matches!(
        field.field_type(),
        FormFieldType::ComboBox | FormFieldType::ListBox
    ) && (index as usize) < field.count_options()
        && field.is_item_selected(index)) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFontSize(
    handle: *mut c_void,
    annotation: *mut c_void,
    value: *mut f32,
) -> c_int {
    if value.is_null() {
        return 0;
    }
    let Some(widget) = (unsafe { widget(handle, annotation, false) }) else {
        return 0;
    };
    unsafe {
        *value = widget.font_size();
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_SetFontColor(
    handle: *mut c_void,
    annotation: *mut c_void,
    red: u32,
    green: u32,
    blue: u32,
) -> c_int {
    if red > 255 || green > 255 || blue > 255 {
        return 0;
    }
    let Some(dict) = (unsafe { annotation_dict(annotation) }) else {
        return 0;
    };
    if !dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .is_some_and(|dict| dict.get_name_for(b"Subtype") == b"FreeText")
    {
        return 0;
    }
    let Some(env) = (unsafe { formfill::form_environment(handle) }) else {
        return 0;
    };
    let Some(document) = env.document() else {
        return 0;
    };
    let (objects, root, data) = {
        let document = document.borrow();
        (document.holder(), document.root(), document.page_data_ref())
    };
    if !generate_default_appearance_with_color_for_object(
        &objects,
        root.as_ref(),
        &data,
        &dict,
        CfxColor::make_rgb(
            red as f32 / 255.0,
            green as f32 / 255.0,
            blue as f32 / 255.0,
        ),
    ) {
        return 0;
    }
    // The source removes the stale AP until the generator updates its stream.
    dict.borrow_mut()
        .as_any_mut()
        .downcast_mut::<CpdfDictionary>()
        .unwrap()
        .remove_for(b"AP");
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFontColor(
    handle: *mut c_void,
    annotation: *mut c_void,
    red: *mut u32,
    green: *mut u32,
    blue: *mut u32,
) -> c_int {
    if red.is_null() || green.is_null() || blue.is_null() {
        return 0;
    }
    let color = match unsafe { FPDFAnnot_GetSubtype(annotation) } {
        3 => {
            let Some(dict) = (unsafe { annotation_dict(annotation) }) else {
                return 0;
            };
            let root = unsafe { formfill::form_environment(handle) }
                .and_then(|env| env.document())
                .and_then(|document| document.borrow().root());
            let form = root.and_then(|root| {
                root.borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .and_then(|dict| dict.get_dict_for(b"AcroForm"))
            });
            let borrowed = dict.borrow();
            let Some(annot) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
                return 0;
            };
            let form = form.as_ref().map(|form| form.borrow());
            let form = form
                .as_ref()
                .and_then(|form| form.as_any().downcast_ref::<CpdfDictionary>());
            CpdfDefaultAppearance::from_dicts(Some(annot), form)
                .color_argb()
                .map(|color| {
                    let argb = color.argb;
                    ((argb >> 16) & 255) | (((argb >> 8) & 255) << 8) | ((argb & 255) << 16)
                })
        }
        20 => unsafe { widget(handle, annotation, false) }.and_then(|widget| widget.text_color()),
        _ => return 0,
    };
    let Some(color) = color else {
        return 0;
    };
    unsafe {
        *red = color & 255;
        *green = (color >> 8) & 255;
        *blue = (color >> 16) & 255;
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_IsChecked(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    unsafe { widget(handle, annotation, true) }.is_some_and(|widget| widget.is_checked()) as c_int
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormControlCount(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    unsafe { form_field(handle, annotation) }.map_or(-1, |field| field.count_controls() as c_int)
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormControlIndex(
    handle: *mut c_void,
    annotation: *mut c_void,
) -> c_int {
    let Some(dict) = (unsafe { annotation_dict(annotation) }) else {
        return -1;
    };
    let Some(env) = (unsafe { formfill::form_environment(handle) }) else {
        return -1;
    };
    let core = env.interactive_form().core_form();
    let Some(field) = core.get_field_by_dict(&dict) else {
        return -1;
    };
    core.get_control_by_dict(&dict)
        .map_or(-1, |control| field.control_index(&control))
}
#[no_mangle]
pub unsafe extern "C" fn FPDFAnnot_GetFormFieldExportValue(
    handle: *mut c_void,
    annotation: *mut c_void,
    buffer: *mut u16,
    capacity: c_ulong,
) -> c_ulong {
    let Some(widget) = (unsafe { widget(handle, annotation, true) }) else {
        return 0;
    };
    unsafe { copy_wide(&widget.export_value(), buffer, capacity) }
}

#[cfg(test)]
mod tests {
    use super::super::unimplemented_abi::FPDF_FORMFILLINFO;
    use super::*;
    use std::ptr::{null, null_mut};
    struct Session {
        document: *mut c_void,
        page: *mut c_void,
        form: *mut c_void,
        _info: Box<FPDF_FORMFILLINFO>,
    }
    impl Session {
        fn new(bytes: &[u8]) -> Self {
            FPDF_InitLibrary();
            let document =
                unsafe { FPDF_LoadMemDocument64(bytes.as_ptr().cast(), bytes.len(), null()) };
            assert!(!document.is_null());
            let page = unsafe { FPDF_LoadPage(document, 0) };
            assert!(!page.is_null());
            let mut info: Box<FPDF_FORMFILLINFO> = Box::new(unsafe { std::mem::zeroed() });
            info.version = 2;
            info.xfa_disabled = 1;
            let form = unsafe { FPDFDOC_InitFormFillEnvironment(document, &mut *info) };
            assert!(!form.is_null());
            unsafe {
                FORM_OnAfterLoadPage(page, form);
            }
            Self {
                document,
                page,
                form,
                _info: info,
            }
        }
        fn annot(&self, index: i32) -> *mut c_void {
            let result = unsafe { FPDFPage_GetAnnot(self.page, index) };
            assert!(!result.is_null());
            result
        }
    }
    impl Drop for Session {
        fn drop(&mut self) {
            unsafe {
                FORM_OnBeforeClosePage(self.page, self.form);
                FPDF_ClosePage(self.page);
                FPDFDOC_ExitFormFillEnvironment(self.form);
                FPDF_CloseDocument(self.document);
                FPDF_DestroyLibrary();
            }
        }
    }
    fn text(mut get: impl FnMut(*mut u16, c_ulong) -> c_ulong, expected_bytes: c_ulong) -> String {
        assert_eq!(get(null_mut(), 0), expected_bytes);
        let mut words = vec![0xcccc; expected_bytes as usize / 2 + 1];
        assert_eq!(get(words.as_mut_ptr(), expected_bytes - 1), expected_bytes);
        assert!(words.iter().all(|&word| word == 0xcccc));
        assert_eq!(get(words.as_mut_ptr(), expected_bytes), expected_bytes);
        assert_eq!(words[expected_bytes as usize / 2 - 1], 0);
        assert_eq!(*words.last().unwrap(), 0xcccc);
        String::from_utf16(&words[..expected_bytes as usize / 2 - 1]).unwrap()
    }
    #[test]
    fn source_option_label_combo_and_list_cases_preserve_bounds_and_utf16_lengths() {
        // Complete GetOptionLabelCombobox/GetOptionLabelListbox inputs and
        // assertions from fpdf_annot_embeddertest.cpp, plus short-buffer guards.
        for bytes in [
            &include_bytes!("../../../upstream/pdfium/testing/resources/combobox_form.pdf")[..],
            &include_bytes!("../../../upstream/pdfium/testing/resources/listbox_form.pdf")[..],
        ] {
            let session = Session::new(bytes);
            for (annotation, index, length, label) in [
                (0, 0, 8, "Foo"),
                (1, 0, 12, "Apple"),
                (1, 25, 18, "Zucchini"),
            ] {
                let annot = session.annot(annotation);
                assert_eq!(
                    text(
                        |buffer, capacity| unsafe {
                            FPDFAnnot_GetOptionLabel(session.form, annot, index, buffer, capacity)
                        },
                        length
                    ),
                    label
                );
                unsafe {
                    FPDFPage_CloseAnnot(annot);
                }
            }
            let annot = session.annot(1);
            assert_eq!(
                unsafe { FPDFAnnot_GetOptionLabel(session.form, annot, -1, null_mut(), 0) },
                0
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetOptionLabel(session.form, annot, 26, null_mut(), 0) },
                0
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetOptionLabel(null_mut(), null_mut(), 0, null_mut(), 0) },
                0
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetOptionLabel(null_mut(), annot, 0, null_mut(), 0) },
                0
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetOptionLabel(session.form, null_mut(), 0, null_mut(), 0) },
                0
            );
            unsafe {
                FPDFPage_CloseAnnot(annot);
            }
        }
    }
    #[test]
    fn source_form_field_names_and_annotation_javascript_are_queries_without_execution() {
        for (bytes, length, name) in [
            (
                &include_bytes!("../../../upstream/pdfium/testing/resources/text_form.pdf")[..],
                18,
                "Text Box",
            ),
            (
                &include_bytes!("../../../upstream/pdfium/testing/resources/combobox_form.pdf")[..],
                30,
                "Combo_Editable",
            ),
        ] {
            let session = Session::new(bytes);
            let annot = session.annot(0);
            assert_eq!(
                text(
                    |buffer, capacity| unsafe {
                        FPDFAnnot_GetFormFieldName(session.form, annot, buffer, capacity)
                    },
                    length
                ),
                name
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetFormFieldName(null_mut(), annot, null_mut(), 0) },
                0
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetFormFieldName(session.form, null_mut(), null_mut(), 0) },
                0
            );
            unsafe {
                FPDFPage_CloseAnnot(annot);
            }
        }
        let session = Session::new(include_bytes!(
            "../../../upstream/pdfium/testing/resources/annot_javascript.pdf"
        ));
        let annot = session.annot(0);
        assert_eq!(
            text(
                |buffer, capacity| unsafe {
                    FPDFAnnot_GetFormAdditionalActionJavaScript(
                        session.form,
                        annot,
                        13,
                        buffer,
                        capacity,
                    )
                },
                62
            ),
            "AFDate_FormatEx(\"yyyy-mm-dd\");"
        );
        assert_eq!(
            unsafe {
                FPDFAnnot_GetFormAdditionalActionJavaScript(
                    session.form,
                    null_mut(),
                    0,
                    null_mut(),
                    0,
                )
            },
            0
        );
        assert_eq!(
            unsafe {
                FPDFAnnot_GetFormAdditionalActionJavaScript(null_mut(), annot, 0, null_mut(), 0)
            },
            0
        );
        assert_eq!(
            unsafe {
                FPDFAnnot_GetFormAdditionalActionJavaScript(session.form, annot, 0, null_mut(), 0)
            },
            0
        );
        assert_eq!(
            unsafe {
                FPDFAnnot_GetFormAdditionalActionJavaScript(session.form, annot, 12, null_mut(), 0)
            },
            2
        );
        unsafe {
            FPDFPage_CloseAnnot(annot);
        }
    }
    #[test]
    fn field_flags_control_identity_checked_states_and_hit_testing_use_owned_models() {
        let session = Session::new(include_bytes!(
            "../../../upstream/pdfium/testing/resources/click_form.pdf"
        ));
        for (index, checked) in [
            (0, true),
            (1, false),
            (2, false),
            (3, false),
            (4, true),
            (5, false),
            (6, false),
            (7, true),
        ] {
            let annot = session.annot(index);
            assert_eq!(
                unsafe { FPDFAnnot_IsChecked(session.form, annot) } != 0,
                checked
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetFormControlCount(session.form, annot) },
                if index < 2 { 1 } else { 3 }
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetFormControlIndex(session.form, annot) },
                if index < 2 { 0 } else { (index - 2) % 3 }
            );
            assert!(unsafe { FPDFAnnot_GetFormFieldType(session.form, annot) } > 0);
            let flags = unsafe { FPDFAnnot_GetFormFieldFlags(session.form, annot) };
            assert_eq!(
                unsafe { FPDFAnnot_SetFormFieldFlags(session.form, annot, flags ^ 1) },
                1
            );
            assert_eq!(
                unsafe { FPDFAnnot_GetFormFieldFlags(session.form, annot) },
                flags ^ 1
            );
            assert_eq!(
                unsafe { FPDFAnnot_SetFormFieldFlags(session.form, annot, flags) },
                1
            );
            let mut rect = FS_RECTF {
                left: 0.0,
                top: 0.0,
                right: 0.0,
                bottom: 0.0,
            };
            assert_eq!(unsafe { FPDFAnnot_GetRect(annot, &mut rect) }, 1);
            let point = FS_POINTF {
                x: (rect.left + rect.right) / 2.0,
                y: (rect.bottom + rect.top) / 2.0,
            };
            let hit = unsafe { FPDFAnnot_GetFormFieldAtPoint(session.form, session.page, &point) };
            assert!(!hit.is_null());
            assert_eq!(unsafe { FPDFPage_GetAnnotIndex(session.page, hit) }, index);
            unsafe {
                FPDFPage_CloseAnnot(hit);
                FPDFPage_CloseAnnot(annot);
            }
        }
        assert!(
            unsafe { FPDFAnnot_GetFormFieldAtPoint(session.form, session.page, null()) }.is_null()
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFormControlCount(session.form, null_mut()) },
            -1
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFormControlIndex(null_mut(), null_mut()) },
            -1
        );
    }
    #[test]
    fn source_widget_font_color_and_freetext_color_mutation_keep_appearance_contract() {
        {
            let session = Session::new(include_bytes!(
                "../../../upstream/pdfium/testing/resources/text_form_color.pdf"
            ));
            let annot = session.annot(0);
            let (mut red, mut green, mut blue) = (0, 0, 0);
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                    )
                },
                0
            );
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(
                        session.form,
                        null_mut(),
                        null_mut(),
                        null_mut(),
                        null_mut(),
                    )
                },
                0
            );
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(session.form, annot, null_mut(), null_mut(), null_mut())
                },
                0
            );
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(session.form, annot, &mut red, null_mut(), null_mut())
                },
                0
            );
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(session.form, annot, &mut red, &mut green, null_mut())
                },
                0
            );
            assert_eq!(
                unsafe {
                    FPDFAnnot_GetFontColor(session.form, annot, &mut red, &mut green, &mut blue)
                },
                1
            );
            assert_eq!((red, green, blue), (255, 128, 0));
            assert_eq!(
                unsafe { FPDFAnnot_SetFontColor(session.form, annot, 60, 120, 180) },
                0
            );
            unsafe {
                FPDFPage_CloseAnnot(annot);
            }
        }
        let session = Session::new(include_bytes!(
            "../../../upstream/pdfium/testing/resources/freetext_annotation_without_da.pdf"
        ));
        let annot = session.annot(0);
        let (mut red, mut green, mut blue) = (1, 2, 3);
        assert_eq!(
            unsafe { FPDFAnnot_GetFontColor(session.form, annot, &mut red, &mut green, &mut blue) },
            1
        );
        assert_eq!((red, green, blue), (0, 0, 0));
        assert_eq!(
            unsafe { FPDFAnnot_SetFontColor(session.form, annot, 256, 120, 180) },
            0
        );
        assert_eq!(
            unsafe { FPDFAnnot_SetFontColor(session.form, annot, 60, 120, 180) },
            1
        );
        assert_eq!(
            unsafe { FPDFAnnot_GetFontColor(session.form, annot, &mut red, &mut green, &mut blue) },
            1
        );
        assert_eq!((red, green, blue), (60, 120, 180));
        assert!(!unsafe { annotation_dict(annot) }
            .unwrap()
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .key_exist(b"AP"));
        unsafe {
            FPDFPage_CloseAnnot(annot);
        }
    }
}
