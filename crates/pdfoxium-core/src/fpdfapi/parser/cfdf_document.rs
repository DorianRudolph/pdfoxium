// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cfdf_document.{h,cpp}`: FDF object table, parser and writer.
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_object::ObjectRef;
use super::cpdf_syntax_parser::CpdfSyntaxParser;
use super::fpdf_parser_utility::object_to_debug_bytes;
use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;
use crate::fxcrt::fx_stream::SeekableReadStream;
use std::rc::Rc;

pub struct CfdfDocument {
    objects: HolderRef,
    root: Option<ObjectRef>,
    // Parsed streams retain their input, matching CFDF_Document::file_.
    file: Option<Rc<dyn SeekableReadStream>>,
}
impl Default for CfdfDocument {
    fn default() -> Self {
        Self::new()
    }
}
impl CfdfDocument {
    pub fn new() -> Self {
        Self {
            objects: CpdfIndirectObjectHolder::new(),
            root: None,
            file: None,
        }
    }
    pub fn create_new_doc() -> Self {
        let mut document = Self::new();
        let root = document.objects.borrow_mut().new_indirect_dictionary();
        root.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"FDF", CpdfDictionary::new());
        document.root = Some(root);
        document
    }
    pub fn parse_memory(data: &[u8]) -> Option<Self> {
        // The public borrowed input has no document lifetime in Rust; own the
        // retained input rather than constructing a dangling span stream.
        let file: Rc<dyn SeekableReadStream> =
            Rc::new(CfxReadOnlyContainerStream::new(data.to_vec()));
        let mut document = Self::new();
        document.parse_stream(file);
        document.root.as_ref()?;
        Some(document)
    }
    fn parse_stream(&mut self, file: Rc<dyn SeekableReadStream>) {
        let mut parser = CpdfSyntaxParser::from_file(Rc::clone(&file));
        self.file = Some(file);
        loop {
            let word = parser.get_next_word();
            if word.is_number {
                let number = crate::fxcrt::fx_system::atoui(&word.word);
                if number == 0 {
                    break;
                }
                if !parser.get_next_word().is_number {
                    break;
                }
                if parser.get_next_word().word != b"obj" {
                    break;
                }
                let Some(object) = parser.get_object_body(Some(&self.objects)) else {
                    break;
                };
                self.objects
                    .borrow_mut()
                    .replace_indirect_object_if_higher_generation(number, Some(object));
                if parser.get_next_word().word != b"endobj" {
                    break;
                }
            } else {
                if word.word != b"trailer" {
                    break;
                }
                if let Some(main) = parser.get_object_body(Some(&self.objects)) {
                    self.root = main
                        .borrow()
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .and_then(|dict| dict.get_dict_for(b"Root"));
                }
                break;
            }
        }
    }
    pub fn root(&self) -> Option<ObjectRef> {
        self.root.clone()
    }
    pub fn holder(&self) -> HolderRef {
        Rc::clone(&self.objects)
    }
    pub fn write_to_string(&self) -> Vec<u8> {
        let Some(root) = &self.root else {
            return Vec::new();
        };
        let mut output = b"%FDF-1.2\r\n".to_vec();
        let objects: Vec<_> = self
            .objects
            .borrow()
            .iter()
            .filter_map(|(&number, object)| {
                object.as_ref().map(|object| (number, Rc::clone(object)))
            })
            .collect();
        for (number, object) in objects {
            output.extend_from_slice(format!("{number} 0 obj\r\n").as_bytes());
            output.extend_from_slice(&object_to_debug_bytes(Some(&object)));
            output.extend_from_slice(b"\r\nendobj\r\n\r\n");
        }
        output.extend_from_slice(
            format!(
                "trailer\r\n<</Root {} 0 R>>\r\n%%EOF\r\n",
                root.borrow().obj_num()
            )
            .as_bytes(),
        );
        output
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    #[test]
    fn fdf_round_trip_owns_input_and_preserves_indirect_root() {
        let document = CfdfDocument::create_new_doc();
        let root = document.root().unwrap();
        let fdf = root
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_dict_for(b"FDF")
            .unwrap();
        fdf.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"F", CpdfString::from_bytes(None, b"form.pdf"));
        let parsed = {
            let bytes = document.write_to_string();
            CfdfDocument::parse_memory(&bytes).unwrap()
        };
        assert_eq!(parsed.root().unwrap().borrow().obj_num(), 1);
        assert_eq!(parsed.write_to_string(), document.write_to_string());
        assert!(CfdfDocument::parse_memory(b"%FDF-1.2\ntrailer <<>>").is_none());
        assert!(CfdfDocument::parse_memory(b"%FDF-1.2\n0 0 obj <<>> endobj").is_none());
    }
}
