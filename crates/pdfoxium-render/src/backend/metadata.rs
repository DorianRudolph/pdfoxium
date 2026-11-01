use crate::backend::{document::DocumentState, prelude::*};
use pdfoxium_core::{
    fpdfapi::parser::{cpdf_dictionary::CpdfDictionary, cpdf_object::ObjectRef},
    fpdfsdk::fpdf_attachment as attachment,
};
use std::{
    io::{Read, Write},
    path::Path,
    rc::Rc,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfDocumentMetadataTagType {
    Title,
    Author,
    Subject,
    Keywords,
    Creator,
    Producer,
    CreationDate,
    ModificationDate,
}
impl PdfDocumentMetadataTagType {
    fn key(self) -> &'static [u8] {
        match self {
            Self::Title => b"Title",
            Self::Author => b"Author",
            Self::Subject => b"Subject",
            Self::Keywords => b"Keywords",
            Self::Creator => b"Creator",
            Self::Producer => b"Producer",
            Self::CreationDate => b"CreationDate",
            Self::ModificationDate => b"ModDate",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfDocumentMetadataTag {
    tag: PdfDocumentMetadataTagType,
    value: String,
}
impl PdfDocumentMetadataTag {
    pub fn tag_type(&self) -> PdfDocumentMetadataTagType {
        self.tag
    }
    pub fn value(&self) -> &str {
        &self.value
    }
}
pub struct PdfMetadata {
    tags: Vec<PdfDocumentMetadataTag>,
}
impl PdfMetadata {
    pub fn len(&self) -> usize {
        self.tags.len()
    }
    pub fn is_empty(&self) -> bool {
        self.tags.is_empty()
    }
    pub fn get(&self, tag: PdfDocumentMetadataTagType) -> Option<PdfDocumentMetadataTag> {
        self.tags.iter().find(|t| t.tag == tag).cloned()
    }
    pub fn iter(&self) -> std::slice::Iter<'_, PdfDocumentMetadataTag> {
        self.tags.iter()
    }
}
impl PdfDocument {
    pub fn metadata(&self) -> PdfMetadata {
        let mut tags = Vec::new();
        let info = self.inner.core.borrow_mut().info();
        if let Some(info) = info {
            let info = info.borrow();
            if let Some(info) = info.as_any().downcast_ref::<CpdfDictionary>() {
                for tag in [
                    PdfDocumentMetadataTagType::Title,
                    PdfDocumentMetadataTagType::Author,
                    PdfDocumentMetadataTagType::Subject,
                    PdfDocumentMetadataTagType::Keywords,
                    PdfDocumentMetadataTagType::Creator,
                    PdfDocumentMetadataTagType::Producer,
                    PdfDocumentMetadataTagType::CreationDate,
                    PdfDocumentMetadataTagType::ModificationDate,
                ] {
                    if let Some(value) = info.get_direct_object_for(tag.key()) {
                        tags.push(PdfDocumentMetadataTag {
                            tag,
                            value: crate::backend::text::decode(&value.borrow().get_unicode_text()),
                        });
                    }
                }
            }
        }
        PdfMetadata { tags }
    }
    pub fn attachments(&self) -> PdfAttachments {
        PdfAttachments {
            document: self.inner.clone(),
        }
    }
}
pub struct PdfAttachments {
    document: Rc<DocumentState>,
}
pub struct PdfAttachment {
    _document: Rc<DocumentState>,
    file: ObjectRef,
}
impl PdfAttachments {
    fn source(&self) -> attachment::AttachmentDocument {
        attachment::AttachmentDocument::from_document(&self.document.core.borrow())
    }
    pub fn len(&self) -> usize {
        attachment::attachment_count(&self.source())
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn get(&self, index: usize) -> Result<PdfAttachment, PdfiumError> {
        let file = attachment::attachment_at(&self.source(), index)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        Ok(PdfAttachment {
            _document: self.document.clone(),
            file,
        })
    }

    pub fn create_attachment_from_bytes(
        &mut self,
        name: &str,
        bytes: &[u8],
    ) -> Result<PdfAttachment, PdfiumError> {
        if bytes.len() > i32::MAX as usize {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        let name: Vec<u32> = name.chars().map(u32::from).collect();
        let file = attachment::add_attachment(&self.source(), &name)
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        if !attachment::set_file(&file, &self.document.core.borrow().holder(), bytes, b"") {
            return Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure);
        }
        Ok(PdfAttachment {
            _document: self.document.clone(),
            file,
        })
    }
    pub fn create_attachment_from_file(
        &mut self,
        name: &str,
        path: impl AsRef<Path>,
    ) -> Result<PdfAttachment, PdfiumError> {
        self.create_attachment_from_bytes(name, &std::fs::read(path)?)
    }
    pub fn create_attachment_from_reader(
        &mut self,
        name: &str,
        mut reader: impl Read,
    ) -> Result<PdfAttachment, PdfiumError> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes)?;
        self.create_attachment_from_bytes(name, &bytes)
    }
    pub fn delete_at_index(&mut self, index: usize) -> Result<(), PdfiumError> {
        if index >= self.len() {
            return Err(PdfiumError::AttachmentIndexOutOfBounds);
        }
        if attachment::delete_attachment(&self.source(), index) {
            Ok(())
        } else {
            Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        }
    }
}
impl PdfAttachment {
    pub fn name(&self) -> String {
        crate::backend::text::decode(&attachment::name(&self.file))
    }
    pub fn len(&self) -> usize {
        attachment::with_file(&self.file, |bytes| bytes.len()).unwrap_or(0)
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn save_to_bytes(&self) -> Result<Vec<u8>, PdfiumError> {
        attachment::with_file(&self.file, |bytes| bytes.to_vec())
            .ok_or(PdfiumError::NoDataInAttachment)
    }
    pub fn save_to_writer(&self, writer: &mut impl Write) -> Result<(), PdfiumError> {
        writer.write_all(&self.save_to_bytes()?)?;
        Ok(())
    }
    pub fn save_to_file(&self, path: impl AsRef<Path>) -> Result<(), PdfiumError> {
        self.save_to_writer(&mut std::fs::File::create(path)?)
    }
}
