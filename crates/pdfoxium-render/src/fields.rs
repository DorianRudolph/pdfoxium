// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{backend, prelude::*};
use pdfoxium_core::{
    fpdfapi::parser::cpdf_object::ObjectRef,
    fpdfdoc::{
        cpdf_formcontrol::CpdfFormControl,
        cpdf_formfield::{CpdfFormField, NotificationOption},
        cpdf_interactiveform::CpdfInteractiveForm,
    },
    fpdfsdk::fpdf_annot,
};
use std::{
    collections::HashMap,
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfAppearanceMode {
    Normal = 0,
    RollOver = 1,
    Down = 2,
}
#[derive(Clone)]
pub(crate) struct FieldData {
    page: backend::PdfPage,
    dict: ObjectRef,
    core: Rc<CpdfFormField>,
    control: Rc<CpdfFormControl>,
    _form: Rc<CpdfInteractiveForm>,
}
impl FieldData {
    fn value(&self) -> Option<String> {
        Some(backend::text::decode(&self.core.value()))
    }
    fn name(&self) -> Option<String> {
        Some(backend::text::decode(&self.core.full_name()))
    }
    fn check(&self) -> Result<(), PdfiumError> {
        self.page.check_live()?;
        if fpdf_annot::annotation_index(&self.page.state.core.borrow(), &self.dict).is_none() {
            Err(PdfiumError::OwnershipNotAttachedToPage)
        } else {
            Ok(())
        }
    }
    fn set_value(&self, value: &str) -> Result<(), PdfiumError> {
        self.check()?;
        let date = chrono::Utc::now()
            .format("D:%Y%m%d%H%M%SZ00'00'")
            .to_string();
        fpdf_annot::set_string(
            &self.dict,
            b"M",
            &date.chars().map(u32::from).collect::<Vec<_>>(),
        );
        crate::objects::boolean(self.core.set_value(
            &value.chars().map(u32::from).collect::<Vec<_>>(),
            NotificationOption::DoNotNotify,
        ))
    }
}
mod private {
    pub(crate) trait FieldOwner {
        fn field(&self) -> &super::FieldData;
    }
}
pub enum PdfFormField<'a> {
    PushButton(PdfFormPushButtonField<'a>),
    Checkbox(PdfFormCheckboxField<'a>),
    RadioButton(PdfFormRadioButtonField<'a>),
    ComboBox(PdfFormComboBoxField<'a>),
    ListBox(PdfFormListBoxField<'a>),
    Signature(PdfFormSignatureField<'a>),
    Text(PdfFormTextField<'a>),
    Unknown(PdfFormUnknownField<'a>),
}
pub struct PdfFormPushButtonField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormPushButtonField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormCheckboxField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormCheckboxField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormRadioButtonField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormRadioButtonField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormComboBoxField<'a> {
    data: FieldData,
    options: PdfFormFieldOptions<'a>,
}
impl private::FieldOwner for PdfFormComboBoxField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormListBoxField<'a> {
    data: FieldData,
    options: PdfFormFieldOptions<'a>,
}
impl private::FieldOwner for PdfFormListBoxField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormSignatureField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormSignatureField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormTextField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormTextField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
pub struct PdfFormUnknownField<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl private::FieldOwner for PdfFormUnknownField<'_> {
    fn field(&self) -> &FieldData {
        &self.data
    }
}
impl<'a> PdfFormField<'a> {
    pub(crate) fn from_annotation(page: backend::PdfPage, dict: ObjectRef) -> Option<Self> {
        let form = CpdfInteractiveForm::new(&page.document.core);
        form.fix_page_fields(&page.state.core.borrow());
        let control = form.get_control_by_dict(&dict)?;
        let core = control.field()?;
        let kind = core.field_type() as i32;
        let data = FieldData {
            page,
            dict,
            core,
            control,
            _form: form,
        };
        let options = PdfFormFieldOptions {
            data: data.clone(),
            lifetime: PhantomData,
        };
        Some(match kind {
            1 => Self::PushButton(PdfFormPushButtonField {
                data,
                lifetime: PhantomData,
            }),
            2 => Self::Checkbox(PdfFormCheckboxField {
                data,
                lifetime: PhantomData,
            }),
            3 => Self::RadioButton(PdfFormRadioButtonField {
                data,
                lifetime: PhantomData,
            }),
            4 => Self::ComboBox(PdfFormComboBoxField { data, options }),
            5 => Self::ListBox(PdfFormListBoxField { data, options }),
            7 => Self::Signature(PdfFormSignatureField {
                data,
                lifetime: PhantomData,
            }),
            6 => Self::Text(PdfFormTextField {
                data,
                lifetime: PhantomData,
            }),
            _ => Self::Unknown(PdfFormUnknownField {
                data,
                lifetime: PhantomData,
            }),
        })
    }
}
impl private::FieldOwner for PdfFormField<'_> {
    fn field(&self) -> &FieldData {
        match self {
            Self::PushButton(f) => &f.data,
            Self::Checkbox(f) => &f.data,
            Self::RadioButton(f) => &f.data,
            Self::ComboBox(f) => &f.data,
            Self::ListBox(f) => &f.data,
            Self::Signature(f) => &f.data,
            Self::Text(f) => &f.data,
            Self::Unknown(f) => &f.data,
        }
    }
}
impl<'a> PdfFormField<'a> {
    pub fn field_type(&self) -> PdfFormFieldType {
        match self {
            Self::PushButton(_) => PdfFormFieldType::PushButton,
            Self::Checkbox(_) => PdfFormFieldType::Checkbox,
            Self::RadioButton(_) => PdfFormFieldType::RadioButton,
            Self::ComboBox(_) => PdfFormFieldType::ComboBox,
            Self::ListBox(_) => PdfFormFieldType::ListBox,
            Self::Signature(_) => PdfFormFieldType::Signature,
            Self::Text(_) => PdfFormFieldType::Text,
            Self::Unknown(_) => PdfFormFieldType::Unknown,
        }
    }
    pub fn as_push_button_field(&self) -> Option<&PdfFormPushButtonField<'_>> {
        if let Self::PushButton(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_checkbox_field(&self) -> Option<&PdfFormCheckboxField<'_>> {
        if let Self::Checkbox(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_checkbox_field_mut(&mut self) -> Option<&mut PdfFormCheckboxField<'a>> {
        if let Self::Checkbox(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_radio_button_field(&self) -> Option<&PdfFormRadioButtonField<'_>> {
        if let Self::RadioButton(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_radio_button_field_mut(&mut self) -> Option<&mut PdfFormRadioButtonField<'a>> {
        if let Self::RadioButton(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_combo_box_field(&self) -> Option<&PdfFormComboBoxField<'_>> {
        if let Self::ComboBox(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_list_box_field(&self) -> Option<&PdfFormListBoxField<'_>> {
        if let Self::ListBox(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_signature_field(&self) -> Option<&PdfFormSignatureField<'_>> {
        if let Self::Signature(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_text_field(&self) -> Option<&PdfFormTextField<'_>> {
        if let Self::Text(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_text_field_mut(&mut self) -> Option<&mut PdfFormTextField<'a>> {
        if let Self::Text(field) = self {
            Some(field)
        } else {
            None
        }
    }
    pub fn as_unknown_field(&self) -> Option<&PdfFormUnknownField<'_>> {
        if let Self::Unknown(field) = self {
            Some(field)
        } else {
            None
        }
    }
}
pub trait PdfFormFieldCommon {
    /// Returns the name of this [PdfFormField], if any.
    fn name(&self) -> Option<String>;

    /// Returns the name of the currently set appearance stream for this [PdfFormField], if any.
    fn appearance_stream(&self) -> Option<String>;

    /// Returns the value currently set for the given appearance mode for this [PdfFormField],
    /// if any.
    fn appearance_mode_value(&self, appearance_mode: PdfAppearanceMode) -> Option<String>;

    /// Returns `true` if the value of this [PdfFormField] is read only.
    ///
    /// Users may not change the value of read-only fields, and any associated widget
    /// annotations will not interact with the user; that is, they will not respond
    /// to mouse clicks or change their appearance in response to mouse motions.
    fn is_read_only(&self) -> bool;

    /// Controls whether or not the value of this [PdfFormField] is read only.
    fn set_is_read_only(&mut self, is_read_only: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfFormField] must have a value at the time it is exported
    /// by any "submit form" action.
    ///
    /// For more information on "submit form" actions, refer to Section 8.6.4 of
    /// The PDF Reference (Sixth Edition, PDF Format 1.7), starting on page 702.
    fn is_required(&self) -> bool;

    /// Controls whether or not this [PdfFormField] must have a value at the time it is
    /// exported by any "submit form" action.
    ///
    /// For more information on "submit form" actions, refer to Section 8.6.4 of
    /// The PDF Reference (Sixth Edition, PDF Format 1.7), starting on page 702.
    fn set_is_required(&mut self, is_required: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if the value of this [PdfFormField] will be exported by any
    /// "submit form" action.
    ///
    /// For more information on "submit form" actions, refer to Section 8.6.4 of
    /// The PDF Reference (Sixth Edition, PDF Format 1.7), starting on page 702.
    fn is_exported_on_submit(&self) -> bool;

    /// Controls whether or not the value of this [PdfFormField] will be exported by any
    /// "submit form" action.
    ///
    /// For more information on "submit form" actions, refer to Section 8.6.4 of
    /// The PDF Reference Manual, version 1.7, starting on page 702.
    fn set_is_exported_on_submit(&mut self, is_exported: bool) -> Result<(), PdfiumError>;
}
impl<'a, T: private::FieldOwner> PdfFormFieldCommon for T {
    fn name(&self) -> Option<String> {
        self.field().name()
    }
    fn appearance_stream(&self) -> Option<String> {
        let value = fpdf_annot::string(&self.field().dict, b"AS");
        if value.is_empty() {
            None
        } else {
            Some(backend::text::decode(&value))
        }
    }
    fn appearance_mode_value(&self, appearance_mode: PdfAppearanceMode) -> Option<String> {
        fpdf_annot::appearance_text(&self.field().dict, appearance_mode as i32)
            .map(|u| backend::text::decode(&u))
    }
    fn is_read_only(&self) -> bool {
        self.field().core.field_flags() & 1 != 0
    }
    fn set_is_read_only(&mut self, is_read_only: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ReadOnly, is_read_only)
    }
    fn is_required(&self) -> bool {
        self.field().core.field_flags() & 2 != 0
    }
    fn set_is_required(&mut self, is_required: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::Required, is_required)
    }
    fn is_exported_on_submit(&self) -> bool {
        self.field().core.field_flags() & 4 == 0
    }
    fn set_is_exported_on_submit(&mut self, is_exported: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::NoExport, !is_exported)
    }
}
bitflags::bitflags! {#[derive(Copy,Clone)] pub(crate) struct PdfFormFieldFlags:u32 {const ReadOnly=1<<0;const Required=1<<1;const NoExport=1<<2;const TextMultiline=1<<12;const TextPassword=1<<13;const TextFileSelect=1<<20;const TextDoNotSpellCheck=1<<22;const TextDoNotScroll=1<<23;const TextComb=1<<24;const TextRichText=1<<25;const ChoiceCombo=1<<17;const ChoiceEdit=1<<18;const ChoiceSort=1<<19;const ChoiceMultiSelect=1<<21;const ChoiceDoNotSpellCheck=1<<22;const ChoiceCommitOnSelectionChange=1<<26;const ButtonNoToggleToOff=1<<14;const ButtonIsRadio=1<<15;const ButtonIsPushButton=1<<16;const ButtonIsRadiosInUnison=1<<25;}}
pub(crate) trait FieldHelpers: private::FieldOwner {
    fn get_string_value(&self, key: &str) -> Option<String> {
        let value = fpdf_annot::string(&self.field().dict, key.as_bytes());
        if value.is_empty() {
            None
        } else {
            Some(backend::text::decode(&value))
        }
    }
    fn set_string_value(&self, key: &str, value: &str) -> Result<(), PdfiumError> {
        self.field().check()?;
        fpdf_annot::set_string(
            &self.field().dict,
            key.as_bytes(),
            &value.chars().map(u32::from).collect::<Vec<_>>(),
        );
        Ok(())
    }
    #[cfg(all(test, feature = "image_api"))]
    fn set_flags_impl(&self, flags: PdfFormFieldFlags) -> bool {
        if self.field().check().is_err() {
            return false;
        }
        self.field().core.set_field_flags(flags.bits());
        true
    }
    fn get_flags_impl(&self) -> PdfFormFieldFlags {
        PdfFormFieldFlags::from_bits_retain(self.field().core.field_flags())
    }
    fn update_one_flag_impl(
        &self,
        flag: PdfFormFieldFlags,
        value: bool,
    ) -> Result<(), PdfiumError> {
        self.field().check()?;
        let mut flags = self.get_flags_impl();
        flags.set(flag, value);
        self.field().core.set_field_flags(flags.bits());
        Ok(())
    }
    fn value_impl(&self) -> Option<String> {
        self.field().value()
    }
    fn set_value_impl(&self, value: &str) -> Result<(), PdfiumError> {
        self.field().set_value(value)
    }
    fn index_in_group_impl(&self) -> u32 {
        self.field()
            .core
            .control_index(&self.field().control)
            .max(0) as u32
    }
    fn is_checked_impl(&self) -> Result<bool, PdfiumError> {
        self.field().check()?;
        Ok(self.field().control.is_checked())
    }
    fn set_checked_core(&self, is_checked: bool) -> Result<(), PdfiumError> {
        self.field().check()?;
        let i = self.index_in_group_impl() as usize;
        self.field()
            .core
            .check_control(i, is_checked, NotificationOption::DoNotNotify);
        Ok(())
    }
}
impl<T: private::FieldOwner> FieldHelpers for T {}
impl<'a> PdfFormPushButtonField<'a> {}
impl<'a> PdfFormCheckboxField<'a> {
    pub fn index_in_group(&self) -> u32 {
        self.index_in_group_impl()
    }
    pub fn group_value(&self) -> Option<String> {
        self.value_impl()
    }
    pub fn is_checked(&self) -> Result<bool, PdfiumError> {
        self.is_checked_impl()
    }
    pub fn set_checked(&mut self, is_checked: bool) -> Result<(), PdfiumError> {
        self.set_checked_core(is_checked)
    }
}
impl<'a> PdfFormRadioButtonField<'a> {
    pub fn index_in_group(&self) -> u32 {
        self.index_in_group_impl()
    }
    pub fn group_value(&self) -> Option<String> {
        self.value_impl()
    }
    pub fn is_checked(&self) -> Result<bool, PdfiumError> {
        self.is_checked_impl()
    }
    pub fn set_checked(&mut self) -> Result<(), PdfiumError> {
        self.set_checked_core(true)
    }
    pub fn is_group_selection_required(&self) -> bool {
        !self
            .get_flags_impl()
            .contains(PdfFormFieldFlags::ButtonNoToggleToOff)
    }
    pub fn set_is_group_selection_required(
        &mut self,
        is_group_selection_required: bool,
    ) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(
            PdfFormFieldFlags::ButtonNoToggleToOff,
            !is_group_selection_required,
        )
    }
    pub fn is_group_in_unison(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ButtonIsRadiosInUnison)
    }
    pub fn set_is_group_in_unison(&mut self, is_group_in_unison: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(
            PdfFormFieldFlags::ButtonIsRadiosInUnison,
            is_group_in_unison,
        )
    }
}
impl<'a> PdfFormComboBoxField<'a> {
    pub fn options(&self) -> &PdfFormFieldOptions<'_> {
        &self.options
    }
    pub fn value(&self) -> Option<String> {
        self.options()
            .iter()
            .find(|option| option.is_set())
            .and_then(|option| option.label().cloned())
    }
    pub fn has_editable_text_box(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceEdit)
    }
    pub fn set_has_editable_text_box(
        &mut self,
        has_editable_text_box: bool,
    ) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ChoiceEdit, has_editable_text_box)
    }
    pub fn is_sorted(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceSort)
    }
    pub fn set_is_sorted(&mut self, is_sorted: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ChoiceSort, is_sorted)
    }
    pub fn is_multiselect(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceMultiSelect)
    }
    pub fn set_is_multiselect(&mut self, is_multiselect: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ChoiceMultiSelect, is_multiselect)
    }
    pub fn is_spell_checked(&self) -> bool {
        !self
            .get_flags_impl()
            .contains(PdfFormFieldFlags::TextDoNotSpellCheck)
    }
    pub fn set_is_spell_checked(&mut self, is_spell_checked: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextDoNotSpellCheck, !is_spell_checked)
    }
    pub fn is_commit_on_selection_change(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceCommitOnSelectionChange)
    }
    pub fn set_is_commit_on_selection_change(
        &mut self,
        is_commit_on_selection_change: bool,
    ) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(
            PdfFormFieldFlags::ChoiceCommitOnSelectionChange,
            is_commit_on_selection_change,
        )
    }
}
impl<'a> PdfFormListBoxField<'a> {
    pub fn options(&self) -> &PdfFormFieldOptions<'_> {
        &self.options
    }
    pub fn value(&self) -> Option<String> {
        self.options()
            .iter()
            .find(|option| option.is_set())
            .and_then(|option| option.label().cloned())
    }
    pub fn is_sorted(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceSort)
    }
    pub fn set_is_sorted(&mut self, is_sorted: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ChoiceSort, is_sorted)
    }
    pub fn is_multiselect(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceMultiSelect)
    }
    pub fn set_is_multiselect(&mut self, is_multiselect: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::ChoiceMultiSelect, is_multiselect)
    }
    pub fn is_commit_on_selection_change(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::ChoiceCommitOnSelectionChange)
    }
    pub fn set_is_commit_on_selection_change(
        &mut self,
        is_commit_on_selection_change: bool,
    ) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(
            PdfFormFieldFlags::ChoiceCommitOnSelectionChange,
            is_commit_on_selection_change,
        )
    }
}
impl<'a> PdfFormTextField<'a> {
    pub fn value(&self) -> Option<String> {
        if self.is_rich_text() {
            self.get_string_value("RV")
        } else {
            self.value_impl()
        }
    }
    pub fn set_value(&mut self, value: &str) -> Result<(), PdfiumError> {
        if self.is_rich_text() {
            self.set_string_value("RV", value)
        } else {
            self.set_value_impl(value)
        }
    }
    pub fn is_multiline(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::TextMultiline)
    }
    pub fn set_is_multiline(&self, is_multiline: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextMultiline, is_multiline)
    }
    pub fn is_password(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::TextPassword)
    }
    pub fn set_is_password(&self, is_password: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextPassword, is_password)
    }
    pub fn is_file_select(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::TextFileSelect)
    }
    pub fn set_is_file_select(&mut self, is_file_select: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextFileSelect, is_file_select)
    }
    pub fn is_spell_checked(&self) -> bool {
        !self
            .get_flags_impl()
            .contains(PdfFormFieldFlags::TextDoNotSpellCheck)
    }
    pub fn set_is_spell_checked(&mut self, is_spell_checked: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextDoNotSpellCheck, !is_spell_checked)
    }
    pub fn is_scrollable(&self) -> bool {
        !self
            .get_flags_impl()
            .contains(PdfFormFieldFlags::TextDoNotScroll)
    }
    pub fn set_is_scrollable(&mut self, is_scrollable: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextDoNotScroll, !is_scrollable)
    }
    pub fn is_combed(&self) -> bool {
        // This flag only takes effect if the multi-line, password, and file select flags
        // are all unset.

        !self.is_multiline()
            && !self.is_password()
            && !self.is_file_select()
            && self.get_flags_impl().contains(PdfFormFieldFlags::TextComb)
    }
    pub fn is_rich_text(&self) -> bool {
        self.get_flags_impl()
            .contains(PdfFormFieldFlags::TextRichText)
    }
    pub fn set_is_rich_text(&mut self, is_rich_text: bool) -> Result<(), PdfiumError> {
        self.update_one_flag_impl(PdfFormFieldFlags::TextRichText, is_rich_text)
    }
}
impl<'a> PdfFormSignatureField<'a> {}
impl<'a> PdfFormUnknownField<'a> {}
pub type PdfFormFieldOptionIndex = usize;
pub struct PdfFormFieldOptions<'a> {
    data: FieldData,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfFormFieldOptions<'a> {
    pub fn len(&self) -> PdfFormFieldOptionIndex {
        self.data.core.count_options()
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfFormFieldOptionIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfFormFieldOptionIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }
    pub fn get(&self, index: PdfFormFieldOptionIndex) -> Result<PdfFormFieldOption, PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::FormFieldOptionIndexOutOfBounds);
        }
        let label = Some(backend::text::decode(
            &self.data.core.option_label(index as i32),
        ));
        Ok(PdfFormFieldOption {
            index,
            is_set: self.data.core.is_item_selected(index as i32),
            label,
        })
    }
    pub fn iter(&self) -> PdfFormFieldOptionsIterator<'_> {
        PdfFormFieldOptionsIterator {
            options: self,
            next_index: 0,
        }
    }
}
pub struct PdfFormFieldOption {
    index: PdfFormFieldOptionIndex,
    is_set: bool,
    label: Option<String>,
}
impl PdfFormFieldOption {
    pub fn index(&self) -> PdfFormFieldOptionIndex {
        self.index
    }
    pub fn is_set(&self) -> bool {
        self.is_set
    }
    pub fn label(&self) -> Option<&String> {
        self.label.as_ref()
    }
}
pub struct PdfFormFieldOptionsIterator<'a> {
    options: &'a PdfFormFieldOptions<'a>,
    next_index: usize,
}
impl Iterator for PdfFormFieldOptionsIterator<'_> {
    type Item = PdfFormFieldOption;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.options.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.options.get(i).ok()
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfFormType {
    // The FORMTYPE_COUNT constant simply specifies the number of form types supported
    // by Pdfium; we do not need to expose it.
    None = 0 as isize,
    Acrobat = 1 as isize,
    XfaFull = 2 as isize,
    XfaForeground = 3 as isize,
}
pub struct PdfForm<'a> {
    pub(crate) document: Rc<backend::document::DocumentState>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfForm<'a> {
    pub(crate) fn from_core(document: Rc<backend::document::DocumentState>) -> Option<Self> {
        let kind =
            pdfoxium_core::fpdfsdk::fpdf_view::form_type(document.core.borrow().root().as_ref());
        (kind != 0).then_some(Self {
            document,
            lifetime: PhantomData,
        })
    }
}
impl<'a> PdfForm<'a> {
    pub fn form_type(&self) -> PdfFormType {
        match pdfoxium_core::fpdfsdk::fpdf_view::form_type(
            self.document.core.borrow().root().as_ref(),
        ) {
            1 => PdfFormType::Acrobat,
            2 => PdfFormType::XfaFull,
            3 => PdfFormType::XfaForeground,
            _ => PdfFormType::None,
        }
    }
    pub fn field_values(&self, pages: &'a PdfPages<'a>) -> HashMap<String, Option<String>> {
        let mut result = HashMap::new();

        let field_value_true = Some("true".to_string());

        let field_value_false = Some("false".to_string());

        for page in pages.iter() {
            for annotation in page.annotations().iter() {
                if let Some(field) = annotation.as_form_field() {
                    let field_type = field.field_type();

                    let field_value = match field_type {
                        PdfFormFieldType::Checkbox => {
                            if field
                                .as_checkbox_field()
                                .unwrap()
                                .is_checked()
                                .unwrap_or(false)
                            {
                                field_value_true.clone()
                            } else {
                                field_value_false.clone()
                            }
                        }
                        PdfFormFieldType::ComboBox => field.as_combo_box_field().unwrap().value(),
                        PdfFormFieldType::ListBox => field.as_list_box_field().unwrap().value(),
                        PdfFormFieldType::RadioButton => {
                            let field = field.as_radio_button_field().unwrap();

                            if field.is_checked().unwrap_or(false) {
                                field.group_value()
                            } else {
                                field_value_false.clone()
                            }
                        }
                        PdfFormFieldType::Text => field.as_text_field().unwrap().value(),
                        PdfFormFieldType::PushButton
                        | PdfFormFieldType::Signature
                        | PdfFormFieldType::Unknown => None,
                    };

                    // A group of checkbox or radio button controls all share the same name, so
                    // as we iterate over the controls, the value of the group will be updated.
                    // Only the value of the last control in the group will be captured.
                    // This isn't the behaviour we want; we prefer to capture the value of
                    // a checked control in preference to an unchecked control.

                    let field_name = field.name().unwrap_or_default();

                    if (field_type == PdfFormFieldType::Checkbox
                        || field_type == PdfFormFieldType::RadioButton)
                        && result.contains_key(&field_name)
                    {
                        // Only overwrite an existing entry for this control group if
                        // this field is set.

                        if field_value != field_value_false {
                            result.insert(field_name, field_value);
                        }
                    } else {
                        // For all other control types, we assume that field names are unique.

                        result.insert(field_name, field_value);
                    }
                }
            }
        }

        result
    }
}

impl<'a> From<PdfFormPushButtonField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormPushButtonField<'a>) -> Self {
        Self::PushButton(field)
    }
}

impl<'a> From<PdfFormCheckboxField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormCheckboxField<'a>) -> Self {
        Self::Checkbox(field)
    }
}

impl<'a> From<PdfFormRadioButtonField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormRadioButtonField<'a>) -> Self {
        Self::RadioButton(field)
    }
}

impl<'a> From<PdfFormComboBoxField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormComboBoxField<'a>) -> Self {
        Self::ComboBox(field)
    }
}

impl<'a> From<PdfFormListBoxField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormListBoxField<'a>) -> Self {
        Self::ListBox(field)
    }
}

impl<'a> From<PdfFormTextField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormTextField<'a>) -> Self {
        Self::Text(field)
    }
}

impl<'a> From<PdfFormSignatureField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormSignatureField<'a>) -> Self {
        Self::Signature(field)
    }
}

impl<'a> From<PdfFormUnknownField<'a>> for PdfFormField<'a> {
    fn from(field: PdfFormUnknownField<'a>) -> Self {
        Self::Unknown(field)
    }
}
