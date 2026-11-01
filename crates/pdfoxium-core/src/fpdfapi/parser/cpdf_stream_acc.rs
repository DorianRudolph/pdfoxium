// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_stream_acc.{h,cpp}`. The stream borrow lasts for
//! the accessor's lifetime, replacing PDFium's retained stream pointer.

use sha1::{Digest, Sha1};
use std::cell::Ref;
use std::ops::Deref;
use std::rc::Rc;

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_object::ObjectRef;
use super::cpdf_stream::CpdfStream;
use super::fpdf_parser_decode::{get_decoder_array, pdf_data_decode};

enum AccData {
    Empty,
    BorrowedMemory,
    Owned(Vec<u8>),
}

pub struct CpdfStreamAcc<'a> {
    stream: &'a CpdfStream,
    image_decoder: Vec<u8>,
    image_param: Option<ObjectRef>,
    data: AccData,
}

impl<'a> CpdfStreamAcc<'a> {
    pub fn new(stream: &'a CpdfStream) -> Self {
        Self {
            stream,
            image_decoder: Vec::new(),
            image_param: None,
            data: AccData::Empty,
        }
    }

    pub fn get_stream(&self) -> &'a CpdfStream {
        self.stream
    }
    pub fn get_image_param(&self) -> Option<ObjectRef> {
        self.image_param.clone()
    }
    pub fn get_image_decoder(&self) -> &[u8] {
        &self.image_decoder
    }

    pub fn get_length1_for_test(&self) -> i32 {
        let dict = self.stream.get_dict();
        let borrowed = dict.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .get_integer_for(b"Length1", 0)
    }

    pub fn get_span(&self) -> &[u8] {
        match &self.data {
            AccData::Empty => &[],
            AccData::BorrowedMemory => self.stream.get_in_memory_raw_data(),
            AccData::Owned(data) => data,
        }
    }

    pub fn get_size(&self) -> u32 {
        self.get_span().len() as u32
    }

    pub fn key_for_cache(&self) -> u64 {
        if self.stream.obj_num() == 0 {
            (1u64 << 63) | (self.stream as *const CpdfStream as usize as u64)
        } else {
            (u64::from(self.stream.obj_num()) << 32) | u64::from(self.stream.gen_num())
        }
    }

    pub fn compute_digest(&self) -> Vec<u8> {
        Sha1::digest(self.get_span()).to_vec()
    }

    pub fn detach_data(&mut self) -> Vec<u8> {
        match std::mem::replace(&mut self.data, AccData::Empty) {
            AccData::Empty => Vec::new(),
            AccData::BorrowedMemory => self.stream.get_in_memory_raw_data().to_vec(),
            AccData::Owned(data) => data,
        }
    }

    /// Stable shared font data without copying the raw fallback when a filter
    /// failed. The stream owns Rc-backed memory; decoded storage moves here.
    pub fn detach_shared_data(&mut self) -> Rc<Vec<u8>> {
        match std::mem::replace(&mut self.data, AccData::Empty) {
            AccData::Empty => Rc::new(Vec::new()),
            AccData::BorrowedMemory => self
                .stream
                .share_in_memory_raw_data()
                .expect("memory stream"),
            AccData::Owned(data) => Rc::new(data),
        }
    }

    pub fn load_all_data_raw(&mut self) {
        self.load_all_data(true, 0, false);
    }
    pub fn load_all_data_filtered(&mut self) {
        self.load_all_data(false, 0, false);
    }
    pub fn load_all_data_filtered_with_estimated_size(&mut self, estimated_size: u32) {
        self.load_all_data(false, estimated_size, false);
    }
    pub fn load_all_data_image_acc(&mut self, estimated_size: u32) {
        self.load_all_data(false, estimated_size, true);
    }

    fn load_all_data(&mut self, raw: bool, estimated_size: u32, image_acc: bool) {
        if raw {
            assert_eq!(estimated_size, 0);
            assert!(!image_acc);
        }
        if self.stream.get_raw_size() == 0 {
            return;
        }
        let memory_based = self.stream.is_memory_based();
        if raw || !self.stream.has_filter() {
            self.data = if memory_based {
                AccData::BorrowedMemory
            } else {
                let data = self.stream.read_all_raw_data();
                if data.is_empty() {
                    return;
                }
                AccData::Owned(data)
            };
            return;
        }
        let dict = self.stream.get_dict();
        let decoders = {
            let borrowed = dict.borrow();
            get_decoder_array(borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap())
        };
        let raw_data = if memory_based {
            Vec::new()
        } else {
            self.stream.read_all_raw_data()
        };
        if !memory_based && raw_data.is_empty() {
            return;
        }
        let source = if memory_based {
            self.stream.get_in_memory_raw_data()
        } else {
            &raw_data
        };
        let Some(decoders) = decoders else {
            self.data = if memory_based {
                AccData::BorrowedMemory
            } else {
                AccData::Owned(raw_data)
            };
            return;
        };
        if decoders.is_empty() {
            self.data = if memory_based {
                AccData::BorrowedMemory
            } else {
                AccData::Owned(raw_data)
            };
            return;
        }
        let Some(result) = pdf_data_decode(source, estimated_size, image_acc, &decoders) else {
            self.data = if memory_based {
                AccData::BorrowedMemory
            } else {
                AccData::Owned(raw_data)
            };
            return;
        };
        self.image_decoder = result.image_encoding;
        self.image_param = result.image_params;
        self.data = if !result.data_was_decoded {
            if memory_based {
                AccData::BorrowedMemory
            } else {
                AccData::Owned(raw_data)
            }
        } else {
            AccData::Owned(result.data)
        };
    }
}

/// Retained image accessor. Borrowed memory stays attached to the PDF stream;
/// filtered data moves from the existing accessor without a pixel/data copy.
/// This supplies CPDF_DIB's lifetime beyond the borrow used to load its stream.
enum RetainedAccData {
    Empty,
    BorrowedMemory,
    Owned(Rc<Vec<u8>>),
}

pub struct CpdfRetainedStreamAcc {
    stream: ObjectRef,
    data: RetainedAccData,
    image_decoder: Vec<u8>,
    image_param: Option<ObjectRef>,
}

pub enum RetainedStreamSpan<'a> {
    Stream(Ref<'a, [u8]>),
    Data(&'a [u8]),
}

impl Deref for RetainedStreamSpan<'_> {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        match self {
            Self::Stream(span) => span,
            Self::Data(span) => span,
        }
    }
}

impl CpdfRetainedStreamAcc {
    pub fn key_for_cache(&self) -> u64 {
        let borrowed = self.stream.borrow();
        let stream = borrowed
            .as_any()
            .downcast_ref::<CpdfStream>()
            .expect("retained stream");
        if stream.obj_num() == 0 {
            (1u64 << 63) | (stream as *const CpdfStream as usize as u64)
        } else {
            (u64::from(stream.obj_num()) << 32) | u64::from(stream.gen_num())
        }
    }
    pub fn load_image(stream: ObjectRef, estimated_size: u32) -> Option<Self> {
        Self::load(stream, false, estimated_size, true)
    }
    pub fn load_filtered(stream: ObjectRef) -> Option<Self> {
        Self::load(stream, false, 0, false)
    }
    pub fn load_raw(stream: ObjectRef) -> Option<Self> {
        Self::load(stream, true, 0, false)
    }
    fn load(stream: ObjectRef, raw: bool, estimated_size: u32, image: bool) -> Option<Self> {
        let (data, image_decoder, image_param) = {
            let borrowed = stream.borrow();
            let source = borrowed.as_any().downcast_ref::<CpdfStream>()?;
            let mut acc = CpdfStreamAcc::new(source);
            acc.load_all_data(raw, estimated_size, image);
            (acc.data, acc.image_decoder, acc.image_param)
        };
        Some(Self {
            stream,
            data: match data {
                AccData::Empty => RetainedAccData::Empty,
                AccData::BorrowedMemory => RetainedAccData::BorrowedMemory,
                AccData::Owned(data) => RetainedAccData::Owned(Rc::new(data)),
            },
            image_decoder,
            image_param,
        })
    }

    pub fn get_span(&self) -> RetainedStreamSpan<'_> {
        match &self.data {
            RetainedAccData::Empty => RetainedStreamSpan::Data(&[]),
            RetainedAccData::Owned(data) => RetainedStreamSpan::Data(data),
            RetainedAccData::BorrowedMemory => {
                RetainedStreamSpan::Stream(Ref::map(self.stream.borrow(), |object| {
                    object
                        .as_any()
                        .downcast_ref::<CpdfStream>()
                        .expect("image stream")
                        .get_in_memory_raw_data()
                }))
            }
        }
    }

    /// Share the existing source allocation with a long-lived image decoder.
    /// Filtered bytes moved into Rc at load; raw bytes remain stream-owned.
    pub fn share_data(&self) -> Rc<Vec<u8>> {
        match &self.data {
            RetainedAccData::Empty => Rc::new(Vec::new()),
            RetainedAccData::Owned(data) => Rc::clone(data),
            RetainedAccData::BorrowedMemory => self
                .stream
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()
                .expect("retained image stream")
                .share_in_memory_raw_data()
                .expect("memory stream"),
        }
    }

    pub fn get_stream(&self) -> &ObjectRef {
        &self.stream
    }

    pub fn get_image_decoder(&self) -> &[u8] {
        &self.image_decoder
    }
    pub fn get_image_param(&self) -> Option<ObjectRef> {
        self.image_param.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfStreamAcc;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_name::CpdfName;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fxcodec::flate;

    #[test]
    fn retained_image_access_borrows_original_stream_storage() {
        let stream = retain_object(CpdfStream::from_bytes(&[1, 2, 3]));
        let pointer = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_in_memory_raw_data()
            .as_ptr();
        let weak = std::rc::Rc::downgrade(&stream);
        let cache_key = {
            let borrowed = stream.borrow();
            CpdfStreamAcc::new(borrowed.as_any().downcast_ref::<CpdfStream>().unwrap())
                .key_for_cache()
        };
        let acc = super::CpdfRetainedStreamAcc::load_image(stream, 3).unwrap();
        assert_eq!(acc.key_for_cache(), cache_key);
        assert_ne!(cache_key & (1u64 << 63), 0);
        assert_eq!(acc.get_span().as_ptr(), pointer);
        assert_eq!(acc.share_data().as_ptr(), pointer);
        assert_eq!(&*acc.get_span(), &[1, 2, 3]);
        drop(acc);
        assert!(weak.upgrade().is_none());
    }

    #[test]
    fn filtered_stream_decodes_and_raw_access_keeps_compressed_bytes() {
        let compressed = flate::encode(b"stream data");
        let stream = CpdfStream::from_bytes(&compressed);
        let dict = stream.get_dict();
        dict.borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(
                b"Filter",
                Some(retain_object(CpdfName::new(None, b"FlateDecode"))),
            );
        let mut access = CpdfStreamAcc::new(&stream);
        access.load_all_data_filtered();
        assert_eq!(access.get_span(), b"stream data");
        assert_eq!(access.compute_digest().len(), 20);
        access.load_all_data_raw();
        assert_eq!(access.get_span(), compressed);
        assert_eq!(access.detach_data(), compressed);
    }

    #[test]
    fn decoded_empty_stream_does_not_revert_to_encoded_bytes() {
        for (filter, source) in [
            (b"FlateDecode".as_slice(), flate::encode(b"")),
            (b"ASCIIHexDecode".as_slice(), b">".to_vec()),
        ] {
            let stream = CpdfStream::from_bytes(&source);
            stream
                .get_dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_for(b"Filter", Some(retain_object(CpdfName::new(None, filter))));
            let mut access = CpdfStreamAcc::new(&stream);
            access.load_all_data_filtered();
            assert!(access.get_span().is_empty());
            access.load_all_data_raw();
            assert_eq!(access.get_span(), source);
        }
    }

    #[test]
    fn deferred_images_and_already_applied_crypt_keep_source_data() {
        for filter in [b"DCTDecode".as_slice(), b"Crypt".as_slice()] {
            let source = b"source supplied to the remaining decoder";
            let stream = CpdfStream::from_bytes(source);
            stream
                .get_dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .set_for(b"Filter", Some(retain_object(CpdfName::new(None, filter))));
            let mut access = CpdfStreamAcc::new(&stream);
            access.load_all_data_filtered();
            assert_eq!(access.get_span(), source);
        }
    }
}

#[cfg(test)]
#[path = "cpdf_stream_acc_unittest.rs"]
mod upstream_tests;
