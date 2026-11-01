// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Owned document information and lazy embedded-file access. Coordinates use
//! unscaled PDF page units, matching PDFium; `/UserUnit` is not applied.

use crate::{Document, Error, Page, PageId, Rect, Result, Rotation, Size};
use pdfoxium_core::{
    fpdfapi::{
        page::cpdf_page::CpdfPage,
        parser::{
            cpdf_array::CpdfArray,
            cpdf_dictionary::CpdfDictionary,
            cpdf_document::CpdfDocument,
            cpdf_object::{get_direct_object, ObjectRef},
            cpdf_scalar_object::ObjectType,
        },
    },
    fpdfdoc::{
        cpdf_action::{ActionType, CpdfAction},
        cpdf_bookmark::CpdfBookmark,
        cpdf_dest::CpdfDest,
        cpdf_pagelabel::CpdfPageLabel,
    },
    fpdfsdk::{fpdf_attachment as attachment, fpdf_view},
};
use std::{collections::HashSet, io::Write, rc::Rc};

/// Standard document information entries. PDF date strings are preserved as
/// written, since incomplete dates and explicit UTC offsets are meaningful.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    pub creation_date: Option<String>,
    pub modification_date: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Permissions {
    /// PDF permission bits as reported for the credentials used to open the file.
    pub bits: u32,
}
impl Permissions {
    pub fn can_print(self) -> bool {
        self.bits & (1 << 2) != 0
    }
    pub fn can_modify(self) -> bool {
        self.bits & (1 << 3) != 0
    }
    pub fn can_copy(self) -> bool {
        self.bits & (1 << 4) != 0
    }
    pub fn can_annotate(self) -> bool {
        self.bits & (1 << 5) != 0
    }
    pub fn can_fill_forms(self) -> bool {
        self.bits & (1 << 8) != 0
    }
    pub fn can_assemble(self) -> bool {
        self.bits & (1 << 10) != 0
    }
    pub fn can_print_high_quality(self) -> bool {
        self.bits & (1 << 11) != 0
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormType {
    None,
    AcroForm,
    XfaFull,
    XfaForeground,
    Unknown(i32),
}

/// Dictionary-only layout; reading it does not parse page content. `size`
/// includes intrinsic page rotation; `crop` remains in page coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct PageLayout {
    pub size: Size,
    pub rotation: Rotation,
    pub crop: Rect,
    pub label: Option<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Destination {
    pub page_index: usize,
    pub view: DestinationView,
}
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum DestinationView {
    Xyz {
        left: Option<f32>,
        top: Option<f32>,
        zoom: Option<f32>,
    },
    Fit,
    FitH {
        top: Option<f32>,
    },
    FitV {
        left: Option<f32>,
    },
    FitR {
        left: Option<f32>,
        bottom: Option<f32>,
        right: Option<f32>,
        top: Option<f32>,
    },
    FitB,
    FitBH {
        top: Option<f32>,
    },
    FitBV {
        left: Option<f32>,
    },
    Unknown {
        name: Vec<u8>,
        parameters: Vec<Option<f32>>,
    },
}
/// Navigation data only: none of these actions is executed by this API.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum LinkTarget {
    Destination(Destination),
    NamedDestination {
        name: Vec<u8>,
        resolved: Option<Destination>,
    },
    /// Original URI bytes (including the catalog's base URI when applicable).
    Uri(Vec<u8>),
    NamedAction(Vec<u8>),
    OptionalContent(crate::OptionalContentAction),
    /// Action kind and referenced file, when present. Other action payloads are
    /// not interpreted; callers must not treat this as an executable action.
    UnsupportedAction {
        kind: Vec<u8>,
        file: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub struct Link {
    pub annotation_index: usize,
    pub rect: Rect,
    pub target: Option<LinkTarget>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct AnnotationInfo {
    pub annotation_index: usize,
    pub subtype: Vec<u8>,
    pub rect: Rect,
    pub flags: i32,
    pub contents: Option<String>,
}
/// Flat preorder outline, with zero-based depth; avoids recursive ownership
/// even for extremely deeply nested documents. Cyclic/repeated items are skipped.
#[derive(Clone, Debug, PartialEq)]
pub struct OutlineItem {
    pub depth: usize,
    pub title: String,
    pub target: Option<LinkTarget>,
    pub color: Option<[f32; 3]>,
    pub bold: bool,
    pub italic: bool,
    pub open: bool,
}

/// Owned read-only interaction snapshot. Text is requested separately with
/// `Page::text()`, so building this model does not parse page content.
#[derive(Clone, Debug, PartialEq)]
pub struct PageModel {
    pub id: PageId,
    pub index: usize,
    /// Document revision at extraction; imports can change page indices/layouts.
    pub revision: u64,
    pub layout: PageLayout,
    pub links: Vec<Link>,
    pub annotations: Vec<AnnotationInfo>,
}
/// Owned document navigation/layout snapshot. Building it visits page
/// dictionaries but does not parse all page contents or decode attachments.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentModel {
    pub revision: u64,
    pub metadata: Metadata,
    pub permissions: Permissions,
    pub form_type: FormType,
    pub pages: Vec<PageLayout>,
    pub outline: Vec<OutlineItem>,
}

/// Lazy attachment handle. Retains the document, but listing attachments does
/// not decode their payloads. The embedded name is metadata, not a safe path.
#[derive(Clone)]
pub struct Attachment {
    document: Document,
    file: ObjectRef,
    name: String,
    description: String,
}
impl std::fmt::Debug for Attachment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Attachment")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}
impl Attachment {
    pub(crate) fn from_core(document: Document, file: ObjectRef) -> Self {
        Self {
            name: text(&attachment::name(&file)),
            description: text(&attachment::description(&file).unwrap_or_default()),
            document,
            file,
        }
    }
    /// Replace the embedded payload. The optional PDF date string is supplied by the caller.
    pub fn set_data(&self, bytes: &[u8], creation_date: Option<&str>) -> Result<()> {
        if bytes.len() > i32::MAX as usize {
            return Err(Error::InvalidArgument("attachment exceeds PDF stream size"));
        }
        self.document.with_structure_edit(|core| {
            let holder = core.borrow().holder();
            attachment::set_file(
                &self.file,
                &holder,
                bytes,
                creation_date.unwrap_or("").as_bytes(),
            )
            .then_some(())
            .ok_or(Error::Edit)
        })
    }
    pub fn set_description(&mut self, value: &str) -> Result<()> {
        self.document.with_structure_edit(|_| {
            attachment::set_description(
                &self.file,
                &value.chars().map(u32::from).collect::<Vec<_>>(),
            )
            .then_some(())
            .ok_or(Error::Edit)
        })?;
        self.description = value.to_owned();
        Ok(())
    }
    pub fn parameter(&self, key: &str) -> Result<Option<String>> {
        self.document.with_core(|_| {
            Ok(attachment::string_value(&self.file, key.as_bytes()).map(|v| text(&v)))
        })
    }
    pub fn set_parameter(&self, key: &str, value: &str) -> Result<()> {
        if key.is_empty() || key.contains('\0') {
            return Err(Error::InvalidArgument("invalid attachment key"));
        }
        self.document.with_structure_edit(|_| {
            attachment::set_string_value(
                &self.file,
                key.as_bytes(),
                &value.chars().map(u32::from).collect::<Vec<_>>(),
            )
            .then_some(())
            .ok_or(Error::Edit)
        })
    }
    pub fn subtype(&self) -> Result<String> {
        self.document
            .with_core(|_| Ok(text(&attachment::subtype(&self.file))))
    }

    pub fn name(&self) -> &str {
        &self.name
    }
    /// Description snapshot from listing or this handle's last setter.
    /// Use `current_description` to observe edits through another handle.
    pub fn description(&self) -> &str {
        &self.description
    }
    pub fn current_description(&self) -> Result<String> {
        self.document.with_core(|_| {
            Ok(text(
                &attachment::description(&self.file).unwrap_or_default(),
            ))
        })
    }
    pub fn has_parameter(&self, key: &str) -> Result<bool> {
        self.document
            .with_core(|_| Ok(attachment::has_key(&self.file, key.as_bytes())))
    }
    pub fn parameter_type(&self, key: &str) -> Result<Option<crate::PdfValueType>> {
        self.document.with_core(|_| {
            Ok(
                attachment::value_type(&self.file, key.as_bytes()).map(|kind| match kind {
                    ObjectType::Boolean => crate::PdfValueType::Boolean,
                    ObjectType::Number => crate::PdfValueType::Number,
                    ObjectType::String => crate::PdfValueType::String,
                    ObjectType::Name => crate::PdfValueType::Name,
                    ObjectType::Array => crate::PdfValueType::Array,
                    ObjectType::Dictionary => crate::PdfValueType::Dictionary,
                    ObjectType::Stream => crate::PdfValueType::Stream,
                    ObjectType::Null => crate::PdfValueType::Null,
                    ObjectType::Reference => crate::PdfValueType::Reference,
                }),
            )
        })
    }

    /// Decode the embedded payload into an owned byte vector.
    pub fn read(&self) -> Result<Vec<u8>> {
        self.document.with_core(|_| {
            attachment::with_file(&self.file, |bytes| bytes.to_vec()).ok_or(Error::Format)
        })
    }
    /// Decode and write the payload. The core guard remains active across writer
    /// callbacks, so attempts to reenter this document return `Busy`.
    pub fn save_to(&self, writer: &mut impl Write) -> Result<()> {
        self.document.with_core(|_| {
            attachment::with_file(&self.file, |bytes| writer.write_all(bytes))
                .ok_or(Error::Format)?
                .map_err(Error::Io)
        })
    }
}

fn entry(object: &ObjectRef, name: &[u8]) -> Option<ObjectRef> {
    let value = object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(name)?;
    get_direct_object(&value)
}
fn array_items(object: &ObjectRef) -> Vec<(usize, ObjectRef)> {
    let values = {
        let borrowed = object.borrow();
        let Some(array) = borrowed.as_any().downcast_ref::<CpdfArray>() else {
            return Vec::new();
        };
        (0..array.size())
            .map(|i| array.get_object_at(i))
            .collect::<Vec<_>>()
    };
    values
        .iter()
        .enumerate()
        .filter_map(|(index, value)| {
            value
                .as_ref()
                .and_then(get_direct_object)
                .map(|value| (index, value))
        })
        .collect()
}
fn rect(value: pdfoxium_core::fxcrt::fx_coordinates::CfxFloatRect) -> Rect {
    Rect {
        left: value.left,
        bottom: value.bottom,
        right: value.right,
        top: value.top,
    }
}
/// Decode mixed full scalars/UTF-16 units used by PDF text helpers. Invalid
/// sequences become U+FFFD rather than producing invalid Rust strings.
fn text(values: &[u32]) -> String {
    let mut output = String::new();
    let mut index = 0;
    while index < values.len() {
        let mut value = values[index];
        index += 1;
        if (0xd800..=0xdbff).contains(&value)
            && values
                .get(index)
                .is_some_and(|v| (0xdc00..=0xdfff).contains(v))
        {
            value = 0x10000 + ((value - 0xd800) << 10) + values[index] - 0xdc00;
            index += 1;
        }
        output.push(char::from_u32(value).unwrap_or(char::REPLACEMENT_CHARACTER));
    }
    output
}
fn text_entry(object: &ObjectRef, key: &[u8]) -> Option<String> {
    let value = entry(object, key)?;
    let borrowed = value.borrow();
    matches!(
        borrowed.object_type(),
        ObjectType::String | ObjectType::Name
    )
    .then(|| text(&borrowed.get_unicode_text()))
}
fn destination(doc: &mut CpdfDocument, dest: CpdfDest) -> Option<Destination> {
    let array = dest.array()?;
    let index = usize::try_from(dest.dest_page_index(doc)).ok()?;
    if index >= doc.page_count() {
        return None;
    }
    // Preserve null/missing values rather than using CpdfDest::param's zero fallback.
    let entry_at = |index| {
        let object = array
            .borrow()
            .as_any()
            .downcast_ref::<CpdfArray>()?
            .get_object_at(index)?;
        get_direct_object(&object)
    };
    let number = |index| {
        entry_at(index).and_then(|value| {
            let borrowed = value.borrow();
            (borrowed.object_type() == ObjectType::Number).then(|| borrowed.get_number())
        })
    };
    let name = entry_at(1).map_or_else(Vec::new, |value| value.borrow().get_string());
    let view = match name.as_slice() {
        b"XYZ" => DestinationView::Xyz {
            left: number(2),
            top: number(3),
            zoom: number(4).filter(|v| *v != 0.0),
        },
        b"Fit" => DestinationView::Fit,
        b"FitH" => DestinationView::FitH { top: number(2) },
        b"FitV" => DestinationView::FitV { left: number(2) },
        b"FitR" => DestinationView::FitR {
            left: number(2),
            bottom: number(3),
            right: number(4),
            top: number(5),
        },
        b"FitB" => DestinationView::FitB,
        b"FitBH" => DestinationView::FitBH { top: number(2) },
        b"FitBV" => DestinationView::FitBV { left: number(2) },
        _ => {
            let count = array.borrow().as_any().downcast_ref::<CpdfArray>()?.size();
            DestinationView::Unknown {
                name,
                parameters: (2..count).map(number).collect(),
            }
        }
    };
    Some(Destination {
        page_index: index,
        view,
    })
}
fn destination_target(doc: &mut CpdfDocument, value: ObjectRef) -> Option<LinkTarget> {
    let kind = value.borrow().object_type();
    let resolved = destination(doc, CpdfDest::create(doc, Some(value.clone())));
    if matches!(kind, ObjectType::String | ObjectType::Name) {
        Some(LinkTarget::NamedDestination {
            name: value.borrow().get_string(),
            resolved,
        })
    } else {
        resolved.map(LinkTarget::Destination)
    }
}
fn target(doc: &mut CpdfDocument, dictionary: &ObjectRef) -> Option<LinkTarget> {
    if let Some(dest) = entry(dictionary, b"Dest") {
        return destination_target(doc, dest);
    }
    let action = CpdfAction::new(entry(dictionary, b"A"));
    let dict = action.dict()?;
    Some(match action.action_type() {
        ActionType::GoTo => {
            return entry(&dict, b"D").and_then(|dest| destination_target(doc, dest))
        }
        ActionType::Uri => LinkTarget::Uri(action.uri(doc)),
        ActionType::Named => LinkTarget::NamedAction(action.named_action()),
        ActionType::SetOcgState => LinkTarget::OptionalContent(
            pdfoxium_core::fpdfapi::page::optional_content::parse_action(&dict)?,
        ),
        _ => {
            let file = action.file_path();
            LinkTarget::UnsupportedAction {
                kind: entry(&dict, b"S").map_or_else(Vec::new, |value| value.borrow().get_string()),
                file: (!file.is_empty()).then(|| text(&file)),
            }
        }
    })
}
fn layout(doc: &mut CpdfDocument, index: usize) -> Result<PageLayout> {
    let count = doc.page_count();
    if index >= count {
        return Err(Error::PageOutOfRange { index, count });
    }
    let page = CpdfPage::from_document(doc, index).ok_or(Error::Format)?;
    Ok(PageLayout {
        size: Size {
            width: page.width(),
            height: page.height(),
        },
        rotation: match page.rotation() {
            1 => Rotation::Cw90,
            2 => Rotation::Cw180,
            3 => Rotation::Cw270,
            _ => Rotation::None,
        },
        crop: rect(page.bbox()),
        label: i32::try_from(index)
            .ok()
            .and_then(|i| CpdfPageLabel::new(Some(doc)).get_label(i))
            .map(|v| text(&v)),
    })
}
impl Document {
    pub fn model(&self) -> Result<DocumentModel> {
        Ok(DocumentModel {
            revision: self.revision(),
            metadata: self.metadata()?,
            permissions: self.permissions()?,
            form_type: self.form_type()?,
            pages: self.page_layouts()?,
            outline: self.outline()?,
        })
    }
    pub fn metadata(&self) -> Result<Metadata> {
        self.with_core(|core| {
            let Some(info) = core.borrow_mut().info() else {
                return Ok(Metadata::default());
            };
            Ok(Metadata {
                title: text_entry(&info, b"Title"),
                author: text_entry(&info, b"Author"),
                subject: text_entry(&info, b"Subject"),
                keywords: text_entry(&info, b"Keywords"),
                creator: text_entry(&info, b"Creator"),
                producer: text_entry(&info, b"Producer"),
                creation_date: text_entry(&info, b"CreationDate"),
                modification_date: text_entry(&info, b"ModDate"),
            })
        })
    }
    pub fn permissions(&self) -> Result<Permissions> {
        self.with_core(|core| {
            Ok(Permissions {
                bits: core.borrow().user_permissions(true),
            })
        })
    }
    pub fn form_type(&self) -> Result<FormType> {
        self.with_core(|core| {
            Ok(match fpdf_view::form_type(core.borrow().root().as_ref()) {
                0 => FormType::None,
                1 => FormType::AcroForm,
                2 => FormType::XfaFull,
                3 => FormType::XfaForeground,
                n => FormType::Unknown(n),
            })
        })
    }
    /// Query one page's layout without loading its content.
    pub fn page_layout(&self, index: usize) -> Result<PageLayout> {
        if let Some(form) = self.active_forms().filter(|f| f.is_full_xfa()) {
            let sizes = form.page_sizes()?;
            let count = sizes.len();
            let size = *sizes
                .get(index)
                .ok_or(Error::PageOutOfRange { index, count })?;
            return Ok(PageLayout {
                size,
                rotation: Rotation::None,
                crop: Rect {
                    left: 0.,
                    bottom: 0.,
                    right: size.width,
                    top: size.height,
                },
                label: None,
            });
        }

        self.with_core(|core| layout(&mut core.borrow_mut(), index))
    }
    /// Query one page's read-only interaction model without loading its content.
    pub fn page_model(&self, index: usize) -> Result<PageModel> {
        self.page_handle(index)?.model()
    }
    pub fn page_layouts(&self) -> Result<Vec<PageLayout>> {
        if let Some(form) = self.active_forms().filter(|f| f.is_full_xfa()) {
            return form.page_sizes().map(|sizes| {
                sizes
                    .into_iter()
                    .map(|size| PageLayout {
                        size,
                        rotation: Rotation::None,
                        crop: Rect {
                            left: 0.,
                            bottom: 0.,
                            right: size.width,
                            top: size.height,
                        },
                        label: None,
                    })
                    .collect()
            });
        }

        self.with_core(|core| {
            let mut doc = core.borrow_mut();
            (0..doc.page_count()).map(|i| layout(&mut doc, i)).collect()
        })
    }
    pub fn outline(&self) -> Result<Vec<OutlineItem>> {
        self.with_core(|core| {
            let mut doc = core.borrow_mut();
            let first = doc
                .root()
                .and_then(|root| entry(&root, b"Outlines"))
                .and_then(|outline| entry(&outline, b"First"));
            let mut pending = first.map(|object| vec![(object, 0)]).unwrap_or_default();
            let mut visited = HashSet::new();
            let mut result = Vec::new();
            while let Some((dict, depth)) = pending.pop() {
                if !visited.insert(Rc::as_ptr(&dict) as *const () as usize) {
                    continue;
                }
                if dict.borrow().object_type() != ObjectType::Dictionary {
                    continue;
                }
                if let Some(next) = entry(&dict, b"Next") {
                    pending.push((next, depth));
                }
                if let Some(child) = entry(&dict, b"First") {
                    pending.push((child, depth + 1));
                }
                let bookmark = CpdfBookmark::new(Some(dict.clone()));
                result.push(OutlineItem {
                    depth,
                    title: text(&bookmark.title()),
                    target: target(&mut doc, &dict),
                    color: bookmark.color(),
                    bold: bookmark.style() & 2 != 0,
                    italic: bookmark.style() & 1 != 0,
                    open: bookmark.count() >= 0,
                });
            }
            Ok(result)
        })
    }
    pub fn add_attachment(&self, name: &str, bytes: &[u8]) -> Result<Attachment> {
        if name.is_empty() || bytes.len() > i32::MAX as usize {
            return Err(Error::InvalidArgument(
                "attachment requires a name and supported size",
            ));
        }
        self.with_structure_edit(|core| {
            let context = attachment::AttachmentDocument::from_document(&core.borrow());
            let file = attachment::add_attachment(
                &context,
                &name.chars().map(u32::from).collect::<Vec<_>>(),
            )
            .ok_or(Error::Edit)?;
            let holder = core.borrow().holder();
            if !attachment::set_file(&file, &holder, bytes, b"") {
                return Err(Error::Edit);
            }
            Ok(Attachment::from_core(self.clone(), file))
        })
    }
    /// Remove an entry from the embedded-file name tree. Retained attachment handles
    /// still refer to the detached file specification; they are not retargeted.
    pub fn delete_attachment(&self, index: usize) -> Result<()> {
        self.with_structure_edit(|core| {
            let context = attachment::AttachmentDocument::from_document(&core.borrow());
            if index >= attachment::attachment_count(&context) {
                return Err(Error::InvalidArgument("attachment index out of range"));
            }
            attachment::delete_attachment(&context, index)
                .then_some(())
                .ok_or(Error::Edit)
        })
    }

    pub fn attachments(&self) -> Result<Vec<Attachment>> {
        self.with_core(|core| {
            let snapshot = attachment::AttachmentDocument::from_document(&core.borrow());
            (0..attachment::attachment_count(&snapshot))
                .map(|index| {
                    let file = attachment::attachment_at(&snapshot, index).ok_or(Error::Format)?;
                    Ok(Attachment {
                        document: self.clone(),
                        name: text(&attachment::name(&file)),
                        description: text(&attachment::description(&file).unwrap_or_default()),
                        file,
                    })
                })
                .collect()
        })
    }
}
impl Page {
    pub fn model(&self) -> Result<PageModel> {
        self.ensure_live()?;
        Ok(PageModel {
            id: self.id(),
            index: self.index(),
            revision: self.document().revision(),
            layout: self.layout()?,
            links: self.links()?,
            annotations: self.annotations()?,
        })
    }
    pub fn layout(&self) -> Result<PageLayout> {
        self.ensure_live()?;
        self.document()
            .with_core(|core| layout(&mut core.borrow_mut(), self.index()))
    }
    /// Annotation links only; URLs inferred from text are a separate text query.
    pub fn links(&self) -> Result<Vec<Link>> {
        self.ensure_live()?;
        self.document().with_core(|core| {
            let mut doc = core.borrow_mut();
            let page = CpdfPage::from_document(&mut doc, self.index()).ok_or(Error::Format)?;
            let annots = page
                .annots_array()
                .map(|a| array_items(&a))
                .unwrap_or_default();
            let mut result = Vec::new();
            for (annotation_index, dict) in annots.into_iter() {
                if entry(&dict, b"Subtype").is_none_or(|v| v.borrow().get_string() != b"Link") {
                    continue;
                }
                let bounds = dict
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .map(|d| d.get_rect_for(b"Rect"))
                    .ok_or(Error::Format)?;
                result.push(Link {
                    annotation_index,
                    rect: rect(bounds),
                    target: target(&mut doc, &dict),
                });
            }
            Ok(result)
        })
    }
    pub fn annotations(&self) -> Result<Vec<AnnotationInfo>> {
        self.ensure_live()?;
        self.document().with_core(|core| {
            let page = CpdfPage::from_document(&mut core.borrow_mut(), self.index())
                .ok_or(Error::Format)?;
            let annots = page
                .annots_array()
                .map(|a| array_items(&a))
                .unwrap_or_default();
            Ok(annots
                .into_iter()
                .filter_map(|(annotation_index, object)| {
                    let borrowed = object.borrow();
                    let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>()?;
                    let subtype = dict.get_byte_string_for(b"Subtype", b"");
                    let bounds = dict.get_rect_for(b"Rect");
                    let flags = dict.get_integer_for(b"F", 0);
                    drop(borrowed);
                    Some(AnnotationInfo {
                        annotation_index,
                        subtype,
                        rect: rect(bounds),
                        flags,
                        contents: text_entry(&object, b"Contents"),
                    })
                })
                .collect())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_and_models_do_not_load_page_content() {
        let doc = Document::from_bytes(include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/render_corpus/pdfs/rectangles_multi_pages.pdf"
        )))
        .unwrap();
        let page = doc.page_handle(0).unwrap();
        assert!(!page.is_loaded().unwrap());
        let model = doc.page_model(0).unwrap();
        assert_eq!(model.id, page.id());
        assert_eq!(model.revision, doc.revision());
        assert_eq!(model.layout, doc.page_layout(0).unwrap());
        assert_eq!(doc.model().unwrap().pages.len(), doc.page_count().unwrap());
        assert!(!page.is_loaded().unwrap());
        assert!(matches!(
            doc.page_layout(usize::MAX),
            Err(Error::PageOutOfRange { .. })
        ));
    }
}
