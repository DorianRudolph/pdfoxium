// Public interfaces copied from pdfium-render 0.9.4, MIT; see LICENSE.pdfium-render.
// Backing operations use pdfoxium-core directly.
use crate::backend::document::DocumentState;
use crate::prelude::*;
use pdfoxium_core::{
    fpdfapi::parser::cpdf_object::ObjectRef,
    fpdfsdk::{fpdf_catalog as catalog, fpdf_signature as signatures},
};
use std::{
    marker::PhantomData,
    ops::{Range, RangeInclusive},
    rc::Rc,
};
pub struct PdfCatalog<'a> {
    pub(crate) document: Rc<DocumentState>,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfCatalog<'a> {
    pub fn is_tagged(&self) -> bool {
        catalog::is_tagged(&self.document.core.borrow())
    }
    pub fn get_language(&self) -> Result<String, PdfiumError> {
        catalog::language(&self.document.core.borrow())
            .map(|v| crate::backend::text::decode(&v))
            .filter(|v| !v.is_empty())
            .ok_or(PdfiumError::NoLanguageSetInDocumentCatalog)
    }
    pub fn set_language(&mut self, language: impl ToString) -> Result<(), PdfiumError> {
        let text = language.to_string();
        if catalog::set_language(
            &self.document.core.borrow(),
            &text.chars().map(u32::from).collect::<Vec<_>>(),
        ) {
            Ok(())
        } else {
            Err(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
        }
    }
}
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfSignatureModificationDetectionPermission {
    /// MDP access permission level 1: no changes to the document are permitted;
    /// any change to the document invalidates the signature.
    Mdp1,

    /// MDP access permission level 2: permitted changes are filling in forms,
    /// instantiating page templates, and signing; other changes invalidate the signature.
    Mdp2,

    /// MDP access permission level 3: permitted changes are the same as for level 2,
    /// as well as annotation creation, deletion, and modification; other changes
    /// invalidate the signature.
    Mdp3,
}
pub type PdfSignatureIndex = u16;
pub struct PdfSignature<'a> {
    _document: Rc<DocumentState>,
    field: ObjectRef,
    lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfSignature<'a> {
    pub fn bytes(&self) -> Vec<u8> {
        signatures::contents(&self.field).unwrap_or_default()
    }
    pub fn reason(&self) -> Option<String> {
        signatures::reason(&self.field)
            .map(|v| crate::backend::text::decode(&v))
            .filter(|v| !v.is_empty())
    }
    pub fn signing_date(&self) -> Option<String> {
        signatures::time(&self.field)
            .and_then(|v| String::from_utf8(v).ok())
            .filter(|v| !v.is_empty())
    }
    pub fn modification_detection_permission(
        &self,
    ) -> Result<PdfSignatureModificationDetectionPermission, PdfiumError> {
        match signatures::doc_mdp_permission(&self.field) {
            1 => Ok(PdfSignatureModificationDetectionPermission::Mdp1),
            2 => Ok(PdfSignatureModificationDetectionPermission::Mdp2),
            3 => Ok(PdfSignatureModificationDetectionPermission::Mdp3),
            0 => Err(PdfiumError::PdfiumLibraryInternalError(
                PdfiumInternalError::Unknown,
            )),
            _ => Err(PdfiumError::UnknownPdfSignatureModificationDetectionPermissionLevel),
        }
    }
}
pub struct PdfSignatures<'a> {
    pub(crate) document: Rc<DocumentState>,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfSignatures<'a> {
    pub fn len(&self) -> PdfSignatureIndex {
        signatures::collect_signatures(&self.document.core.borrow()).len() as PdfSignatureIndex
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
    pub fn as_range(&self) -> Range<PdfSignatureIndex> {
        0..self.len()
    }
    pub fn as_range_inclusive(&self) -> RangeInclusive<PdfSignatureIndex> {
        0..=self.len().saturating_sub(1)
    }
    pub fn get(&self, index: PdfSignatureIndex) -> Result<PdfSignature<'a>, PdfiumError> {
        let field = signatures::collect_signatures(&self.document.core.borrow())
            .get(index as usize)
            .cloned()
            .ok_or(PdfiumError::SignatureIndexOutOfBounds)?;
        Ok(PdfSignature {
            _document: self.document.clone(),
            field,
            lifetime: PhantomData,
        })
    }
    pub fn iter(&self) -> PdfSignaturesIterator<'_> {
        PdfSignaturesIterator {
            signatures: self,
            next_index: 0,
        }
    }
}
pub struct PdfSignaturesIterator<'a> {
    signatures: &'a PdfSignatures<'a>,
    next_index: PdfSignatureIndex,
}
impl<'a> Iterator for PdfSignaturesIterator<'a> {
    type Item = PdfSignature<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.next_index >= self.signatures.len() {
            return None;
        }
        let index = self.next_index;
        self.next_index += 1;
        self.signatures.get(index).ok()
    }
}
bitflags::bitflags! {
    struct FpdfPermissions: u32 {
        const RESERVED_BIT_1 =                          0b00000000000000000000000000000001;
        const RESERVED_BIT_2 =                          0b00000000000000000000000000000010;
        const CAN_PRINT_BIT_3 =                         0b00000000000000000000000000000100;
        const CAN_MODIFY_BIT_4 =                        0b00000000000000000000000000001000;
        const CAN_EXTRACT_TEXT_AND_GRAPHICS_BIT_5 =     0b00000000000000000000000000010000;
        const CAN_ANNOTATE_AND_FORM_FILL_BIT_6 =        0b00000000000000000000000000100000;
        const RESERVED_BIT_7 =                          0b00000000000000000000000001000000;
        const RESERVED_BIT_8 =                          0b00000000000000000000000010000000;
        const V3_CAN_FORM_FILL_BIT_9 =                  0b00000000000000000000000100000000;
        const V3_CAN_EXTRACT_TEXT_AND_GRAPHICS_BIT_10 = 0b00000000000000000000001000000000;
        const V3_CAN_ASSEMBLE_DOCUMENT_BIT_11 =         0b00000000000000000000010000000000;
        const V3_CAN_PRINT_HIGH_QUALITY_BIT_12 =        0b00000000000000000000100000000000;
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfSecurityHandlerRevision {
    Unprotected,
    Revision2,
    Revision3,
    Revision4,
}
pub struct PdfPermissions<'a> {
    pub(crate) document: Rc<DocumentState>,
    pub(crate) lifetime: PhantomData<&'a ()>,
}
impl<'a> PdfPermissions<'a> {
    pub fn security_handler_revision(&self) -> Result<PdfSecurityHandlerRevision, PdfiumError> {
        let document = self.document.core.borrow();
        let handler = document.parser().security_handler();
        match handler.as_ref().and_then(|h| h.revision()).unwrap_or(-1) {
            -1 => Ok(PdfSecurityHandlerRevision::Unprotected),
            2 => Ok(PdfSecurityHandlerRevision::Revision2),
            3 => Ok(PdfSecurityHandlerRevision::Revision3),
            4 => Ok(PdfSecurityHandlerRevision::Revision4),
            _ => Err(PdfiumError::UnknownPdfSecurityHandlerRevision),
        }
    }
    pub fn can_print_high_quality(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            PdfSecurityHandlerRevision::Revision2 => {
                permissions.contains(FpdfPermissions::CAN_PRINT_BIT_3)
            }
            PdfSecurityHandlerRevision::Revision3 | PdfSecurityHandlerRevision::Revision4 => {
                permissions.contains(FpdfPermissions::CAN_PRINT_BIT_3)
                    && permissions.contains(FpdfPermissions::V3_CAN_PRINT_HIGH_QUALITY_BIT_12)
            }
        };

        Ok(result)
    }
    pub fn can_print_only_low_quality(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected | PdfSecurityHandlerRevision::Revision2 => {
                false
            }
            PdfSecurityHandlerRevision::Revision3 | PdfSecurityHandlerRevision::Revision4 => {
                permissions.contains(FpdfPermissions::CAN_PRINT_BIT_3)
                    && !permissions.contains(FpdfPermissions::V3_CAN_PRINT_HIGH_QUALITY_BIT_12)
            }
        };

        Ok(result)
    }
    pub fn can_assemble_document(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            PdfSecurityHandlerRevision::Revision2 => {
                permissions.contains(FpdfPermissions::CAN_MODIFY_BIT_4)
            }
            PdfSecurityHandlerRevision::Revision3 | PdfSecurityHandlerRevision::Revision4 => {
                permissions.contains(FpdfPermissions::V3_CAN_ASSEMBLE_DOCUMENT_BIT_11)
            }
        };

        Ok(result)
    }
    pub fn can_modify_document_content(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            _ => permissions.contains(FpdfPermissions::CAN_MODIFY_BIT_4),
        };

        Ok(result)
    }
    pub fn can_extract_text_and_graphics(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            PdfSecurityHandlerRevision::Revision2 => {
                permissions.contains(FpdfPermissions::CAN_EXTRACT_TEXT_AND_GRAPHICS_BIT_5)
            }
            // TODO: AJRC - 27/5/22 - what operations are permitted by bit 10 but prevented by bit 5?
            PdfSecurityHandlerRevision::Revision3 | PdfSecurityHandlerRevision::Revision4 => {
                permissions.contains(FpdfPermissions::V3_CAN_EXTRACT_TEXT_AND_GRAPHICS_BIT_10)
            }
        };

        Ok(result)
    }
    pub fn can_fill_existing_interactive_form_fields(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            PdfSecurityHandlerRevision::Revision2 => {
                permissions.contains(FpdfPermissions::CAN_ANNOTATE_AND_FORM_FILL_BIT_6)
            }
            PdfSecurityHandlerRevision::Revision3 | PdfSecurityHandlerRevision::Revision4 => {
                permissions.contains(FpdfPermissions::V3_CAN_FORM_FILL_BIT_9)
            }
        };

        Ok(result)
    }
    pub fn can_create_new_interactive_form_fields(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            _ => {
                permissions.contains(FpdfPermissions::CAN_MODIFY_BIT_4)
                    && permissions.contains(FpdfPermissions::CAN_ANNOTATE_AND_FORM_FILL_BIT_6)
            }
        };

        Ok(result)
    }
    pub fn can_add_or_modify_text_annotations(&self) -> Result<bool, PdfiumError> {
        let permissions = self.get_permissions_bits();

        let result = match self.security_handler_revision()? {
            PdfSecurityHandlerRevision::Unprotected => true,
            _ => permissions.contains(FpdfPermissions::CAN_ANNOTATE_AND_FORM_FILL_BIT_6),
        };

        Ok(result)
    }
}
impl PdfPermissions<'_> {
    fn get_permissions_bits(&self) -> FpdfPermissions {
        FpdfPermissions::from_bits_truncate(self.document.core.borrow().parser().permissions(false))
    }
}
