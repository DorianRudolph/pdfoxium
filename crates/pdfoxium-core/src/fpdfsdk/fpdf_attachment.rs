// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Embedded-file editing from `fpdfsdk/fpdf_attachment.cpp`, with annotation
//! file-spec helpers from `fpdfsdk/fpdf_annot.cpp`. The host supplies UTC dates.
use crate::fdrm::fx_crypt::crypt_md5_generate;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef;
use crate::fpdfapi::parser::cpdf_object::{
    get_direct_object, make_reference, retain_object, ObjectRef,
};
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_stream_acc::CpdfRetainedStreamAcc;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_decode::{
    hex_decode, pdf_decode_text, pdf_hex_encode_string,
};
use crate::fpdfdoc::cpdf_filespec::CpdfFileSpec;
use crate::fpdfdoc::cpdf_nametree::CpdfNameTree;
use crate::fxcrt::fx_string::utf8_encode;

fn dictionary_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> R) -> Option<R> {
    let mut borrowed = object.borrow_mut();
    Some(f(borrowed.as_any_mut().downcast_mut::<CpdfDictionary>()?))
}
fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)
}
fn direct_entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    get_direct_object(&entry(object, key)?)
}

/// Snapshot the document's object ownership before callbacks may re-enter it.
pub struct AttachmentDocument {
    root: Option<ObjectRef>,
    objects: HolderRef,
}
impl AttachmentDocument {
    pub fn from_document(document: &CpdfDocument) -> Self {
        Self {
            root: document.root(),
            objects: document.holder(),
        }
    }
}
pub fn attachment_count(document: &AttachmentDocument) -> usize {
    document
        .root
        .as_ref()
        .and_then(|root| CpdfNameTree::create_from_root(root, b"EmbeddedFiles"))
        .map_or(0, |tree| tree.count())
}
pub fn add_attachment(document: &AttachmentDocument, name: &[u32]) -> Option<ObjectRef> {
    if name.is_empty() {
        return None;
    }
    let tree = CpdfNameTree::create_with_root_name_array_from_parts(
        document.root.as_ref()?,
        &document.objects,
        b"EmbeddedFiles",
    )?;
    let file = create_file_spec(&document.objects, name);
    tree.add_value_and_name(make_reference(&file, &document.objects), name)
        .then_some(file)
}
pub fn attachment_at(document: &AttachmentDocument, index: usize) -> Option<ObjectRef> {
    let tree = CpdfNameTree::create_from_root(document.root.as_ref()?, b"EmbeddedFiles")?;
    tree.lookup_value_and_name(index).map(|(file, _)| file)
}
pub fn delete_attachment(document: &AttachmentDocument, index: usize) -> bool {
    document
        .root
        .as_ref()
        .and_then(|root| CpdfNameTree::create_from_root(root, b"EmbeddedFiles"))
        .is_some_and(|tree| tree.delete_value_and_name(index))
}
pub fn create_file_spec(objects: &HolderRef, name: &[u32]) -> ObjectRef {
    let file = objects.borrow_mut().new_indirect_dictionary();
    dictionary_mut(&file, |dict| {
        dict.set_new_name_for(b"Type", b"Filespec");
        dict.set_new_for(b"UF", CpdfString::from_unicode(None, name));
        dict.set_new_for(b"F", CpdfString::from_unicode(None, name));
    });
    file
}
pub fn name(file: &ObjectRef) -> Vec<u32> {
    CpdfFileSpec::new(file.clone()).file_name()
}
pub fn has_key(file: &ObjectRef, key: &[u8]) -> bool {
    CpdfFileSpec::new(file.clone())
        .params_dict()
        .is_some_and(|params| entry(&params, key).is_some())
}
pub fn value_type(file: &ObjectRef, key: &[u8]) -> Option<ObjectType> {
    let params = CpdfFileSpec::new(file.clone()).params_dict()?;
    let object = entry(&params, key)?;
    let kind = object.borrow().object_type();
    Some(kind)
}
pub fn set_string_value(file: &ObjectRef, key: &[u8], value: &[u32]) -> bool {
    let Some(params) = CpdfFileSpec::new(file.clone()).mutable_params_dict() else {
        return false;
    };
    dictionary_mut(&params, |params| {
        if key == b"CheckSum" {
            params.set_new_for(
                key,
                CpdfString::from_hex_bytes(None, &hex_decode(&utf8_encode(value)).data),
            );
        } else {
            params.set_new_for(key, CpdfString::from_unicode(None, value));
        }
    })
    .is_some()
}
pub fn string_value(file: &ObjectRef, key: &[u8]) -> Option<Vec<u32>> {
    let params = CpdfFileSpec::new(file.clone()).params_dict()?;
    let Some(object) = entry(&params, key) else {
        return Some(Vec::new());
    };
    let borrowed = object.borrow();
    if !matches!(
        borrowed.object_type(),
        ObjectType::String | ObjectType::Name
    ) {
        return Some(Vec::new());
    }
    if key == b"CheckSum" {
        if let Some(string) = borrowed
            .as_any()
            .downcast_ref::<CpdfString>()
            .filter(|s| s.is_hex())
        {
            return Some(pdf_decode_text(&pdf_hex_encode_string(string.get_string())));
        }
    }
    Some(borrowed.get_unicode_text())
}
pub fn set_description(file: &ObjectRef, value: &[u32]) -> bool {
    let Some(dict) = CpdfFileSpec::new(file.clone()).mutable_file_spec_dict() else {
        return false;
    };
    dictionary_mut(&dict, |dict| {
        dict.set_new_for(b"Desc", CpdfString::from_unicode(None, value));
    })
    .is_some()
}
pub fn description(file: &ObjectRef) -> Option<Vec<u32>> {
    let dict = CpdfFileSpec::new(file.clone()).file_spec_dict()?;
    Some(
        entry(&dict, b"Desc")
            .filter(|v| v.borrow().object_type() == ObjectType::String)
            .map_or_else(Vec::new, |v| v.borrow().get_unicode_text()),
    )
}
/// Contents acquire their own PDF stream allocation, matching source ownership.
/// Date formatting is kept at the host boundary rather than adding a local clock.
pub fn set_file(
    file: &ObjectRef,
    objects: &HolderRef,
    contents: &[u8],
    creation_date: &[u8],
) -> bool {
    if file.borrow().object_type() != ObjectType::Dictionary || contents.len() > i32::MAX as usize {
        return false;
    }
    let mut params = CpdfDictionary::new();
    params.set_new_for(
        b"Size",
        CpdfScalarObject::number_from_signed(contents.len() as i32),
    );
    params.set_new_for(b"CreationDate", CpdfString::from_bytes(None, creation_date));
    params.set_new_for(
        b"CheckSum",
        CpdfString::from_hex_bytes(None, &crypt_md5_generate(contents)),
    );
    let mut dictionary = CpdfDictionary::new();
    dictionary.set_new_for(
        b"DL",
        CpdfScalarObject::number_from_signed(contents.len() as i32),
    );
    dictionary.set_new_for(b"Params", params);
    let stream = retain_object(CpdfStream::from_data_and_dict(
        contents.to_vec(),
        retain_object(dictionary),
    ));
    objects.borrow_mut().add_indirect_object(stream.clone());
    let mut embedded = CpdfDictionary::new();
    embedded.set_for(b"F", Some(make_reference(&stream, objects)));
    dictionary_mut(file, |dict| {
        dict.set_new_for(b"EF", embedded);
    })
    .is_some()
}
pub fn with_file<R>(file: &ObjectRef, consume: impl FnOnce(&[u8]) -> R) -> Option<R> {
    // Release the file-spec/parameter borrows before input stream callbacks.
    let stream = CpdfFileSpec::new(file.clone()).file_stream()?;
    let access = CpdfRetainedStreamAcc::load_filtered(stream)?;
    let data = access.get_span();
    Some(consume(&data))
}
pub fn subtype(file: &ObjectRef) -> Vec<u32> {
    let Some(stream) = CpdfFileSpec::new(file.clone()).file_stream() else {
        return Vec::new();
    };
    let dictionary = stream
        .borrow()
        .as_any()
        .downcast_ref::<CpdfStream>()
        .unwrap()
        .get_dict();
    let name = direct_entry(&dictionary, b"Subtype")
        .filter(|value| value.borrow().object_type() == ObjectType::Name)
        .map_or_else(Vec::new, |value| value.borrow().get_string());
    pdf_decode_text(&name)
}
pub fn af_relationship(file: &ObjectRef) -> Option<Vec<u32>> {
    let dict = CpdfFileSpec::new(file.clone()).file_spec_dict()?;
    let name = direct_entry(&dict, b"AFRelationship")
        .filter(|value| value.borrow().object_type() == ObjectType::Name)
        .map_or_else(Vec::new, |value| value.borrow().get_string());
    Some(pdf_decode_text(&name))
}
pub fn annotation_attachment(annotation: &ObjectRef) -> Option<ObjectRef> {
    if crate::fpdfsdk::fpdf_annot::subtype(annotation)
        != crate::fpdfdoc::cpdf_annot::Subtype::FileAttachment
    {
        return None;
    }
    direct_entry(annotation, b"FS")
}
pub fn add_annotation_attachment(
    annotation: &ObjectRef,
    objects: &HolderRef,
    name: &[u32],
) -> Option<ObjectRef> {
    if name.is_empty()
        || crate::fpdfsdk::fpdf_annot::subtype(annotation)
            != crate::fpdfdoc::cpdf_annot::Subtype::FileAttachment
    {
        return None;
    }
    let file = create_file_spec(objects, name);
    dictionary_mut(annotation, |dict| {
        dict.set_for(b"FS", Some(make_reference(&file, objects)));
    })?;
    Some(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(text: &str) -> Vec<u32> {
        text.chars().map(u32::from).collect()
    }
    #[test]
    fn document_attachments_keep_order_payload_checksum_and_parameter_contracts() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let context = AttachmentDocument::from_document(&document);
        let first = add_attachment(&context, &wide("z.txt")).unwrap();
        let second = add_attachment(&context, &wide("a.txt")).unwrap();
        assert!(add_attachment(&context, &wide("a.txt")).is_none());
        assert!(add_attachment(&context, &[]).is_none());
        assert_eq!(attachment_count(&context), 2);
        assert_eq!(name(&attachment_at(&context, 0).unwrap()), wide("a.txt"));
        assert!(std::rc::Rc::ptr_eq(
            &attachment_at(&context, 1).unwrap(),
            &first
        ));
        assert!(!set_string_value(
            &first,
            b"CreationDate",
            &wide("missing stream")
        ));
        assert!(set_file(
            &first,
            &document.holder(),
            b"payload\0bytes",
            b"D:20261022112233"
        ));
        assert_eq!(
            with_file(&first, |bytes| bytes.to_vec()),
            Some(b"payload\0bytes".to_vec())
        );
        assert!(has_key(&first, b"CheckSum"));
        assert_eq!(value_type(&first, b"Size"), Some(ObjectType::Number));
        assert_eq!(string_value(&first, b"Size"), Some(Vec::new()));
        assert_eq!(string_value(&first, b"Absent"), Some(Vec::new()));
        assert!(set_string_value(&first, b"CheckSum", &wide("01 aB 2")));
        assert_eq!(string_value(&first, b"CheckSum"), Some(wide("<01AB20>")));
        assert!(set_description(&first, &wide("Unicode \u{1f4ce}")));
        assert_eq!(description(&first), Some(wide("Unicode \u{1f4ce}")));
        assert_eq!(subtype(&first), Vec::<u32>::new());
        assert_eq!(af_relationship(&first), Some(Vec::new()));
        assert!(delete_attachment(&context, 0));
        assert!(!delete_attachment(&context, 9));
        assert_eq!(attachment_count(&context), 1);
        assert_eq!(name(&second), wide("a.txt")); // Deletion only unlinks the name.
    }
    #[test]
    fn annotation_files_are_not_implicitly_added_to_document_name_tree() {
        let mut document = CpdfDocument::new();
        document.create_new_doc();
        let mut annotation = CpdfDictionary::new();
        annotation.set_new_name_for(b"Subtype", b"FileAttachment");
        let annotation = retain_object(annotation);
        let file =
            add_annotation_attachment(&annotation, &document.holder(), &wide("note.txt")).unwrap();
        assert!(std::rc::Rc::ptr_eq(
            &annotation_attachment(&annotation).unwrap(),
            &file
        ));
        assert!(set_file(
            &file,
            &document.holder(),
            b"",
            b"D:20261022000000"
        ));
        assert_eq!(with_file(&file, |bytes| bytes.len()), Some(0));
        assert_eq!(
            attachment_count(&AttachmentDocument::from_document(&document)),
            0
        );
    }
}
