// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_stream.{h,cpp}`.

use std::any::Any;
use std::collections::HashSet;
use std::rc::Rc;

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_flateencoder::CpdfFlateEncoder;
use super::cpdf_object::{clone_non_cyclic, retain_object, ObjectRef, PdfObject};
use super::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::fpdf_parser_decode::pdf_decode_text;
use super::fpdf_parser_utility::validate_dict_type;
use crate::fxcrt::fx_stream::{SeekableReadStream, WriteStream};

pub const FILE_BUF_SIZE: usize = 512;

enum StreamData {
    Memory(Rc<Vec<u8>>),
    File(Rc<dyn SeekableReadStream>),
}

pub struct CpdfStream {
    obj_num: u32,
    gen_num: u32,
    data: StreamData,
    dict: ObjectRef,
}

impl CpdfStream {
    pub fn from_dict(dict: ObjectRef) -> Self {
        Self::from_data_and_dict(Vec::new(), dict)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let mut stream = Self {
            obj_num: 0,
            gen_num: 0,
            data: StreamData::Memory(Rc::new(Vec::new())),
            dict: retain_object(CpdfDictionary::new()),
        };
        stream.set_data(bytes);
        stream
    }

    pub fn from_data_and_dict(data: Vec<u8>, dict: ObjectRef) -> Self {
        Self::check_dict(&dict);
        let mut stream = Self {
            obj_num: 0,
            gen_num: 0,
            data: StreamData::Memory(Rc::new(data)),
            dict,
        };
        stream.set_length_in_dict(stream.get_raw_size());
        stream
    }

    /// Create a distinct stream with a private dictionary and shared immutable
    /// source bytes. Editing an appearance resource dictionary must not mutate
    /// another widget's stream or copy its potentially large contents.
    pub fn with_shared_data_and_dict(&self, dict: ObjectRef) -> Self {
        Self::check_dict(&dict);
        let data = match &self.data {
            StreamData::Memory(data) => StreamData::Memory(Rc::clone(data)),
            StreamData::File(file) => StreamData::File(Rc::clone(file)),
        };
        Self {
            obj_num: 0,
            gen_num: 0,
            data,
            dict,
        }
    }

    pub fn from_file(file: Rc<dyn SeekableReadStream>, dict: ObjectRef) -> Self {
        Self::check_dict(&dict);
        let mut stream = Self {
            obj_num: 0,
            gen_num: 0,
            data: StreamData::File(file),
            dict,
        };
        stream.set_length_in_dict(stream.get_raw_size());
        stream
    }

    pub fn obj_num(&self) -> u32 {
        self.obj_num
    }
    pub fn set_obj_num(&mut self, value: u32) {
        self.obj_num = value;
    }
    pub fn gen_num(&self) -> u32 {
        self.gen_num
    }
    pub fn set_gen_num(&mut self, value: u32) {
        self.gen_num = value;
    }
    pub fn get_dict(&self) -> ObjectRef {
        Rc::clone(&self.dict)
    }

    pub fn is_file_based(&self) -> bool {
        matches!(self.data, StreamData::File(_))
    }
    pub fn is_memory_based(&self) -> bool {
        matches!(self.data, StreamData::Memory(_))
    }

    pub fn get_raw_size(&self) -> usize {
        match &self.data {
            StreamData::Memory(data) => data.len(),
            StreamData::File(file) => {
                usize::try_from(file.get_size()).expect("negative PDF stream file size")
            }
        }
    }

    pub fn get_in_memory_raw_data(&self) -> &[u8] {
        match &self.data {
            StreamData::Memory(data) => data,
            StreamData::File(_) => panic!("file based stream has no in-memory span"),
        }
    }

    pub fn share_in_memory_raw_data(&self) -> Option<Rc<Vec<u8>>> {
        match &self.data {
            StreamData::Memory(data) => Some(Rc::clone(data)),
            StreamData::File(_) => None,
        }
    }

    pub fn read_all_raw_data(&self) -> Vec<u8> {
        let StreamData::File(file) = &self.data else {
            panic!("ReadAllRawData requires file based stream");
        };
        let mut result = vec![0; self.get_raw_size()];
        if !file.read_block_at_offset(&mut result, 0) {
            return Vec::new();
        }
        result
    }

    pub fn set_data(&mut self, data: &[u8]) {
        self.take_data(data.to_vec());
    }

    pub fn set_data_from_stringstream(&mut self, data: &[u8], position: i64) {
        if position <= 0 {
            self.set_data(&[]);
            return;
        }
        let size = usize::try_from(position).expect("negative stringstream position");
        self.set_data(&data[..size]);
    }

    pub fn set_data_from_stringstream_and_remove_filter(&mut self, data: &[u8], position: i64) {
        if position <= 0 {
            self.set_data_and_remove_filter(&[]);
            return;
        }
        let size = usize::try_from(position).expect("negative stringstream position");
        self.set_data_and_remove_filter(&data[..size]);
    }

    pub fn take_data(&mut self, data: Vec<u8>) {
        let size = data.len();
        self.data = StreamData::Memory(Rc::new(data));
        self.set_length_in_dict(size);
    }

    pub fn set_data_and_remove_filter(&mut self, data: &[u8]) {
        self.set_data(data);
        let mut dict = self.dict.borrow_mut();
        let dict = dict.as_any_mut().downcast_mut::<CpdfDictionary>().unwrap();
        dict.remove_for(b"Filter");
        dict.remove_for(b"DecodeParms");
    }

    pub fn init_stream_from_file(&mut self, file: Rc<dyn SeekableReadStream>) {
        let size = usize::try_from(file.get_size()).expect("negative PDF stream file size");
        self.data = StreamData::File(file);
        self.dict = retain_object(CpdfDictionary::new());
        self.set_length_in_dict(size);
    }

    pub fn has_filter(&self) -> bool {
        self.dict
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .key_exist(b"Filter")
    }

    fn set_length_in_dict(&mut self, size: usize) {
        let length = i32::try_from(size).expect("PDF stream length exceeds int");
        self.dict
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(
                b"Length",
                Some(retain_object(CpdfScalarObject::number_from_signed(length))),
            );
    }

    fn check_dict(dict: &ObjectRef) {
        let borrowed = dict.borrow();
        assert!(borrowed.is_inline(), "PDF stream dictionary must be direct");
        assert_eq!(borrowed.object_type(), ObjectType::Dictionary);
    }

    fn clone_with_visited(&self, direct: bool, visited: &HashSet<usize>) -> ObjectRef {
        let mut access = CpdfStreamAcc::new(self);
        access.load_all_data_raw();
        let data = access.detach_data();
        let dict =
            clone_non_cyclic(&self.dict, direct, visited).expect("cyclic PDF stream dictionary");
        retain_object(Self::from_data_and_dict(data, dict))
    }

    fn write_stream_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        let is_metadata = {
            let borrowed = self.dict.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            validate_dict_type(Some(dict), b"Metadata")
                && crate::fxcrt::bytestring::eq_cstr(&dict.get_name_for(b"Subtype"), b"XML")
        };
        let mut encoder = CpdfFlateEncoder::new(self, !is_metadata);
        let encrypted = if is_metadata {
            None
        } else {
            encryptor.map(|enc| enc.encrypt(encoder.get_span()))
        };
        let size = encrypted
            .as_ref()
            .map_or_else(|| encoder.get_span().len(), Vec::len);
        encoder.update_length(size);
        if !encoder.write_dict_to(archive, encryptor) {
            return false;
        }
        if !archive.write_string(b"stream\r\n") {
            return false;
        }
        let data = encrypted.as_deref().unwrap_or_else(|| encoder.get_span());
        archive.write_block(data) && archive.write_string(b"\r\nendstream")
    }
}

impl PdfObject for CpdfStream {
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
    fn object_type(&self) -> ObjectType {
        ObjectType::Stream
    }
    fn obj_num(&self) -> u32 {
        self.obj_num
    }
    fn set_obj_num(&mut self, value: u32) {
        self.obj_num = value;
    }
    fn gen_num(&self) -> u32 {
        self.gen_num
    }
    fn set_gen_num(&mut self, value: u32) {
        self.gen_num = value;
    }
    fn clone_object(&self) -> ObjectRef {
        let mut visited = HashSet::new();
        visited.insert(self as *const Self as usize);
        self.clone_with_visited(false, &visited)
    }
    fn clone_non_cyclic(&self, direct: bool, visited: &HashSet<usize>) -> Option<ObjectRef> {
        Some(self.clone_with_visited(direct, visited))
    }
    fn get_unicode_text(&self) -> Vec<u32> {
        let mut access = CpdfStreamAcc::new(self);
        access.load_all_data_filtered();
        pdf_decode_text(access.get_span())
    }
    fn write_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        self.write_stream_to(archive, encryptor)
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfStream;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_encryptor::{CpdfEncryptor, CryptoHandler};
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::{retain_object, PdfObject};
    use crate::fxcrt::cfx_read_only_span_stream::CfxReadOnlySpanStream;
    use crate::fxcrt::fx_stream::WriteStream;
    use std::cell::Cell;
    use std::rc::Rc;

    struct Buffer(Vec<u8>);
    impl WriteStream for Buffer {
        fn write_block(&mut self, data: &[u8]) -> bool {
            self.0.extend_from_slice(data);
            true
        }
    }
    struct Handler(Cell<usize>);
    impl CryptoHandler for Handler {
        fn encrypt_content(&self, _obj_num: u32, _gen_num: u32, src: &[u8]) -> Vec<u8> {
            self.0.set(self.0.get() + 1);
            src.iter().map(|value| value ^ 0x20).collect()
        }
    }

    #[test]
    fn raw_data_updates_length_and_filter_dictionary() {
        let mut stream = CpdfStream::from_bytes(b"abc");
        let dict = stream.get_dict();
        assert_eq!(
            dict.borrow()
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_integer_for(b"Length", 0),
            3
        );
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(
                b"Filter",
                Some(retain_object(CpdfName::new(None, b"FlateDecode"))),
            );
        assert!(stream.has_filter());
        stream.set_data_and_remove_filter(b"x");
        assert!(!stream.has_filter());
        assert_eq!(stream.get_in_memory_raw_data(), b"x");
        let file = Rc::new(CfxReadOnlySpanStream::new(b"file"));
        stream.init_stream_from_file(file);
        assert!(stream.is_file_based());
        assert_eq!(stream.read_all_raw_data(), b"file");
    }

    #[test]
    fn metadata_stream_skips_compression_and_data_encryption() {
        let stream = CpdfStream::from_bytes(b"abc");
        let dict = stream.get_dict();
        let mut borrowed = dict.borrow_mut();
        let dict = borrowed
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap();
        dict.set_for(
            b"Type",
            Some(retain_object(CpdfName::new(None, b"Metadata"))),
        );
        dict.set_for(b"Subtype", Some(retain_object(CpdfName::new(None, b"XML"))));
        drop(borrowed);
        let handler = Handler(Cell::new(0));
        let encryptor = CpdfEncryptor::new(&handler, 1);
        let mut buffer = Buffer(Vec::new());
        assert!(stream.write_to(&mut buffer, Some(&encryptor)));
        assert_eq!(handler.0.get(), 0);
        assert!(buffer.0.ends_with(b"stream\r\nabc\r\nendstream"));
        assert!(!buffer
            .0
            .windows(b"FlateDecode".len())
            .any(|part| part == b"FlateDecode"));
        let clone = stream.clone_object();
        assert!(clone
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .is_some());
    }
}
