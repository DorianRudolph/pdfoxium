// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::{backend, object_collections::ObjectCollection, prelude::*};
use chrono::{DateTime, Utc};
use pdfoxium_core::{
    fpdfapi::{page::cpdf_annotcontext::CpdfAnnotContext, parser::cpdf_object::ObjectRef},
    fpdfsdk::fpdf_annot as sdk,
};
use std::{
    cell::RefCell,
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
pub(crate) struct AnnotationState {
    pub(crate) page: backend::PdfPage,
    pub(crate) dict: ObjectRef,
    pub(crate) context: Rc<RefCell<CpdfAnnotContext>>,
    epoch: u64,
    index: std::cell::Cell<usize>,
    pub(crate) appearance_epoch: std::cell::Cell<u64>,
}
impl AnnotationState {
    pub(crate) fn check(&self) -> Result<(), PdfiumError> {
        self.page.check_live()?;
        if self.page.state.epoch.get() != self.epoch {
            return Err(PdfiumError::OwnershipNotAttachedToAnnotation);
        }
        let page = self.page.state.core.borrow();
        if sdk::annotation_at(&page, self.index.get())
            .is_some_and(|dict| Rc::ptr_eq(&dict, &self.dict))
        {
            return Ok(());
        }
        let index = sdk::annotation_index(&page, &self.dict)
            .ok_or(PdfiumError::OwnershipNotAttachedToAnnotation)?;
        self.index.set(index);
        Ok(())
    }
    fn string(&self, key: &[u8]) -> Option<String> {
        self.check().ok()?;
        if sdk::value_type(&self.dict, key)
            != Some(pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::ObjectType::String)
        {
            return None;
        }
        let value = sdk::string(&self.dict, key);
        if value.is_empty() {
            None
        } else {
            Some(backend::text::decode(&value))
        }
    }
    fn set_string(&self, key: &[u8], value: &str) -> Result<(), PdfiumError> {
        self.check()?;
        if key != b"M" {
            let date = Utc::now().format("D:%Y%m%d%H%M%SZ00'00'").to_string();
            sdk::set_string(
                &self.dict,
                b"M",
                &date.chars().map(u32::from).collect::<Vec<_>>(),
            );
        }
        sdk::set_string(
            &self.dict,
            key,
            &value.chars().map(u32::from).collect::<Vec<_>>(),
        );
        Ok(())
    }
    fn set_flag(&self, bit: i32, value: bool) -> Result<(), PdfiumError> {
        self.check()?;
        let flags = sdk::flags(&self.dict);
        sdk::set_flags(&self.dict, if value { flags | bit } else { flags & !bit });
        Ok(())
    }
}
pub(crate) struct AnnotationData<'a> {
    state: Rc<AnnotationState>,
    objects: PdfPageAnnotationObjects<'a>,
    points: PdfPageAnnotationAttachmentPoints<'a>,
    field: Option<PdfFormField<'a>>,
}
impl<'a> AnnotationData<'a> {
    fn from_core(page: backend::PdfPage, dict: ObjectRef, index: usize) -> Self {
        let key = Rc::as_ptr(&dict).cast::<()>() as usize;
        let state = page
            .document
            .annotations
            .borrow()
            .get(&key)
            .and_then(std::rc::Weak::upgrade)
            .filter(|s| s.epoch == page.state.epoch.get());
        let state = state.unwrap_or_else(|| {
            let mut context = CpdfAnnotContext::new(dict.clone());
            sdk::ensure_form(&mut context, &page.state.core.borrow());
            let state = Rc::new(AnnotationState {
                epoch: page.state.epoch.get(),
                index: std::cell::Cell::new(index),
                appearance_epoch: std::cell::Cell::new(1),
                page: page.alias(),
                dict: dict.clone(),
                context: Rc::new(RefCell::new(context)),
            });
            page.document
                .annotations
                .borrow_mut()
                .insert(key, Rc::downgrade(&state));
            state
        });
        let objects = PdfPageAnnotationObjects {
            collection: ObjectCollection::Annotation {
                page: page.alias(),
                dict: dict.clone(),
                context: state.context.clone(),
            },
            state: state.clone(),
            lifetime: PhantomData,
        };
        let points = PdfPageAnnotationAttachmentPoints {
            state: state.clone(),
            lifetime: PhantomData,
        };
        let field = PdfFormField::from_annotation(page, dict);
        Self {
            state,
            objects,
            points,
            field,
        }
    }
}
mod private {
    pub(crate) trait AnnotationOwner {
        fn data(&self) -> &super::AnnotationData<'_>;
    }
}
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum PdfPageAnnotationType {
    Unknown = 0 as isize,
    Text = 1 as isize,
    Link = 2 as isize,
    FreeText = 3 as isize,
    Line = 4 as isize,
    Square = 5 as isize,
    Circle = 6 as isize,
    Polygon = 7 as isize,
    Polyline = 8 as isize,
    Highlight = 9 as isize,
    Underline = 10 as isize,
    Squiggly = 11 as isize,
    Strikeout = 12 as isize,
    Stamp = 13 as isize,
    Caret = 14 as isize,
    Ink = 15 as isize,
    Popup = 16 as isize,
    FileAttachment = 17 as isize,
    Sound = 18 as isize,
    Movie = 19 as isize,
    Widget = 20 as isize,
    Screen = 21 as isize,
    PrinterMark = 22 as isize,
    TrapNet = 23 as isize,
    Watermark = 24 as isize,
    ThreeD = 25 as isize,
    RichMedia = 26 as isize,
    XfaWidget = 27 as isize,
    Redacted = 28 as isize,
}
pub enum PdfPageAnnotation<'a> {
    Circle(PdfPageCircleAnnotation<'a>),
    FreeText(PdfPageFreeTextAnnotation<'a>),
    Highlight(PdfPageHighlightAnnotation<'a>),
    Ink(PdfPageInkAnnotation<'a>),
    Link(PdfPageLinkAnnotation<'a>),
    Popup(PdfPagePopupAnnotation<'a>),
    Square(PdfPageSquareAnnotation<'a>),
    Squiggly(PdfPageSquigglyAnnotation<'a>),
    Stamp(PdfPageStampAnnotation<'a>),
    Strikeout(PdfPageStrikeoutAnnotation<'a>),
    Text(PdfPageTextAnnotation<'a>),
    Underline(PdfPageUnderlineAnnotation<'a>),
    Widget(PdfPageWidgetAnnotation<'a>),
    XfaWidget(PdfPageXfaWidgetAnnotation<'a>),
    Redacted(PdfPageRedactedAnnotation<'a>),

    /// Common properties shared by all [PdfPageAnnotation] types can still be accessed for
    /// annotations not supported by Pdfium, but annotation-specific functionality
    /// will be unavailable.
    Unsupported(PdfPageUnsupportedAnnotation<'a>),
}
pub struct PdfPageCircleAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageCircleAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageCircleAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageCircleAnnotation<'a>) -> Self {
        Self::Circle(v)
    }
}
pub struct PdfPageFreeTextAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageFreeTextAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageFreeTextAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageFreeTextAnnotation<'a>) -> Self {
        Self::FreeText(v)
    }
}
pub struct PdfPageHighlightAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageHighlightAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageHighlightAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageHighlightAnnotation<'a>) -> Self {
        Self::Highlight(v)
    }
}
pub struct PdfPageInkAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageInkAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageInkAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageInkAnnotation<'a>) -> Self {
        Self::Ink(v)
    }
}
pub struct PdfPageLinkAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageLinkAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageLinkAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageLinkAnnotation<'a>) -> Self {
        Self::Link(v)
    }
}
pub struct PdfPagePopupAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPagePopupAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPagePopupAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPagePopupAnnotation<'a>) -> Self {
        Self::Popup(v)
    }
}
pub struct PdfPageSquareAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageSquareAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageSquareAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageSquareAnnotation<'a>) -> Self {
        Self::Square(v)
    }
}
pub struct PdfPageSquigglyAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageSquigglyAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageSquigglyAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageSquigglyAnnotation<'a>) -> Self {
        Self::Squiggly(v)
    }
}
pub struct PdfPageStampAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageStampAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageStampAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageStampAnnotation<'a>) -> Self {
        Self::Stamp(v)
    }
}
pub struct PdfPageStrikeoutAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageStrikeoutAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageStrikeoutAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageStrikeoutAnnotation<'a>) -> Self {
        Self::Strikeout(v)
    }
}
pub struct PdfPageTextAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageTextAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageTextAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageTextAnnotation<'a>) -> Self {
        Self::Text(v)
    }
}
pub struct PdfPageUnderlineAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageUnderlineAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageUnderlineAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageUnderlineAnnotation<'a>) -> Self {
        Self::Underline(v)
    }
}
pub struct PdfPageWidgetAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageWidgetAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageWidgetAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageWidgetAnnotation<'a>) -> Self {
        Self::Widget(v)
    }
}
pub struct PdfPageXfaWidgetAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageXfaWidgetAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageXfaWidgetAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageXfaWidgetAnnotation<'a>) -> Self {
        Self::XfaWidget(v)
    }
}
pub struct PdfPageRedactedAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageRedactedAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageRedactedAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageRedactedAnnotation<'a>) -> Self {
        Self::Redacted(v)
    }
}
pub struct PdfPageUnsupportedAnnotation<'a> {
    data: AnnotationData<'a>,
}
impl<'a> private::AnnotationOwner for PdfPageUnsupportedAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        &self.data
    }
}
impl<'a> From<PdfPageUnsupportedAnnotation<'a>> for PdfPageAnnotation<'a> {
    fn from(v: PdfPageUnsupportedAnnotation<'a>) -> Self {
        Self::Unsupported(v)
    }
}
impl<'a> private::AnnotationOwner for PdfPageAnnotation<'a> {
    fn data(&self) -> &AnnotationData<'_> {
        match self {
            Self::Circle(a) => &a.data,
            Self::FreeText(a) => &a.data,
            Self::Highlight(a) => &a.data,
            Self::Ink(a) => &a.data,
            Self::Link(a) => &a.data,
            Self::Popup(a) => &a.data,
            Self::Square(a) => &a.data,
            Self::Squiggly(a) => &a.data,
            Self::Stamp(a) => &a.data,
            Self::Strikeout(a) => &a.data,
            Self::Text(a) => &a.data,
            Self::Underline(a) => &a.data,
            Self::Widget(a) => &a.data,
            Self::XfaWidget(a) => &a.data,
            Self::Redacted(a) => &a.data,
            Self::Unsupported(a) => &a.data,
        }
    }
}
impl<'a> PdfPageAnnotation<'a> {
    pub(crate) fn from_core(page: backend::PdfPage, dict: ObjectRef, index: usize) -> Self {
        let kind = sdk::subtype(&dict) as i32;
        let data = AnnotationData::from_core(page, dict, index);
        match kind {
            6 => Self::Circle(PdfPageCircleAnnotation { data }),
            3 => Self::FreeText(PdfPageFreeTextAnnotation { data }),
            9 => Self::Highlight(PdfPageHighlightAnnotation { data }),
            15 => Self::Ink(PdfPageInkAnnotation { data }),
            2 => Self::Link(PdfPageLinkAnnotation { data }),
            16 => Self::Popup(PdfPagePopupAnnotation { data }),
            5 => Self::Square(PdfPageSquareAnnotation { data }),
            11 => Self::Squiggly(PdfPageSquigglyAnnotation { data }),
            13 => Self::Stamp(PdfPageStampAnnotation { data }),
            12 => Self::Strikeout(PdfPageStrikeoutAnnotation { data }),
            1 => Self::Text(PdfPageTextAnnotation { data }),
            10 => Self::Underline(PdfPageUnderlineAnnotation { data }),
            20 => Self::Widget(PdfPageWidgetAnnotation { data }),
            27 => Self::XfaWidget(PdfPageXfaWidgetAnnotation { data }),
            28 => Self::Redacted(PdfPageRedactedAnnotation { data }),
            _ => Self::Unsupported(PdfPageUnsupportedAnnotation { data }),
        }
    }
    pub(crate) fn core_object_target(
        &self,
    ) -> Result<(backend::PdfPage, ObjectRef, Rc<RefCell<CpdfAnnotContext>>), PdfiumError> {
        use private::AnnotationOwner;
        let s = &self.data().state;
        s.check()?;
        if !sdk::ensure_append_form(&mut s.context.borrow_mut(), &s.page.state.core.borrow()) {
            return Err(PdfiumError::UnsupportedPdfPageObjectType);
        }
        Ok((s.page.alias(), s.dict.clone(), s.context.clone()))
    }
}
impl<'a> PdfPageAnnotation<'a> {
    pub fn annotation_type(&self) -> PdfPageAnnotationType {
        match self {
            Self::Circle(_) => PdfPageAnnotationType::Circle,
            Self::FreeText(_) => PdfPageAnnotationType::FreeText,
            Self::Highlight(_) => PdfPageAnnotationType::Highlight,
            Self::Ink(_) => PdfPageAnnotationType::Ink,
            Self::Link(_) => PdfPageAnnotationType::Link,
            Self::Popup(_) => PdfPageAnnotationType::Popup,
            Self::Square(_) => PdfPageAnnotationType::Square,
            Self::Squiggly(_) => PdfPageAnnotationType::Squiggly,
            Self::Stamp(_) => PdfPageAnnotationType::Stamp,
            Self::Strikeout(_) => PdfPageAnnotationType::Strikeout,
            Self::Text(_) => PdfPageAnnotationType::Text,
            Self::Underline(_) => PdfPageAnnotationType::Underline,
            Self::Widget(_) => PdfPageAnnotationType::Widget,
            Self::XfaWidget(_) => PdfPageAnnotationType::XfaWidget,
            Self::Redacted(_) => PdfPageAnnotationType::Redacted,
            Self::Unsupported(a) => a.get_type(),
        }
    }
    pub fn is_supported(&self) -> bool {
        !self.is_unsupported()
    }
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_))
    }
    pub fn as_circle_annotation(&self) -> Option<&PdfPageCircleAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Circle(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_circle_annotation_mut(&mut self) -> Option<&mut PdfPageCircleAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Circle(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_free_text_annotation(&self) -> Option<&PdfPageFreeTextAnnotation<'_>> {
        match self {
            PdfPageAnnotation::FreeText(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_free_text_annotation_mut(&mut self) -> Option<&mut PdfPageFreeTextAnnotation<'a>> {
        match self {
            PdfPageAnnotation::FreeText(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_highlight_annotation(&self) -> Option<&PdfPageHighlightAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Highlight(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_highlight_annotation_mut(&mut self) -> Option<&mut PdfPageHighlightAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Highlight(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_ink_annotation(&self) -> Option<&PdfPageInkAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Ink(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_ink_annotation_mut(&mut self) -> Option<&mut PdfPageInkAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Ink(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_link_annotation(&self) -> Option<&PdfPageLinkAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Link(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_link_annotation_mut(&mut self) -> Option<&mut PdfPageLinkAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Link(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_popup_annotation(&self) -> Option<&PdfPagePopupAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Popup(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_popup_annotation_mut(&mut self) -> Option<&mut PdfPagePopupAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Popup(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_square_annotation(&self) -> Option<&PdfPageSquareAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Square(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_square_annotation_mut(&mut self) -> Option<&mut PdfPageSquareAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Square(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_squiggly_annotation(&self) -> Option<&PdfPageSquigglyAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Squiggly(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_squiggly_annotation_mut(&mut self) -> Option<&mut PdfPageSquigglyAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Squiggly(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_stamp_annotation(&self) -> Option<&PdfPageStampAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Stamp(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_stamp_annotation_mut(&mut self) -> Option<&mut PdfPageStampAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Stamp(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_strikeout_annotation(&self) -> Option<&PdfPageStrikeoutAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Strikeout(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_strikeout_annotation_mut(&mut self) -> Option<&mut PdfPageStrikeoutAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Strikeout(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_text_annotation(&self) -> Option<&PdfPageTextAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Text(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_text_annotation_mut(&mut self) -> Option<&mut PdfPageTextAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Text(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_underline_annotation(&self) -> Option<&PdfPageUnderlineAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Underline(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_underline_annotation_mut(&mut self) -> Option<&mut PdfPageUnderlineAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Underline(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_widget_annotation(&self) -> Option<&PdfPageWidgetAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Widget(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_widget_annotation_mut(&mut self) -> Option<&mut PdfPageWidgetAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Widget(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_xfa_widget_annotation(&self) -> Option<&PdfPageXfaWidgetAnnotation<'_>> {
        match self {
            PdfPageAnnotation::XfaWidget(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_xfa_widget_annotation_mut(&mut self) -> Option<&mut PdfPageXfaWidgetAnnotation<'a>> {
        match self {
            PdfPageAnnotation::XfaWidget(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_redacted_annotation(&self) -> Option<&PdfPageRedactedAnnotation<'_>> {
        match self {
            PdfPageAnnotation::Redacted(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_redacted_annotation_mut(&mut self) -> Option<&mut PdfPageRedactedAnnotation<'a>> {
        match self {
            PdfPageAnnotation::Redacted(annotation) => Some(annotation),
            _ => None,
        }
    }
    pub fn as_form_field(&self) -> Option<&PdfFormField<'_>> {
        use private::AnnotationOwner;
        self.data().field.as_ref()
    }
    pub fn as_form_field_mut(&mut self) -> Option<&mut PdfFormField<'a>> {
        match self {
            Self::Circle(a) => a.data.field.as_mut(),
            Self::FreeText(a) => a.data.field.as_mut(),
            Self::Highlight(a) => a.data.field.as_mut(),
            Self::Ink(a) => a.data.field.as_mut(),
            Self::Link(a) => a.data.field.as_mut(),
            Self::Popup(a) => a.data.field.as_mut(),
            Self::Square(a) => a.data.field.as_mut(),
            Self::Squiggly(a) => a.data.field.as_mut(),
            Self::Stamp(a) => a.data.field.as_mut(),
            Self::Strikeout(a) => a.data.field.as_mut(),
            Self::Text(a) => a.data.field.as_mut(),
            Self::Underline(a) => a.data.field.as_mut(),
            Self::Widget(a) => a.data.field.as_mut(),
            Self::XfaWidget(a) => a.data.field.as_mut(),
            Self::Redacted(a) => a.data.field.as_mut(),
            Self::Unsupported(a) => a.data.field.as_mut(),
        }
    }
}
pub trait PdfPageAnnotationCommon {
    /// Returns the name of this [PdfPageAnnotation], if any. This is a text string uniquely identifying
    /// this annotation among all the annotations attached to the containing page.
    fn name(&self) -> Option<String>;

    /// Returns `true` if this [PdfPageAnnotation] supports applying text markup to the page
    /// by setting the annotation contents using the [PdfPageAnnotationCommon::set_contents()]
    /// function.
    fn is_markup_annotation(&self) -> bool;

    /// Returns `true` if this [PdfPageAnnotation] supports setting attachment points that
    /// visually associate it with a `PdfPageObject`.
    fn has_attachment_points(&self) -> bool;

    /// Returns the bounding box of this [PdfPageAnnotation].
    fn bounds(&self) -> Result<PdfRect, PdfiumError>;

    /// Sets the bounding box of this [PdfPageAnnotation].
    ///
    /// This sets the position, the width, and the height of the annotation in a single operation.
    /// To set these properties separately, use the [PdfPageAnnotationCommon::set_position()],
    /// [PdfPageAnnotationCommon::set_width()], and [PdfPageAnnotationCommon::set_height()] functions.
    fn set_bounds(&mut self, bounds: PdfRect) -> Result<(), PdfiumError>;

    /// Sets the bottom right corner of this [PdfPageAnnotation] to the given values.
    ///
    /// To set the position, the width, and the height of the annotation in a single operation,
    /// use the [PdfPageAnnotationCommon::set_bounds()] function.
    fn set_position(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError>;

    /// Sets the width of this [PdfPageAnnotation] to the given value.
    ///
    /// To set the position, the width, and the height of the annotation in a single operation,
    /// use the [PdfPageAnnotationCommon::set_bounds()] function.
    fn set_width(&mut self, width: PdfPoints) -> Result<(), PdfiumError>;

    /// Sets the height of this [PdfPageAnnotation] to the given value.
    ///
    /// To set the position, the width, and the height of the annotation in a single operation,
    /// use the [PdfPageAnnotationCommon::set_bounds()] function.
    fn set_height(&mut self, width: PdfPoints) -> Result<(), PdfiumError>;

    /// Returns the text to be displayed for this [PdfPageAnnotation], or, if this type of annotation
    /// does not display text, an alternate description of the annotation's contents in human-readable
    /// form. In either case this text is useful when extracting the document's contents in support
    /// of accessibility to users with disabilities or for other purposes.
    fn contents(&self) -> Option<String>;

    /// Sets the text to be displayed for this [PdfPageAnnotation], or, if this type of annotation
    /// does not display text, an alternate description of the annotation's contents in human-readable
    /// form for providing accessibility to users with disabilities or for other purposes.
    fn set_contents(&mut self, contents: &str) -> Result<(), PdfiumError>;

    /// Returns the name of the creator of this [PdfPageAnnotation], if any.
    fn creator(&self) -> Option<String>;

    /// Sets the name of the creator of this [PdfPageAnnotation].
    fn set_creator(&mut self, creator: &str) -> Result<(), PdfiumError>;

    /// Returns the date and time when this [PdfPageAnnotation] was originally created, if any.
    fn creation_date(&self) -> Option<String>;

    /// Sets the date and time when this [PdfPageAnnotation] was originally created.
    fn set_creation_date(&mut self, date: DateTime<Utc>) -> Result<(), PdfiumError>;

    /// Returns the date and time when this [PdfPageAnnotation] was last modified, if any.
    fn modification_date(&self) -> Option<String>;

    /// Sets the date and time when this [PdfPageAnnotation] was last modified.
    fn set_modification_date(&mut self, date: DateTime<Utc>) -> Result<(), PdfiumError>;

    /// Returns the color of any filled paths in this [PdfPageAnnotation].
    fn fill_color(&self) -> Result<PdfColor, PdfiumError>;

    /// Sets the color of any filled paths in this [PdfPageAnnotation].
    fn set_fill_color(&mut self, fill_color: PdfColor) -> Result<(), PdfiumError>;

    /// Returns the color of any stroked paths in this [PdfPageAnnotation].
    fn stroke_color(&self) -> Result<PdfColor, PdfiumError>;

    /// Sets the color of any stroked paths in this [PdfPageAnnotation].
    fn set_stroke_color(&mut self, stroke_color: PdfColor) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] should not be displayed to the user
    /// if it does not belong to one of the standard annotation types and no annotation
    /// handler is available that supports it.
    fn is_invisible_if_unsupported(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] should be displayed to the user
    /// if it does not belong to one of the standard annotation types and no annotation
    /// handler is available that supports it.
    fn set_is_invisible_if_unsupported(&mut self, is_invisible: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] should not be displayed or printed,
    /// nor allowed to interact with the user, regardless of its annotation type or whether
    /// an annotation handler is available that supports it.
    ///
    /// This flag was added in PDF version 1.2.
    fn is_hidden(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] should be displayed, printed,
    /// and allowed to interact with the user, regardless of its annotation type or whether
    /// an annotation handler is available that supports it.
    ///
    /// This flag was added in PDF version 1.2.
    fn set_is_hidden(&mut self, is_hidden: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] should be printed when the
    /// page is printed.
    ///
    /// This can be useful, for example, for annotations representing interactive
    /// push buttons, which would serve no meaningful purpose on the printed page.
    ///
    /// This flag was added in PDF version 1.2.
    fn is_printed(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] should be printed when the
    /// page is printed.
    ///
    /// This can be useful, for example, for annotations representing interactive
    /// push buttons, which would serve no meaningful purpose on the printed page.
    ///
    /// This flag was added in PDF version 1.2.
    fn set_is_printed(&mut self, is_printed: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if the appearance of this [PdfPageAnnotation] should scale to match
    /// the magnification of the page. If `false`, the location of the annotation on the
    /// page (defined by the upper-left corner of its annotation rectangle) will remain fixed,
    /// regardless of the page magnification.
    ///
    /// This flag was added in PDF version 1.3.
    fn is_zoomable(&self) -> bool;

    /// Controls whether or not the appearance of this [PdfPageAnnotation] should scale to
    /// match the magnification of the page.
    ///
    /// This flag was added in PDF version 1.3.
    fn set_is_zoomable(&mut self, is_zoomable: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if the appearance of this [PdfPageAnnotation] should rotate to match
    /// the rotation of the page. If `false`, the upper-left corner of the annotation rectangle
    /// will remain in a fixed location on the page, regardless of the page rotation.
    ///
    /// This flag was added in PDF version 1.3.
    fn is_rotatable(&self) -> bool;

    /// Controls whether or not the appearance of this [PdfPageAnnotation] should rotate
    /// to match the rotation of the page.
    ///
    /// This flag was added in PDF version 1.3.
    fn set_is_rotatable(&mut self, is_rotatable: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] should not be displayed to, or allowed to
    /// interact with, the user. The annotation may be printed (depending on the setting of
    /// the [PdfPageAnnotationCommon::is_printed()] flag) but should be considered
    /// hidden for purposes of on-screen display and user interaction.
    ///
    /// This flag was added in PDF version 1.3.
    fn is_printable_but_not_viewable(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] should be displayed to, and allowed
    /// to interact with, the user. Whether or not the annotation should be printed is
    /// controlled separately by the [PdfPageAnnotationCommon::set_is_printed()] function.
    ///
    /// This flag was added in PDF version 1.3.
    fn set_is_printable_but_not_viewable(
        &mut self,
        is_printable_but_not_viewable: bool,
    ) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] should not be allowed to interact
    /// with the user. The annotation may be displayed or printed (depending on the settings
    /// of the [PdfPageAnnotationCommon::is_printed()] and [PdfPageAnnotationCommon::is_printable_but_not_viewable()]
    /// flags) but should not respond to mouse clicks or change its appearance
    /// in response to mouse motions.
    ///
    /// This flag is ignored for widget annotations; its function is subsumed by
    /// the [PdfFormFieldCommon::is_read_only()] flag of the associated form field.
    ///
    /// THis flag was added in PDF version 1.3.
    fn is_read_only(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] should be allowed to interact
    /// with the user.
    ///
    /// This flag is ignored for widget annotations; its function is subsumed by
    /// the [PdfFormFieldCommon::is_read_only()] flag of the associated form field.
    ///
    /// THis flag was added in PDF version 1.3.
    fn set_is_read_only(&mut self, is_read_only: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if this [PdfPageAnnotation] is locked. Locked annotations cannot be
    /// deleted, repositioned, or resized by the user. The content of a locked annotation
    /// may still be editable, depending on the setting of the [PdfPageAnnotationCommon::is_editable()]
    /// flag.
    fn is_locked(&self) -> bool;

    /// Controls whether or not this [PdfPageAnnotation] is locked. Locked annotations cannot be
    /// deleted, repositioned, or resized by the user. The content of a locked annotation
    /// may still be editable, depending on the setting of the [PdfPageAnnotationCommon::set_is_editable()]
    /// function.
    fn set_is_locked(&mut self, is_locked: bool) -> Result<(), PdfiumError>;

    /// Returns `true` if the contents of this [PdfPageAnnotation] can be edited by the user.
    /// This setting does not control whether or not the annotation can be deleted,
    /// repositioned, or resized; those properties are controlled by the
    /// [PdfPageAnnotationCommon::is_locked()] flag.
    fn is_editable(&self) -> bool;

    /// Controls whether or not the contents of this [PdfPageAnnotation] can be edited by the user.
    /// This setting does not control whether or not the annotation can be deleted,
    /// repositioned, or resized; those properties are controlled by the
    /// [PdfPageAnnotationCommon::set_is_locked()] function.
    fn set_is_editable(&mut self, is_editable: bool) -> Result<(), PdfiumError>;

    /// Returns an immutable collection of all the page objects in this [PdfPageAnnotation].
    ///
    /// Page objects can be retrieved from any type of [PdfPageAnnotation], but Pdfium currently
    /// only permits adding new page objects to, or removing existing page objects from, annotations
    /// of types [PdfPageAnnotationType::Ink] and [PdfPageAnnotationType::Stamp]. All other annotation
    /// types are read-only.
    ///
    /// To gain access to the mutable collection of page objects inside an ink or stamp annotation,
    /// you must first unwrap the annotation, like so:
    /// ```
    /// # use pdfium_render::prelude::*;
    /// # fn example(annotation: &mut PdfPageAnnotation<'_>) {
    /// annotation.as_stamp_annotation_mut().unwrap().objects_mut();
    /// # }
    /// ```
    fn objects(&self) -> &PdfPageAnnotationObjects<'_>;

    /// Returns an immutable collection of the attachment points that visually associate
    /// this [PdfPageAnnotation] with one or more `PdfPageObject` objects on this `PdfPage`.
    ///
    /// This collection is provided for all annotation types, but it will always be empty
    /// if the annotation does not support attachment points. Pdfium supports attachment points
    /// for all markup annotations and the Link annotation, but not for any other annotation type.
    /// The [PdfPageAnnotationCommon::has_attachment_points()] function will return `true`
    /// if the annotation supports attachment points.
    ///
    /// To gain access to the mutable collection of attachment points inside a supported
    /// annotation, you must first unwrap the annotation, like so:
    /// ```
    /// # use pdfium_render::prelude::*;
    /// # fn example(annotation: &mut PdfPageAnnotation<'_>) {
    /// annotation.as_link_annotation_mut().unwrap().attachment_points_mut();
    /// # }
    /// ```
    fn attachment_points(&self) -> &PdfPageAnnotationAttachmentPoints<'_>;
}
impl<T: private::AnnotationOwner> PdfPageAnnotationCommon for T {
    fn name(&self) -> Option<String> {
        self.data().state.string(b"NM")
    }
    fn is_markup_annotation(&self) -> bool {
        sdk::has_attachment_points(&self.data().state.dict)
            && sdk::subtype(&self.data().state.dict) as i32 != 2
    }
    fn has_attachment_points(&self) -> bool {
        sdk::has_attachment_points(&self.data().state.dict)
    }
    fn bounds(&self) -> Result<PdfRect, PdfiumError> {
        self.data().state.check()?;
        Ok(PdfRect::from_core(sdk::rect(&self.data().state.dict)))
    }
    fn set_bounds(&mut self, bounds: PdfRect) -> Result<(), PdfiumError> {
        self.data().state.check()?;
        sdk::set_rect(&self.data().state.dict, bounds.as_core());
        self.data().state.set_string(
            b"M",
            &Utc::now().format("D:%Y%m%d%H%M%SZ00'00'").to_string(),
        )
    }
    fn set_position(&mut self, x: PdfPoints, y: PdfPoints) -> Result<(), PdfiumError> {
        let r = self.bounds()?;
        self.set_bounds(PdfRect::new(y, x, y + r.height(), x + r.width()))
    }
    fn set_width(&mut self, width: PdfPoints) -> Result<(), PdfiumError> {
        let r = self.bounds()?;
        self.set_bounds(PdfRect::new(
            r.bottom(),
            r.left(),
            r.top(),
            r.left() + width,
        ))
    }
    fn set_height(&mut self, width: PdfPoints) -> Result<(), PdfiumError> {
        let r = self.bounds()?;
        self.set_bounds(PdfRect::new(
            r.bottom(),
            r.left(),
            r.bottom() + width,
            r.right(),
        ))
    }
    fn contents(&self) -> Option<String> {
        self.data().state.string(b"Contents")
    }
    fn set_contents(&mut self, contents: &str) -> Result<(), PdfiumError> {
        self.data().state.set_string(b"Contents", contents)
    }
    fn creator(&self) -> Option<String> {
        self.data().state.string(b"T")
    }
    fn set_creator(&mut self, creator: &str) -> Result<(), PdfiumError> {
        self.data().state.set_string(b"T", creator)
    }
    fn creation_date(&self) -> Option<String> {
        self.data().state.string(b"CreationDate")
    }
    fn set_creation_date(&mut self, date: DateTime<Utc>) -> Result<(), PdfiumError> {
        self.data().state.set_string(
            b"CreationDate",
            &date.format("D:%Y%m%d%H%M%SZ00'00'").to_string(),
        )
    }
    fn modification_date(&self) -> Option<String> {
        self.data().state.string(b"M")
    }
    fn set_modification_date(&mut self, date: DateTime<Utc>) -> Result<(), PdfiumError> {
        self.data()
            .state
            .set_string(b"M", &date.format("D:%Y%m%d%H%M%SZ00'00'").to_string())
    }
    fn fill_color(&self) -> Result<PdfColor, PdfiumError> {
        self.data().state.check()?;
        let c = sdk::color(&self.data().state.dict, true)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        Ok(PdfColor::new(
            c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
        ))
    }
    fn set_fill_color(&mut self, fill_color: PdfColor) -> Result<(), PdfiumError> {
        self.data().state.check()?;
        crate::objects::boolean(sdk::set_color(
            &self.data().state.dict,
            true,
            crate::objects::rgba(fill_color),
        ))
    }
    fn stroke_color(&self) -> Result<PdfColor, PdfiumError> {
        self.data().state.check()?;
        let c = sdk::color(&self.data().state.dict, false)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        Ok(PdfColor::new(
            c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8,
        ))
    }
    fn set_stroke_color(&mut self, stroke_color: PdfColor) -> Result<(), PdfiumError> {
        self.data().state.check()?;
        crate::objects::boolean(sdk::set_color(
            &self.data().state.dict,
            false,
            crate::objects::rgba(stroke_color),
        ))
    }
    fn is_invisible_if_unsupported(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 0) != 0
    }
    fn set_is_invisible_if_unsupported(&mut self, is_invisible: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 0, is_invisible)
    }
    fn is_hidden(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 1) != 0
    }
    fn set_is_hidden(&mut self, is_hidden: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 1, is_hidden)
    }
    fn is_printed(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 2) != 0
    }
    fn set_is_printed(&mut self, is_printed: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 2, is_printed)
    }
    fn is_zoomable(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 3) == 0
    }
    fn set_is_zoomable(&mut self, is_zoomable: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 3, !is_zoomable)
    }
    fn is_rotatable(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 4) == 0
    }
    fn set_is_rotatable(&mut self, is_rotatable: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 4, !is_rotatable)
    }
    fn is_printable_but_not_viewable(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 5) != 0
    }
    fn set_is_printable_but_not_viewable(
        &mut self,
        is_printable_but_not_viewable: bool,
    ) -> Result<(), PdfiumError> {
        self.data()
            .state
            .set_flag(1 << 5, is_printable_but_not_viewable)
    }
    fn is_read_only(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 6) != 0
    }
    fn set_is_read_only(&mut self, is_read_only: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 6, is_read_only)
    }
    fn is_locked(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 7) != 0
    }
    fn set_is_locked(&mut self, is_locked: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 7, is_locked)
    }
    fn is_editable(&self) -> bool {
        sdk::flags(&self.data().state.dict) & (1 << 9) == 0
    }
    fn set_is_editable(&mut self, is_editable: bool) -> Result<(), PdfiumError> {
        self.data().state.set_flag(1 << 9, !is_editable)
    }
    fn objects(&self) -> &PdfPageAnnotationObjects<'_> {
        &self.data().objects
    }
    fn attachment_points(&self) -> &PdfPageAnnotationAttachmentPoints<'_> {
        &self.data().points
    }
}
impl<'a> PdfPageCircleAnnotation<'a> {}
impl<'a> PdfPageFreeTextAnnotation<'a> {}
impl<'a> PdfPageHighlightAnnotation<'a> {
    pub fn attachment_points_mut(&mut self) -> &mut PdfPageAnnotationAttachmentPoints<'a> {
        &mut self.data.points
    }
}
impl<'a> PdfPageInkAnnotation<'a> {
    pub fn objects_mut(&mut self) -> &mut PdfPageAnnotationObjects<'a> {
        &mut self.data.objects
    }
}
impl<'a> PdfPageLinkAnnotation<'a> {
    pub fn link(&self) -> Result<PdfLink<'_>, PdfiumError> {
        self.data.state.check()?;
        Ok(PdfLink::from_core(
            self.data.state.page.document.clone(),
            self.data.state.dict.clone(),
        ))
    }
    pub fn set_link(&mut self, uri: &str) -> Result<(), PdfiumError> {
        self.data.state.check()?;
        crate::objects::boolean(sdk::set_uri(&self.data.state.dict, uri.as_bytes()))
    }
    pub fn attachment_points_mut(&mut self) -> &mut PdfPageAnnotationAttachmentPoints<'a> {
        &mut self.data.points
    }
}
impl<'a> PdfPagePopupAnnotation<'a> {}
impl<'a> PdfPageSquareAnnotation<'a> {}
impl<'a> PdfPageSquigglyAnnotation<'a> {
    pub fn attachment_points_mut(&mut self) -> &mut PdfPageAnnotationAttachmentPoints<'a> {
        &mut self.data.points
    }
}
impl<'a> PdfPageStampAnnotation<'a> {
    pub fn objects_mut(&mut self) -> &mut PdfPageAnnotationObjects<'a> {
        &mut self.data.objects
    }
}
impl<'a> PdfPageStrikeoutAnnotation<'a> {
    pub fn attachment_points_mut(&mut self) -> &mut PdfPageAnnotationAttachmentPoints<'a> {
        &mut self.data.points
    }
}
impl<'a> PdfPageTextAnnotation<'a> {}
impl<'a> PdfPageUnderlineAnnotation<'a> {
    pub fn attachment_points_mut(&mut self) -> &mut PdfPageAnnotationAttachmentPoints<'a> {
        &mut self.data.points
    }
}
impl<'a> PdfPageWidgetAnnotation<'a> {
    pub fn form_field(&self) -> Option<&PdfFormField<'_>> {
        self.data.field.as_ref()
    }
    pub fn form_field_mut(&mut self) -> Option<&mut PdfFormField<'a>> {
        self.data.field.as_mut()
    }
}
impl<'a> PdfPageXfaWidgetAnnotation<'a> {
    pub fn form_field(&self) -> Option<&PdfFormField<'_>> {
        self.data.field.as_ref()
    }
    pub fn form_field_mut(&mut self) -> Option<&mut PdfFormField<'a>> {
        self.data.field.as_mut()
    }
}
impl<'a> PdfPageRedactedAnnotation<'a> {}
impl<'a> PdfPageUnsupportedAnnotation<'a> {
    pub fn get_type(&self) -> PdfPageAnnotationType {
        annotation_kind(sdk::subtype(&self.data.state.dict) as i32)
    }
}
fn annotation_kind(kind: i32) -> PdfPageAnnotationType {
    match kind {
        0 => PdfPageAnnotationType::Unknown,
        1 => PdfPageAnnotationType::Text,
        2 => PdfPageAnnotationType::Link,
        3 => PdfPageAnnotationType::FreeText,
        4 => PdfPageAnnotationType::Line,
        5 => PdfPageAnnotationType::Square,
        6 => PdfPageAnnotationType::Circle,
        7 => PdfPageAnnotationType::Polygon,
        8 => PdfPageAnnotationType::Polyline,
        9 => PdfPageAnnotationType::Highlight,
        10 => PdfPageAnnotationType::Underline,
        11 => PdfPageAnnotationType::Squiggly,
        12 => PdfPageAnnotationType::Strikeout,
        13 => PdfPageAnnotationType::Stamp,
        14 => PdfPageAnnotationType::Caret,
        15 => PdfPageAnnotationType::Ink,
        16 => PdfPageAnnotationType::Popup,
        17 => PdfPageAnnotationType::FileAttachment,
        18 => PdfPageAnnotationType::Sound,
        19 => PdfPageAnnotationType::Movie,
        20 => PdfPageAnnotationType::Widget,
        21 => PdfPageAnnotationType::Screen,
        22 => PdfPageAnnotationType::PrinterMark,
        23 => PdfPageAnnotationType::TrapNet,
        24 => PdfPageAnnotationType::Watermark,
        25 => PdfPageAnnotationType::ThreeD,
        26 => PdfPageAnnotationType::RichMedia,
        27 => PdfPageAnnotationType::XfaWidget,
        28 => PdfPageAnnotationType::Redacted,
        _ => PdfPageAnnotationType::Unknown,
    }
}
pub struct PdfPageAnnotationObjects<'a> {
    pub(crate) collection: ObjectCollection,
    state: Rc<AnnotationState>,
    lifetime: PhantomData<&'a ()>,
}
impl crate::object_collections::private::CollectionOwner for PdfPageAnnotationObjects<'_> {
    fn collection(&self) -> &ObjectCollection {
        let _ = self.state.check();
        &self.collection
    }
}
pub type PdfPageAnnotationAttachmentPointIndex = usize;
pub struct PdfPageAnnotationAttachmentPoints<'a> {
    state: Rc<AnnotationState>,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageAnnotationAttachmentPoints<'a> {
    pub fn len(&self) -> PdfPageAnnotationAttachmentPointIndex {
        sdk::attachment_points_count(&self.state.dict)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageAnnotationAttachmentPointIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfPageAnnotationAttachmentPointIndex> {
        if self.is_empty() {
            0..=0
        } else {
            0..=(self.len() - 1)
        }
    }
    pub fn get(
        &self,
        index: PdfPageAnnotationAttachmentPointIndex,
    ) -> Result<PdfQuadPoints, PdfiumError> {
        self.state.check()?;
        let p = sdk::attachment_points(&self.state.dict, index)
            .ok_or(PdfiumError::PageAnnotationAttachmentPointIndexOutOfBounds)?;
        Ok(PdfQuadPoints::new_from_values(
            p[0], p[1], p[2], p[3], p[4], p[5], p[6], p[7],
        ))
    }
    pub fn first(&self) -> Result<PdfQuadPoints, PdfiumError> {
        if !self.is_empty() {
            self.get(0)
        } else {
            Err(PdfiumError::NoAttachmentPointsInPageAnnotation)
        }
    }
    pub fn last(&self) -> Result<PdfQuadPoints, PdfiumError> {
        if !self.is_empty() {
            self.get(self.len() - 1)
        } else {
            Err(PdfiumError::NoAttachmentPointsInPageAnnotation)
        }
    }
    pub fn create_attachment_point_at_end(
        &mut self,
        attachment_point: PdfQuadPoints,
    ) -> Result<(), PdfiumError> {
        self.state.check()?;
        crate::objects::boolean(sdk::set_attachment_points(
            &self.state.dict,
            None,
            [
                attachment_point.x1().value,
                attachment_point.y1().value,
                attachment_point.x2().value,
                attachment_point.y2().value,
                attachment_point.x3().value,
                attachment_point.y3().value,
                attachment_point.x4().value,
                attachment_point.y4().value,
            ],
        ))
    }
    pub fn set_attachment_point_at_index(
        &mut self,
        index: PdfPageAnnotationAttachmentPointIndex,
        attachment_point: PdfQuadPoints,
    ) -> Result<(), PdfiumError> {
        self.state.check()?;
        crate::objects::boolean(sdk::set_attachment_points(
            &self.state.dict,
            Some(index),
            [
                attachment_point.x1().value,
                attachment_point.y1().value,
                attachment_point.x2().value,
                attachment_point.y2().value,
                attachment_point.x3().value,
                attachment_point.y3().value,
                attachment_point.x4().value,
                attachment_point.y4().value,
            ],
        ))
    }
    pub fn iter(&self) -> PdfPageAnnotationAttachmentPointsIterator<'_> {
        PdfPageAnnotationAttachmentPointsIterator {
            points: self,
            next_index: 0,
        }
    }
}
pub struct PdfPageAnnotationAttachmentPointsIterator<'a> {
    points: &'a PdfPageAnnotationAttachmentPoints<'a>,
    next_index: usize,
}
impl Iterator for PdfPageAnnotationAttachmentPointsIterator<'_> {
    type Item = PdfQuadPoints;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.points.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.points.get(i).ok()
    }
}
pub type PdfPageAnnotationIndex = usize;
pub struct PdfPageAnnotations<'a> {
    page: backend::PdfPage,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPageAnnotations<'a> {
    pub(crate) fn new(page: backend::PdfPage) -> Self {
        Self {
            page,
            lifetime: PhantomData,
        }
    }
    fn create_data(&self, kind: PdfPageAnnotationType) -> Result<AnnotationData<'a>, PdfiumError> {
        let dict = self.page.edit(|p| {
            sdk::create_annotation(p, kind as i32)
                .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        })?;
        let index = sdk::annotation_count(&self.page.state.core.borrow())
            .checked_sub(1)
            .ok_or(PdfiumError::PageAnnotationIndexOutOfBounds)?;
        let data = AnnotationData::from_core(self.page.alias(), dict, index);
        data.state.set_string(
            b"CreationDate",
            &Utc::now().format("D:%Y%m%d%H%M%SZ00'00'").to_string(),
        )?;
        Ok(data)
    }
}
impl<'a> PdfPageAnnotations<'a> {
    pub fn len(&self) -> PdfPageAnnotationIndex {
        sdk::annotation_count(&self.page.state.core.borrow())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfPageAnnotationIndex> {
        0..self.len()
    }
    pub fn get(&self, index: PdfPageAnnotationIndex) -> Result<PdfPageAnnotation<'a>, PdfiumError> {
        self.page.check_live()?;
        let dict = sdk::annotation_at(&self.page.state.core.borrow(), index)
            .ok_or(PdfiumError::PageAnnotationIndexOutOfBounds)?;
        Ok(PdfPageAnnotation::from_core(self.page.alias(), dict, index))
    }
    pub fn first(&self) -> Result<PdfPageAnnotation<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(0)
        } else {
            Err(PdfiumError::NoAnnotationsInCollection)
        }
    }
    pub fn last(&self) -> Result<PdfPageAnnotation<'a>, PdfiumError> {
        if !self.is_empty() {
            self.get(self.len() - 1)
        } else {
            Err(PdfiumError::NoAnnotationsInCollection)
        }
    }
    pub fn iter(&self) -> PdfPageAnnotationsIterator<'_> {
        PdfPageAnnotationsIterator {
            annotations: self,
            next_index: 0,
        }
    }
    pub fn create_free_text_annotation(
        &mut self,
        text: &str,
    ) -> Result<PdfPageFreeTextAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::FreeText)?;
        let mut annotation = PdfPageFreeTextAnnotation { data };
        annotation.set_contents(text)?;
        Ok(annotation)
    }
    pub fn create_highlight_annotation(
        &mut self,
    ) -> Result<PdfPageHighlightAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Highlight)?;
        let annotation = PdfPageHighlightAnnotation { data };
        Ok(annotation)
    }
    pub fn create_ink_annotation(&mut self) -> Result<PdfPageInkAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Ink)?;
        let annotation = PdfPageInkAnnotation { data };
        Ok(annotation)
    }
    pub fn create_link_annotation(
        &mut self,
        uri: &str,
    ) -> Result<PdfPageLinkAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Link)?;
        let mut annotation = PdfPageLinkAnnotation { data };
        annotation.set_link(uri)?;
        Ok(annotation)
    }
    pub fn create_popup_annotation(&mut self) -> Result<PdfPagePopupAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Popup)?;
        let annotation = PdfPagePopupAnnotation { data };
        Ok(annotation)
    }
    pub fn create_square_annotation(&mut self) -> Result<PdfPageSquareAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Square)?;
        let annotation = PdfPageSquareAnnotation { data };
        Ok(annotation)
    }
    pub fn create_squiggly_annotation(
        &mut self,
    ) -> Result<PdfPageSquigglyAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Squiggly)?;
        let annotation = PdfPageSquigglyAnnotation { data };
        Ok(annotation)
    }
    pub fn create_stamp_annotation(&mut self) -> Result<PdfPageStampAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Stamp)?;
        let annotation = PdfPageStampAnnotation { data };
        Ok(annotation)
    }
    pub fn create_strikeout_annotation(
        &mut self,
    ) -> Result<PdfPageStrikeoutAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Strikeout)?;
        let annotation = PdfPageStrikeoutAnnotation { data };
        Ok(annotation)
    }
    pub fn create_text_annotation(
        &mut self,
        text: &str,
    ) -> Result<PdfPageTextAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Text)?;
        let mut annotation = PdfPageTextAnnotation { data };
        annotation.set_contents(text)?;
        Ok(annotation)
    }
    pub fn create_underline_annotation(
        &mut self,
    ) -> Result<PdfPageUnderlineAnnotation<'a>, PdfiumError> {
        let data = self.create_data(PdfPageAnnotationType::Underline)?;
        let annotation = PdfPageUnderlineAnnotation { data };
        Ok(annotation)
    }
    pub fn create_squiggly_annotation_under_object(
        &mut self,
        object: &PdfPageObject,
        color: PdfColor,
        contents: Option<&str>,
    ) -> Result<PdfPageSquigglyAnnotation<'a>, PdfiumError> {
        let mut annotation = self.create_squiggly_annotation()?;

        // The annotation will not display if it is not positioned.

        let bounds = object.bounds()?;

        annotation.set_position(bounds.left(), bounds.bottom())?;
        annotation.set_stroke_color(color)?;

        const SQUIGGLY_HEIGHT: f32 = 12.0;

        let annotation_top = bounds.bottom().value - 5.0;
        let annotation_bottom = annotation_top - SQUIGGLY_HEIGHT;

        annotation
            .attachment_points_mut()
            .create_attachment_point_at_end(PdfQuadPoints::new_from_values(
                bounds.left().value,
                annotation_bottom,
                bounds.right().value,
                annotation_bottom,
                bounds.right().value,
                annotation_top,
                bounds.left().value,
                annotation_top,
            ))?;

        if let Some(contents) = contents {
            annotation.set_width(bounds.width())?;
            annotation.set_height(bounds.height())?;
            annotation.set_contents(contents)?;
        }

        Ok(annotation)
    }
    pub fn create_underline_annotation_under_object(
        &mut self,
        object: &PdfPageObject,
        color: PdfColor,
        contents: Option<&str>,
    ) -> Result<PdfPageUnderlineAnnotation<'a>, PdfiumError> {
        let mut annotation = self.create_underline_annotation()?;

        // The annotation will not display if it is not positioned.

        let bounds = object.bounds()?;

        annotation.set_position(bounds.left(), bounds.bottom())?;
        annotation.set_stroke_color(color)?;
        annotation
            .attachment_points_mut()
            .create_attachment_point_at_end(bounds)?;

        if let Some(contents) = contents {
            annotation.set_width(bounds.width())?;
            annotation.set_height(bounds.height())?;
            annotation.set_contents(contents)?;
        }

        Ok(annotation)
    }
    pub fn create_strikeout_annotation_through_object(
        &mut self,
        object: &PdfPageObject,
        color: PdfColor,
        contents: Option<&str>,
    ) -> Result<PdfPageStrikeoutAnnotation<'a>, PdfiumError> {
        let mut annotation = self.create_strikeout_annotation()?;

        // The annotation will not display if it is not positioned.

        let bounds = object.bounds()?;

        annotation.set_position(bounds.left(), bounds.bottom())?;
        annotation.set_stroke_color(color)?;
        annotation
            .attachment_points_mut()
            .create_attachment_point_at_end(bounds)?;

        if let Some(contents) = contents {
            annotation.set_width(bounds.width())?;
            annotation.set_height(bounds.height())?;
            annotation.set_contents(contents)?;
        }

        Ok(annotation)
    }
    pub fn create_highlight_annotation_over_object(
        &mut self,
        object: &PdfPageObject,
        color: PdfColor,
        contents: Option<&str>,
    ) -> Result<PdfPageHighlightAnnotation<'a>, PdfiumError> {
        let mut annotation = self.create_highlight_annotation()?;

        // The annotation will not display if it is not positioned.

        let bounds = object.bounds()?;

        annotation.set_position(bounds.left(), bounds.bottom())?;
        annotation.set_stroke_color(color)?;
        annotation
            .attachment_points_mut()
            .create_attachment_point_at_end(bounds)?;

        if let Some(contents) = contents {
            annotation.set_width(bounds.width())?;
            annotation.set_height(bounds.height())?;
            annotation.set_contents(contents)?;
        }

        Ok(annotation)
    }
    pub fn delete_annotation(
        &mut self,
        annotation: PdfPageAnnotation<'a>,
    ) -> Result<(), PdfiumError> {
        let data = private::AnnotationOwner::data(&annotation);
        let index = sdk::annotation_index(&self.page.state.core.borrow(), &data.state.dict)
            .ok_or(PdfiumError::PageAnnotationIndexOutOfBounds)?;
        crate::objects::boolean(sdk::remove_annotation(
            &self.page.state.core.borrow(),
            index,
        ))
    }
}
pub struct PdfPageAnnotationsIterator<'a> {
    annotations: &'a PdfPageAnnotations<'a>,
    next_index: usize,
}
impl<'a> Iterator for PdfPageAnnotationsIterator<'a> {
    type Item = PdfPageAnnotation<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.annotations.len() {
            return None;
        }
        let i = self.next_index;
        self.next_index += 1;
        self.annotations.get(i).ok()
    }
}

#[cfg(all(test, feature = "image_api"))]
impl PdfPageWidgetAnnotation<'_> {
    pub(crate) fn get_flags_impl(&self) -> backend::annotations::PdfAnnotationFlags {
        backend::annotations::PdfAnnotationFlags(sdk::flags(&self.data.state.dict) as u32)
    }
    pub(crate) fn set_flags_impl(
        &mut self,
        flags: backend::annotations::PdfAnnotationFlags,
    ) -> bool {
        if self.data.state.check().is_err() {
            return false;
        }
        sdk::set_flags(&self.data.state.dict, flags.0 as i32);
        true
    }
}
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum PdfPageAnnotationVariableTextJustification {
    LeftJustified,
    Centered,
    RightJustified,
}
pub trait PdfPageAnnotationVariableText<'a> {
    /// Returns the size of the text in this annotation. A value of [PdfPoints::ZERO]
    /// indicates that the font size is determined automatically from the annotation height.
    /// See also the [PdfPageAnnotationVariableText::is_font_auto_sized()] function.
    fn font_size(&self, form: &PdfForm) -> Result<PdfPoints, PdfiumError>;

    /// Returns `true` if the font size for this annotation is determined automatically
    /// from the annotation height.
    fn is_font_auto_sized(&self, form: &PdfForm) -> bool;

    /// Returns the color of the text in this annotation.
    fn font_color(&self, form: &PdfForm) -> Result<PdfColor, PdfiumError>;

    /// Sets the color of the text in this annotation.
    fn set_font_color(&mut self, form: &PdfForm, color: PdfColor) -> Result<(), PdfiumError>;

    /// Returns the form of justification that should be used when displaying the text
    /// assigned to this annotation.
    fn justification(&self) -> Result<PdfPageAnnotationVariableTextJustification, PdfiumError>;

    /// Returns the rich text string assigned to this annotation, if any.
    ///
    /// Rich text support was added in PDF version 1.5.
    fn rich_text(&self) -> Option<String>;
}
impl AnnotationState {
    fn appearance(
        &self,
        form: &PdfForm,
    ) -> Result<pdfoxium_core::fpdfdoc::cpdf_defaultappearance::CpdfDefaultAppearance, PdfiumError>
    {
        self.check()?;
        if !Rc::ptr_eq(&self.page.document, &form.document) {
            return Err(PdfiumError::OwnershipNotAttachedToDocument);
        }
        use pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
        let root = form.document.core.borrow().root();
        let acro = root.and_then(|r| {
            r.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .and_then(|d| d.get_dict_for(b"AcroForm"))
        });
        let annot = self.dict.borrow();
        let annot = annot
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .ok_or(PdfiumError::UnknownPdfAnnotationType)?;
        let acro = acro.as_ref().map(|r| r.borrow());
        let acro = acro
            .as_ref()
            .and_then(|r| r.as_any().downcast_ref::<CpdfDictionary>());
        Ok(
            pdfoxium_core::fpdfdoc::cpdf_defaultappearance::CpdfDefaultAppearance::from_dicts(
                Some(annot),
                acro,
            ),
        )
    }
}
impl<'a, T: private::AnnotationOwner> PdfPageAnnotationVariableText<'a> for T {
    fn font_size(&self, form: &PdfForm) -> Result<PdfPoints, PdfiumError> {
        let state = &self.data().state;
        state.check()?;
        if !Rc::ptr_eq(&state.page.document, &form.document) {
            return Err(PdfiumError::OwnershipNotAttachedToDocument);
        }
        let f = pdfoxium_core::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm::new(
            &form.document.core,
        );
        f.fix_page_fields(&state.page.state.core.borrow());
        let control = f
            .get_control_by_dict(&state.dict)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        Ok(PdfPoints::new(
            control.default_appearance().font_size_or_zero(),
        ))
    }
    fn is_font_auto_sized(&self, form: &PdfForm) -> bool {
        self.font_size(form).is_ok_and(|p| p.value == 0.)
    }
    fn font_color(&self, form: &PdfForm) -> Result<PdfColor, PdfiumError> {
        let state = &self.data().state;
        let subtype = sdk::subtype(&state.dict) as i32;
        if subtype != 3 && subtype != 20 {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        let argb = state
            .appearance(form)?
            .color_argb()
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?
            .argb;
        Ok(PdfColor::new(
            (argb >> 16) as u8,
            (argb >> 8) as u8,
            argb as u8,
            255,
        ))
    }
    fn set_font_color(&mut self, form: &PdfForm, color: PdfColor) -> Result<(), PdfiumError> {
        let state = &self.data().state;
        state.check()?;
        if !Rc::ptr_eq(&state.page.document, &form.document) {
            return Err(PdfiumError::OwnershipNotAttachedToDocument);
        }
        if sdk::subtype(&state.dict) as i32 != 3 {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        let (objects, root, data) = {
            let d = form.document.core.borrow();
            (d.holder(), d.root(), d.page_data_ref())
        };
        let c = pdfoxium_core::fxge::cfx_color::CfxColor::make_rgb(
            color.red() as f32 / 255.,
            color.green() as f32 / 255.,
            color.blue() as f32 / 255.,
        );
        if !pdfoxium_core::fpdfdoc::cpdf_generateap::generate_default_appearance_with_color_for_object(&objects,root.as_ref(),&data,&state.dict,c){return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);}
        state
            .dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<pdfoxium_core::fpdfapi::parser::cpdf_dictionary::CpdfDictionary>()
            .ok_or(PdfiumError::UnknownPdfAnnotationType)?
            .remove_for(b"AP");
        state
            .appearance_epoch
            .set(state.appearance_epoch.get().wrapping_add(1));
        *state.context.borrow_mut() = CpdfAnnotContext::new(state.dict.clone());
        Ok(())
    }
    fn justification(&self) -> Result<PdfPageAnnotationVariableTextJustification, PdfiumError> {
        self.data().state.check()?;
        match sdk::number(&self.data().state.dict, b"Q")
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)? as i32
        {
            0 => Ok(PdfPageAnnotationVariableTextJustification::LeftJustified),
            1 => Ok(PdfPageAnnotationVariableTextJustification::Centered),
            2 => Ok(PdfPageAnnotationVariableTextJustification::RightJustified),
            _ => Err(PdfiumError::UnknownPageAnnotationVariableTextJustificationType),
        }
    }
    fn rich_text(&self) -> Option<String> {
        self.data().state.string(b"RV")
    }
}
