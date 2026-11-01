// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfdoc/cpdf_filespec.{h,cpp}` file names and embedded-file parameters.
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{get_direct_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_scalar_object::ObjectType;
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fxcrt::fx_codepage::{multi_byte_to_wide_char, FxCodePage};

pub struct CpdfFileSpec {
    object: ObjectRef,
}
pub(crate) fn decode_ansi(bytes: &[u8]) -> Vec<u32> {
    let length = multi_byte_to_wide_char(FxCodePage::DefAnsi, bytes, &mut []);
    let mut result = vec![0; length];
    multi_byte_to_wide_char(FxCodePage::DefAnsi, bytes, &mut result);
    result
}
fn entry(object: &ObjectRef, key: &[u8]) -> Option<ObjectRef> {
    let value = object
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(key)?;
    get_direct_object(&value)
}
fn typed_entry(object: &ObjectRef, key: &[u8], kind: ObjectType) -> Option<ObjectRef> {
    entry(object, key).filter(|object| object.borrow().object_type() == kind)
}
impl CpdfFileSpec {
    pub fn new(object: ObjectRef) -> Self {
        Self { object }
    }
    pub fn encode_file_name(path: &[u32]) -> Vec<u32> {
        if path.is_empty() {
            return Vec::new();
        }
        #[cfg(windows)]
        {
            let slash = |text: &[u32]| {
                text.iter()
                    .map(|&c| if c == 92 || c == 58 { 47 } else { c })
                    .collect::<Vec<_>>()
            };
            if path.get(1) == Some(&58) {
                let mut result = vec![47, path[0]];
                if path.get(2) != Some(&92) {
                    result.push(47);
                }
                result.extend(slash(&path[2..]));
                return result;
            }
            if path.starts_with(&[92, 92]) {
                return slash(&path[1..]);
            }
            if path[0] == 92 {
                let mut result = vec![47];
                result.extend(slash(path));
                return result;
            }
            return slash(path);
        }
        #[cfg(target_vendor = "apple")]
        {
            let mut result = Vec::new();
            if path.starts_with(&[77, 97, 99]) {
                result.push(47);
            }
            result.extend(
                path.iter()
                    .map(|&c| if c == 92 || c == 58 { 47 } else { c }),
            );
            return result;
        }
        #[cfg(not(any(windows, target_vendor = "apple")))]
        {
            path.to_vec()
        }
    }
    pub fn decode_file_name(path: &[u32]) -> Vec<u32> {
        if path.is_empty() {
            return Vec::new();
        }
        #[cfg(windows)]
        {
            let slash = |text: &[u32]| {
                text.iter()
                    .map(|&c| if c == 47 { 92 } else { c })
                    .collect::<Vec<_>>()
            };
            if path[0] != 47 {
                return slash(path);
            }
            if path.get(1) == Some(&47) {
                return slash(&path[1..]);
            }
            if path.get(2) == Some(&47) {
                let mut result = vec![path[1], 58];
                result.extend(slash(&path[2..]));
                return result;
            }
            let mut result = vec![92];
            result.extend(slash(path));
            return result;
        }
        #[cfg(target_vendor = "apple")]
        {
            let path = if path.starts_with(&[47, 77, 97, 99]) {
                &path[1..]
            } else {
                path
            };
            return path.iter().map(|&c| if c == 47 { 58 } else { c }).collect();
        }
        #[cfg(not(any(windows, target_vendor = "apple")))]
        {
            path.to_vec()
        }
    }
    pub fn file_name(&self) -> Vec<u32> {
        let mut name = Vec::new();
        if self.object.borrow().object_type() == ObjectType::Dictionary {
            if let Some(unicode) = typed_entry(&self.object, b"UF", ObjectType::String) {
                name = unicode.borrow().get_unicode_text();
            }
            if name.is_empty() {
                if let Some(ansi) = typed_entry(&self.object, b"F", ObjectType::String) {
                    name = decode_ansi(&ansi.borrow().get_string());
                }
            }
            if entry(&self.object, b"FS").is_some_and(|fs| fs.borrow().get_string() == b"URL") {
                return name;
            }
            if name.is_empty() {
                for key in [b"DOS".as_slice(), b"Mac", b"Unix"] {
                    if let Some(value) = typed_entry(&self.object, key, ObjectType::String) {
                        name = decode_ansi(&value.borrow().get_string());
                        break;
                    }
                }
            }
        } else if self.object.borrow().object_type() == ObjectType::String {
            name = decode_ansi(&self.object.borrow().get_string());
        }
        Self::decode_file_name(&name)
    }
    pub fn file_stream(&self) -> Option<ObjectRef> {
        let files = typed_entry(&self.object, b"EF", ObjectType::Dictionary)?;
        let url = entry(&self.object, b"FS").is_some_and(|fs| fs.borrow().get_string() == b"URL");
        let keys = [b"UF".as_slice(), b"F", b"DOS", b"Mac", b"Unix"];
        for key in keys.into_iter().take(if url { 2 } else { 5 }) {
            if entry(&self.object, key).is_some_and(|v| !v.borrow().get_unicode_text().is_empty()) {
                if let Some(stream) = typed_entry(&files, key, ObjectType::Stream) {
                    return Some(stream);
                }
            }
        }
        None
    }
    pub fn params_dict(&self) -> Option<ObjectRef> {
        let stream = self.file_stream()?;
        let dictionary = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()?
            .get_dict();
        typed_entry(&dictionary, b"Params", ObjectType::Dictionary)
    }
    pub fn mutable_params_dict(&self) -> Option<ObjectRef> {
        self.params_dict()
    }
    pub fn file_spec_dict(&self) -> Option<ObjectRef> {
        (self.object.borrow().object_type() == ObjectType::Dictionary).then(|| self.object.clone())
    }
    pub fn mutable_file_spec_dict(&self) -> Option<ObjectRef> {
        self.file_spec_dict()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    #[test]
    fn filename_precedence_url_and_embedded_stream_parameters() {
        let mut dict = CpdfDictionary::new();
        dict.set_new_for(b"F", CpdfString::from_bytes(None, b"fallback.pdf"));
        dict.set_new_for(
            b"UF",
            CpdfString::from_unicode(None, &[0x4e2d, 46, 112, 100, 102]),
        );
        let object = retain_object(dict);
        let spec = CpdfFileSpec::new(object.clone());
        assert_eq!(spec.file_name(), [0x4e2d, 46, 112, 100, 102]);
        let mut embedded = CpdfDictionary::new();
        embedded.set_new_for(b"Params", CpdfDictionary::new());
        let stream = retain_object(CpdfStream::from_data_and_dict(
            vec![1, 2, 3],
            retain_object(embedded),
        ));
        let holder =
            crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder::new();
        holder.borrow_mut().add_indirect_object(stream.clone());
        let mut files = CpdfDictionary::new();
        files.set_for(
            b"UF",
            Some(crate::fpdfapi::parser::cpdf_object::make_reference(
                &stream, &holder,
            )),
        );
        object
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
            .unwrap()
            .set_new_for(b"EF", files);
        assert!(std::rc::Rc::ptr_eq(&spec.file_stream().unwrap(), &stream));
        assert!(spec.params_dict().is_some());
    }
}

#[cfg(test)]
#[path = "cpdf_filespec_unittest.rs"]
mod upstream_tests;
