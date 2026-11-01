// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_flateencoder.{h,cpp}`.

use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_encryptor::CpdfEncryptor;
use super::cpdf_name::CpdfName;
use super::cpdf_object::{retain_object, ObjectRef};
use super::cpdf_scalar_object::CpdfScalarObject;
use super::cpdf_stream::CpdfStream;
use super::cpdf_stream_acc::CpdfStreamAcc;
use crate::fxcodec::flate;
use crate::fxcrt::fx_stream::WriteStream;

enum EncoderData {
    Raw,
    Owned(Vec<u8>),
}

pub struct CpdfFlateEncoder<'a> {
    acc: CpdfStreamAcc<'a>,
    data: EncoderData,
    dict: Option<ObjectRef>,
    cloned_dict: Option<ObjectRef>,
}

impl<'a> CpdfFlateEncoder<'a> {
    pub fn new(stream: &'a CpdfStream, flate_encode: bool) -> Self {
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_raw();
        let has_filter = stream.has_filter();
        let original_dict = stream.get_dict();
        if has_filter && !flate_encode {
            let mut filtered = CpdfStreamAcc::new(stream);
            filtered.load_all_data_filtered();
            let cloned_dict = original_dict.borrow().clone_object();
            cloned_dict
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap()
                .remove_for(b"Filter");
            return Self {
                acc,
                data: EncoderData::Raw,
                dict: None,
                cloned_dict: Some(cloned_dict),
            };
        }
        if has_filter || !flate_encode {
            return Self {
                acc,
                data: EncoderData::Raw,
                dict: Some(original_dict),
                cloned_dict: None,
            };
        }
        let encoded = flate::encode(acc.get_span());
        assert!(!encoded.is_empty(), "Flate encoder returned no data");
        let cloned_dict = original_dict.borrow().clone_object();
        {
            let mut borrowed = cloned_dict.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_for(
                b"Length",
                Some(retain_object(CpdfScalarObject::number_from_signed(
                    i32::try_from(encoded.len()).expect("encoded stream length exceeds int"),
                ))),
            );
            dict.set_for(
                b"Filter",
                Some(retain_object(CpdfName::new(None, b"FlateDecode"))),
            );
            dict.remove_for(b"DecodeParms");
        }
        Self {
            acc,
            data: EncoderData::Owned(encoded),
            dict: None,
            cloned_dict: Some(cloned_dict),
        }
    }

    fn get_dict(&self) -> &ObjectRef {
        self.cloned_dict
            .as_ref()
            .or(self.dict.as_ref())
            .expect("missing Flate encoder dictionary")
    }

    pub fn get_span(&self) -> &[u8] {
        match &self.data {
            EncoderData::Raw => self.acc.get_span(),
            EncoderData::Owned(data) => data,
        }
    }

    pub fn update_length(&mut self, size: usize) {
        let old_size = {
            let borrowed = self.get_dict().borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_integer_for(b"Length", 0) as usize
        };
        if old_size == size {
            return;
        }
        if self.cloned_dict.is_none() {
            self.cloned_dict = Some(self.dict.take().unwrap().borrow().clone_object());
        }
        self.get_dict()
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_for(
                b"Length",
                Some(retain_object(CpdfScalarObject::number_from_signed(
                    size as i32,
                ))),
            );
    }

    pub fn write_dict_to(
        &self,
        archive: &mut dyn WriteStream,
        encryptor: Option<&CpdfEncryptor<'_>>,
    ) -> bool {
        self.get_dict().borrow().write_to(archive, encryptor)
    }
}

#[cfg(test)]
mod tests {
    use super::CpdfFlateEncoder;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;

    #[test]
    fn encoder_updates_cloned_length_and_preserves_original_dictionary() {
        let stream = CpdfStream::from_bytes(b"compress me");
        let original_dict = stream.get_dict();
        let mut encoder = CpdfFlateEncoder::new(&stream, true);
        assert!(encoder.get_span().len() > 0);
        encoder.update_length(31);
        let borrowed = original_dict.borrow();
        let original = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(original.get_integer_for(b"Length", 0), 11);
        assert!(!original.key_exist(b"Filter"));
    }
}
