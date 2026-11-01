// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Shared script-global storage and the RC4 persistence format from
//! `fxjs/cfx_globaldata.{h,cpp}`. Rc/Weak replaces manual retained-instance
//! accounting; delegates and individual elements retain source observer
//! lifetimes. No store or element borrow spans a persistence callback.
use super::cfx_keyvalue::{CfxKeyValue, CfxValue, DataType};
use crate::fdrm::fx_crypt::crypt_arc_four_crypt_block;
use std::cell::RefCell;
use std::rc::{Rc, Weak};

const MIN_GLOBAL_DATA_BYTES: usize = 12;
const MAX_GLOBAL_DATA_BYTES: usize = 4 * 1024 - 8;
const MAGIC: u16 = (b'X' as u16) << 8 | b'F' as u16;
const MAX_VERSION: u16 = 2;
const RC4_KEY: &[u8] = &[
    0x19, 0xa8, 0xe8, 0x01, 0xf6, 0xa8, 0xb6, 0x4d, 0x82, 0x04, 0x45, 0x6d, 0xb4, 0xcf, 0xd7, 0x77,
    0x67, 0xf9, 0x75, 0x9f, 0xf0, 0xe0, 0x1e, 0x51, 0xee, 0x46, 0xfd, 0x0b, 0xc9, 0x93, 0x25, 0x55,
    0x4a, 0xee, 0xe0, 0x16, 0xd0, 0xdf, 0x8c, 0xfa, 0x2a, 0xa9, 0x49, 0xfd, 0x97, 0x1c, 0x0e, 0x22,
    0x13, 0x28, 0x7c, 0xaf, 0xc4, 0xfc, 0x9c, 0x12, 0x65, 0x8c, 0x4e, 0x5b, 0x04, 0x75, 0x89, 0xc9,
    0xb1, 0xed, 0x50, 0xca, 0x96, 0x6f, 0x1a, 0x7a, 0xfe, 0x58, 0x5d, 0xec, 0x19, 0x4a, 0xf6, 0x35,
    0x6a, 0x97, 0x14, 0x00, 0x0e, 0xd0, 0x6b, 0xbb, 0xd5, 0x75, 0x55, 0x8b, 0x6e, 0x6b, 0x19, 0xa0,
    0xf8, 0x77, 0xd5, 0xa3,
];

/// LoadBuffer transfers its writable bytes to Rust rather than lending a raw
/// span. BufferDone ends each nonempty load lease, including failed decoding.
/// The caller retains the delegate; a store only observes it weakly.
pub trait GlobalDataDelegate {
    fn store_buffer(&self, buffer: &[u8]) -> bool;
    fn load_buffer(&self) -> Option<Vec<u8>>;
    fn buffer_done(&self);
}

#[derive(Debug, Default, PartialEq)]
pub struct Element {
    pub data: CfxKeyValue,
    pub persistent: bool,
}

thread_local! {
    static INSTANCE: RefCell<Weak<CfxGlobalData>> = const { RefCell::new(Weak::new()) };
}

pub struct CfxGlobalData {
    delegate: Option<Weak<dyn GlobalDataDelegate>>,
    elements: RefCell<Vec<Rc<RefCell<Element>>>>,
}
impl CfxGlobalData {
    pub fn get_retained_instance(delegate: Option<Rc<dyn GlobalDataDelegate>>) -> Rc<Self> {
        if let Some(instance) = INSTANCE.with(|instance| instance.borrow().upgrade()) {
            return instance;
        }
        let instance = Rc::new(Self::new(delegate.as_ref()));
        // Publish before LoadBuffer, which may itself reenter the shared store.
        INSTANCE.with(|shared| *shared.borrow_mut() = Rc::downgrade(&instance));
        instance.load_global_persistent_variables();
        instance
    }
    fn new(delegate: Option<&Rc<dyn GlobalDataDelegate>>) -> Self {
        Self {
            delegate: delegate.map(Rc::downgrade),
            elements: RefCell::new(Vec::new()),
        }
    }
    /// Rust ownership performs the release. The result retains the source
    /// test contract: true means this was the last retained instance.
    pub fn release(self: Rc<Self>) -> bool {
        let last = Rc::strong_count(&self) == 1;
        drop(self);
        last
    }
    pub fn get_size(&self) -> i32 {
        i32::try_from(self.elements.borrow().len()).unwrap_or(i32::MAX)
    }
    pub fn get_at(&self, index: i32) -> Option<Rc<RefCell<Element>>> {
        self.elements
            .borrow()
            .get(usize::try_from(index).ok()?)
            .cloned()
    }
    pub fn get_global_variable(&self, name: &[u8]) -> Option<Rc<RefCell<Element>>> {
        self.elements
            .borrow()
            .iter()
            .find(|element| element.borrow().data.key == name)
            .cloned()
    }
    pub fn entries(&self) -> Vec<Rc<RefCell<Element>>> {
        self.elements.borrow().clone()
    }
    fn set_global_variable(&self, name: &[u8], value: CfxValue) {
        let name = trim_prop_name(name);
        if name.is_empty() {
            return;
        }
        if let Some(element) = self.get_global_variable(name) {
            element.borrow_mut().data.value = value;
            return;
        }
        self.elements
            .borrow_mut()
            .push(Rc::new(RefCell::new(Element {
                data: CfxKeyValue {
                    key: name.to_vec(),
                    value,
                },
                persistent: false,
            })));
    }
    pub fn set_global_variable_number(&self, name: &[u8], value: f64) {
        self.set_global_variable(name, CfxValue::Number(value));
    }
    pub fn set_global_variable_boolean(&self, name: &[u8], value: bool) {
        self.set_global_variable(name, CfxValue::Boolean(value));
    }
    pub fn set_global_variable_string(&self, name: &[u8], value: &[u8]) {
        if trim_prop_name(name).is_empty() {
            return;
        }
        self.set_global_variable(name, CfxValue::String(value.to_vec()));
    }
    pub fn set_global_variable_object(&self, name: &[u8], value: Vec<CfxKeyValue>) {
        self.set_global_variable(name, CfxValue::Object(value));
    }
    pub fn set_global_variable_null(&self, name: &[u8]) {
        self.set_global_variable(name, CfxValue::Null);
    }
    pub fn set_global_variable_persistent(&self, name: &[u8], persistent: bool) -> bool {
        let name = trim_prop_name(name);
        if name.is_empty() {
            return false;
        }
        let Some(element) = self.get_global_variable(name) else {
            return false;
        };
        element.borrow_mut().persistent = persistent;
        true
    }
    pub fn delete_global_variable(&self, name: &[u8]) -> bool {
        let name = trim_prop_name(name);
        if name.is_empty() {
            return false;
        }
        let mut elements = self.elements.borrow_mut();
        let Some(index) = elements
            .iter()
            .position(|element| element.borrow().data.key == name)
        else {
            return false;
        };
        elements.remove(index);
        true
    }
    /// Persist the current shared data explicitly as well as at last release.
    /// Script bindings first copy their live VM values into this store, without
    /// carrying any VM/map borrow into this delegate call.
    pub fn commit(&self) -> bool {
        let Some(delegate) = self.delegate.as_ref().and_then(Weak::upgrade) else {
            return false;
        };
        let buffer = self.make_persistent_buffer();
        delegate.store_buffer(&buffer)
    }
    fn make_persistent_buffer(&self) -> Vec<u8> {
        let mut data = Vec::new();
        let mut count = 0u32;
        for element in self.elements.borrow().iter() {
            let element = element.borrow();
            if !element.persistent {
                continue;
            }
            let value = &element.data.value;
            let extra = match value {
                CfxValue::Number(_) => 8,
                CfxValue::Boolean(_) => 2,
                CfxValue::String(text) => match text.len().checked_add(4) {
                    Some(size) => size,
                    None => break,
                },
                CfxValue::Null => 0,
                // Arrays/objects are shared between live runtimes, but the
                // source persistence format deliberately excludes them.
                CfxValue::Object(_) => continue,
            };
            let Some(size) = element
                .data
                .key
                .len()
                .checked_add(6)
                .and_then(|size| size.checked_add(extra))
            else {
                break;
            };
            if size > MAX_GLOBAL_DATA_BYTES - data.len() {
                break;
            }
            data.extend_from_slice(&(element.data.key.len() as u32).to_le_bytes());
            data.extend_from_slice(&element.data.key);
            data.extend_from_slice(&(value.data_type() as u16).to_le_bytes());
            match value {
                CfxValue::Number(number) => data.extend_from_slice(&number.to_le_bytes()),
                CfxValue::Boolean(boolean) => {
                    data.extend_from_slice(&u16::from(*boolean).to_le_bytes())
                }
                CfxValue::String(text) => {
                    data.extend_from_slice(&(text.len() as u32).to_le_bytes());
                    data.extend_from_slice(text);
                }
                CfxValue::Null => {}
                CfxValue::Object(_) => unreachable!("excluded above"),
            }
            count += 1;
        }
        let mut file = Vec::with_capacity(MIN_GLOBAL_DATA_BYTES + data.len());
        file.extend_from_slice(&MAGIC.to_le_bytes());
        file.extend_from_slice(&MAX_VERSION.to_le_bytes());
        file.extend_from_slice(&count.to_le_bytes());
        file.extend_from_slice(&(data.len() as u32).to_le_bytes());
        file.extend(data);
        crypt_arc_four_crypt_block(&mut file, RC4_KEY);
        file
    }
    fn load_global_persistent_variables(&self) -> bool {
        let Some(delegate) = self.delegate.as_ref().and_then(Weak::upgrade) else {
            return false;
        };
        let Some(mut buffer) = delegate.load_buffer() else {
            return false;
        };
        if buffer.is_empty() {
            return false;
        }
        struct BufferDone<'a>(&'a dyn GlobalDataDelegate);
        impl Drop for BufferDone<'_> {
            fn drop(&mut self) {
                self.0.buffer_done();
            }
        }
        let done = BufferDone(delegate.as_ref());
        let result = self.load_global_persistent_variables_from_buffer(&mut buffer);
        drop(buffer);
        drop(done);
        result
    }
    fn load_global_persistent_variables_from_buffer(&self, buffer: &mut [u8]) -> bool {
        if buffer.len() < MIN_GLOBAL_DATA_BYTES {
            return false;
        }
        crypt_arc_four_crypt_block(buffer, RC4_KEY);
        let Some(entries) = parse_persistent_buffer(buffer) else {
            return false;
        };
        // Validate the whole record before changing shared state. Malformed
        // inputs cannot leave a partially imported set of globals behind.
        for entry in entries {
            self.set_global_variable(&entry.key, entry.value);
            self.set_global_variable_persistent(&entry.key, true);
        }
        true
    }
}
impl Drop for CfxGlobalData {
    fn drop(&mut self) {
        self.commit();
    }
}

fn trim_prop_name(name: &[u8]) -> &[u8] {
    let whitespace = |c: u8| matches!(c, 9..=13 | 32);
    let start = name
        .iter()
        .position(|&c| !whitespace(c))
        .unwrap_or(name.len());
    let end = name
        .iter()
        .rposition(|&c| !whitespace(c))
        .map_or(start, |index| index + 1);
    &name[start..end]
}
struct Reader<'a>(&'a [u8]);
impl<'a> Reader<'a> {
    fn take(&mut self, length: usize) -> Option<&'a [u8]> {
        let result = self.0.get(..length)?;
        self.0 = &self.0[length..];
        Some(result)
    }
    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
}
fn parse_persistent_buffer(buffer: &[u8]) -> Option<Vec<CfxKeyValue>> {
    let mut reader = Reader(buffer);
    if reader.u16()? != MAGIC {
        return None;
    }
    let version = reader.u16()?;
    // Version zero has no defined numeric decoding in the source switch.
    if !(1..=MAX_VERSION).contains(&version) {
        return None;
    }
    let count = reader.u32()? as usize;
    let size = reader.u32()? as usize;
    if size != reader.0.len() || count > size / 6 {
        return None;
    }
    let mut result = Vec::with_capacity(count);
    for _ in 0..count {
        let length = reader.u32()? as usize;
        let key = reader.take(length)?.to_vec();
        let value = match reader.u16()? {
            x if x == DataType::Number as u16 => {
                let number = if version == 1 {
                    f64::from(reader.u32()?)
                } else {
                    f64::from_le_bytes(reader.take(8)?.try_into().ok()?)
                };
                CfxValue::Number(number)
            }
            x if x == DataType::Boolean as u16 => CfxValue::Boolean(reader.u16()? == 1),
            x if x == DataType::String as u16 => {
                let length = reader.u32()? as usize;
                CfxValue::String(reader.take(length)?.to_vec())
            }
            x if x == DataType::Null as u16 => CfxValue::Null,
            // Objects and unknown tags are invalid in persistent buffers.
            _ => return None,
        };
        result.push(CfxKeyValue { key, value });
    }
    reader.0.is_empty().then_some(result)
}

#[cfg(test)]
#[allow(non_snake_case)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[derive(Default)]
    struct TestDelegate {
        last_buffer: RefCell<Vec<u8>>,
        done: Cell<usize>,
    }
    impl GlobalDataDelegate for TestDelegate {
        fn store_buffer(&self, buffer: &[u8]) -> bool {
            *self.last_buffer.borrow_mut() = buffer.to_vec();
            true
        }
        fn load_buffer(&self) -> Option<Vec<u8>> {
            Some(std::mem::take(&mut *self.last_buffer.borrow_mut()))
        }
        fn buffer_done(&self) {
            self.last_buffer.borrow_mut().clear();
            self.done.set(self.done.get() + 1);
        }
    }
    fn assert_element(instance: &CfxGlobalData, index: i32, key: &[u8], value: CfxValue) {
        let element = instance.get_at(index).expect("source ASSERT_TRUE(element)");
        let element = element.borrow();
        assert_eq!(element.data.key, key);
        assert_eq!(element.data.value.data_type(), value.data_type());
        assert_eq!(element.data.value, value);
    }
    // All inputs and assertions from `fxjs/cfx_globaldata_unittest.cpp`.
    #[test]
    fn GetSafety() {
        let instance = CfxGlobalData::get_retained_instance(None);
        assert!(instance.get_global_variable(b"nonesuch").is_none());
        assert!(instance.get_at(-1).is_none());
        assert!(instance.get_at(0).is_none());
        assert!(instance.get_at(1).is_none());
        instance.set_global_variable_number(b"double", 2.0);
        instance.set_global_variable_string(b"string", b"clams");
        assert!(instance.get_global_variable(b"nonesuch").is_none());
        assert!(instance.get_at(-1).is_none());
        assert!(Rc::ptr_eq(
            &instance.get_global_variable(b"double").unwrap(),
            &instance.get_at(0).unwrap()
        ));
        assert!(Rc::ptr_eq(
            &instance.get_global_variable(b"string").unwrap(),
            &instance.get_at(1).unwrap()
        ));
        assert!(instance.get_at(2).is_none());
        assert!(instance.release());
    }
    #[test]
    fn StoreReload() {
        let delegate = Rc::new(TestDelegate::default());
        let interface: Rc<dyn GlobalDataDelegate> = delegate.clone();
        let instance = CfxGlobalData::get_retained_instance(Some(interface.clone()));
        instance.set_global_variable_number(b"double", 2.0);
        instance.set_global_variable_string(b"string", b"clams");
        instance.set_global_variable_boolean(b"boolean", true);
        instance.set_global_variable_null(b"null");
        instance.set_global_variable_object(b"array", Vec::new());
        for name in [
            b"double".as_slice(),
            b"string",
            b"boolean",
            b"null",
            b"array",
        ] {
            assert!(instance.set_global_variable_persistent(name, true));
        }
        assert!(instance.release());
        let instance = CfxGlobalData::get_retained_instance(Some(interface));
        assert_element(&instance, 0, b"double", CfxValue::Number(2.0));
        assert_element(&instance, 1, b"string", CfxValue::String(b"clams".to_vec()));
        assert_element(&instance, 2, b"boolean", CfxValue::Boolean(true));
        assert_element(&instance, 3, b"null", CfxValue::Null);
        // Arrays/objects deliberately do not appear in the file.
        assert!(instance.get_at(4).is_none());
        assert!(instance
            .entries()
            .iter()
            .all(|element| element.borrow().persistent));
        assert_eq!(delegate.done.get(), 1);
        assert!(instance.release());
    }
    #[test]
    fn ResetValues() {
        let instance = CfxGlobalData::get_retained_instance(None);
        for name in [b"double".as_slice(), b"string", b"boolean", b"null"] {
            instance.set_global_variable_string(name, b"bogus!!!");
        }
        instance.set_global_variable_number(b"double", 2.0);
        instance.set_global_variable_string(b"string", b"clams");
        instance.set_global_variable_boolean(b"boolean", true);
        instance.set_global_variable_null(b"null");
        assert_element(&instance, 0, b"double", CfxValue::Number(2.0));
        assert_element(&instance, 1, b"string", CfxValue::String(b"clams".to_vec()));
        assert_element(&instance, 2, b"boolean", CfxValue::Boolean(true));
        assert_element(&instance, 3, b"null", CfxValue::Null);
        assert!(instance.release());
    }
    #[test]
    fn DeleteValues() {
        let instance = CfxGlobalData::get_retained_instance(None);
        instance.set_global_variable_number(b"double", 2.0);
        instance.set_global_variable_string(b"string", b"clams");
        instance.set_global_variable_boolean(b"boolean", true);
        instance.set_global_variable_null(b"null");
        assert_eq!(instance.get_size(), 4);
        assert!(!instance.delete_global_variable(b"nonesuch"));
        assert_eq!(instance.get_size(), 4);
        for (name, remaining) in [
            (b"boolean".as_slice(), 3),
            (b"string", 2),
            (b"double", 1),
            (b"null", 0),
        ] {
            assert!(instance.delete_global_variable(name));
            assert_eq!(instance.get_size(), remaining);
        }
        assert!(instance.release());
    }

    fn frame(version: u16, count: u32, payload: &[u8]) -> Vec<u8> {
        let mut buffer = Vec::new();
        buffer.extend_from_slice(&MAGIC.to_le_bytes());
        buffer.extend_from_slice(&version.to_le_bytes());
        buffer.extend_from_slice(&count.to_le_bytes());
        buffer.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        buffer.extend_from_slice(payload);
        buffer
    }
    fn encrypted(mut plain: Vec<u8>) -> Vec<u8> {
        crypt_arc_four_crypt_block(&mut plain, RC4_KEY);
        plain
    }
    fn record(name: &[u8], tag: u16, data: &[u8]) -> Vec<u8> {
        let mut result = Vec::new();
        result.extend_from_slice(&(name.len() as u32).to_le_bytes());
        result.extend_from_slice(name);
        result.extend_from_slice(&tag.to_le_bytes());
        result.extend_from_slice(data);
        result
    }
    #[test]
    fn malformed_encrypted_buffers_are_checked_completely_before_import() {
        let target = CfxGlobalData::new(None);
        target.set_global_variable_number(b"sentinel", 7.0);
        let payload = record(b"number", DataType::Number as u16, &2.0f64.to_le_bytes());
        let valid = frame(2, 1, &payload);
        let mut malformed = Vec::new();
        // Every truncated prefix, including prefixes whose declared payload
        // size has been adjusted to pass the header size check.
        for length in 0..valid.len() {
            malformed.push(valid[..length].to_vec());
            if length >= MIN_GLOBAL_DATA_BYTES {
                let mut adjusted = valid[..length].to_vec();
                adjusted[8..12]
                    .copy_from_slice(&((length - MIN_GLOBAL_DATA_BYTES) as u32).to_le_bytes());
                malformed.push(adjusted);
            }
        }
        let mut wrong_magic = valid.clone();
        wrong_magic[0] ^= 1;
        malformed.push(wrong_magic);
        malformed.extend([
            frame(0, 1, &payload),
            frame(3, 1, &payload),
            frame(2, 0, &payload),
            frame(2, 2, &payload),
            frame(2, u32::MAX, &payload),
        ]);
        malformed.push(frame(
            2,
            1,
            &record(b"object", DataType::Object as u16, &[]),
        ));
        malformed.push(frame(2, 1, &record(b"unknown", u16::MAX, &[])));
        malformed.push(frame(
            2,
            1,
            &record(b"string", DataType::String as u16, &u32::MAX.to_le_bytes()),
        ));
        malformed.push(frame(2, 1, &u32::MAX.to_le_bytes()));
        let mut trailing = payload.clone();
        trailing.push(0);
        malformed.push(frame(2, 1, &trailing));
        let mut two_entries = payload.clone();
        two_entries.extend(record(b"bad-last-entry", DataType::Number as u16, &[0]));
        malformed.push(frame(2, 2, &two_entries));
        for (index, plain) in malformed.into_iter().enumerate() {
            assert!(
                !target.load_global_persistent_variables_from_buffer(&mut encrypted(plain)),
                "malformed record {index}"
            );
            assert_eq!(target.get_size(), 1);
            assert_element(&target, 0, b"sentinel", CfxValue::Number(7.0));
        }
        let delegate = Rc::new(TestDelegate::default());
        *delegate.last_buffer.borrow_mut() = encrypted(frame(2, 1, &two_entries));
        let interface: Rc<dyn GlobalDataDelegate> = delegate.clone();
        let target = CfxGlobalData::new(Some(&interface));
        assert!(!target.load_global_persistent_variables());
        assert_eq!(target.get_size(), 0);
        assert_eq!(delegate.done.get(), 1);
    }
    #[test]
    fn version_one_and_two_preserve_numeric_bytes_boolean_tags_and_raw_strings() {
        let target = CfxGlobalData::new(None);
        let mut payload = record(b"legacy", DataType::Number as u16, &u32::MAX.to_le_bytes());
        payload.extend(record(
            b"not-one",
            DataType::Boolean as u16,
            &2u16.to_le_bytes(),
        ));
        assert!(target
            .load_global_persistent_variables_from_buffer(&mut encrypted(frame(1, 2, &payload))));
        assert_element(&target, 0, b"legacy", CfxValue::Number(f64::from(u32::MAX)));
        assert_element(&target, 1, b"not-one", CfxValue::Boolean(false));
        let bytes = b"\0\xed\xa0\x80\xf0\x9d\x84\x9e\xed\xb0\x80";
        target.set_global_variable_number(b"negative-zero", -0.0);
        target.set_global_variable_number(b"nan", f64::from_bits(0x7ff8_0000_0000_1234));
        target.set_global_variable_string(b"raw", bytes);
        for name in [b"negative-zero".as_slice(), b"nan", b"raw"] {
            target.set_global_variable_persistent(name, true);
        }
        let output = target.make_persistent_buffer();
        let reloaded = CfxGlobalData::new(None);
        assert!(reloaded.load_global_persistent_variables_from_buffer(&mut output.clone()));
        let zero = reloaded.get_global_variable(b"negative-zero").unwrap();
        let CfxValue::Number(zero) = zero.borrow().data.value else {
            panic!("not a number");
        };
        assert_eq!(zero.to_bits(), (-0.0f64).to_bits());
        let nan = reloaded.get_global_variable(b"nan").unwrap();
        let CfxValue::Number(nan) = nan.borrow().data.value else {
            panic!("not a number");
        };
        assert_eq!(nan.to_bits(), 0x7ff8_0000_0000_1234);
        assert_eq!(
            reloaded
                .get_global_variable(b"raw")
                .unwrap()
                .borrow()
                .data
                .value,
            CfxValue::String(bytes.to_vec())
        );
    }
    #[test]
    fn whitespace_identity_retention_and_payload_limit_follow_source_order() {
        let instance = CfxGlobalData::get_retained_instance(None);
        let retained = CfxGlobalData::get_retained_instance(None);
        assert!(Rc::ptr_eq(&instance, &retained));
        assert!(!instance.release());
        retained.set_global_variable_number(b" \t\r\nkey\x0b\x0c ", 3.0);
        let identity = retained.get_global_variable(b"key").unwrap();
        assert!(retained.get_global_variable(b" key ").is_none());
        assert!(retained.set_global_variable_persistent(b"\tkey\n", true));
        retained.set_global_variable_object(
            b"key",
            vec![CfxKeyValue {
                key: b"child".to_vec(),
                value: CfxValue::Boolean(true),
            }],
        );
        assert!(Rc::ptr_eq(&identity, &retained.get_at(0).unwrap()));
        assert!(identity.borrow().persistent);
        retained.set_global_variable_null(b" \t\n");
        assert!(!retained.delete_global_variable(b" "));
        assert!(!retained.set_global_variable_persistent(b"missing", true));
        assert_eq!(retained.get_size(), 1);
        // Objects and nonpersistent entries are skipped; the first eligible
        // oversized entry terminates saving, excluding later small records.
        retained.set_global_variable_null(b"before");
        retained.set_global_variable_string(b"too-large", &vec![b'x'; MAX_GLOBAL_DATA_BYTES]);
        retained.set_global_variable_null(b"after");
        retained.set_global_variable_string(b"volatile", b"not saved");
        for name in [b"before".as_slice(), b"too-large", b"after"] {
            retained.set_global_variable_persistent(name, true);
        }
        let plain = encrypted(retained.make_persistent_buffer());
        assert_eq!(u32::from_le_bytes(plain[4..8].try_into().unwrap()), 1);
        assert_eq!(u32::from_le_bytes(plain[8..12].try_into().unwrap()), 12);
        assert_eq!(
            parse_persistent_buffer(&plain).unwrap(),
            [CfxKeyValue {
                key: b"before".to_vec(),
                value: CfxValue::Null
            }]
        );
        drop(identity);
        assert!(retained.release());
    }
    #[test]
    fn delegate_callbacks_can_reenter_without_store_borrows_and_expire_safely() {
        #[derive(Default)]
        struct ReentrantDelegate {
            instance: RefCell<Weak<CfxGlobalData>>,
            done: Cell<usize>,
            stores: Cell<usize>,
        }
        impl GlobalDataDelegate for ReentrantDelegate {
            fn load_buffer(&self) -> Option<Vec<u8>> {
                let instance = CfxGlobalData::get_retained_instance(None);
                instance.set_global_variable_number(b"during-load", 1.0);
                *self.instance.borrow_mut() = Rc::downgrade(&instance);
                Some(encrypted(frame(2, 0, &[])))
            }
            fn buffer_done(&self) {
                self.done.set(self.done.get() + 1);
                self.instance
                    .borrow()
                    .upgrade()
                    .unwrap()
                    .set_global_variable_null(b"during-done");
            }
            fn store_buffer(&self, _: &[u8]) -> bool {
                self.stores.set(self.stores.get() + 1);
                if let Some(instance) = self.instance.borrow().upgrade() {
                    assert_eq!(instance.get_size(), 2);
                    instance.set_global_variable_boolean(b"during-done", true);
                }
                false
            }
        }
        let delegate = Rc::new(ReentrantDelegate::default());
        let interface: Rc<dyn GlobalDataDelegate> = delegate.clone();
        let instance = CfxGlobalData::get_retained_instance(Some(interface.clone()));
        assert_eq!(delegate.done.get(), 1);
        assert!(Rc::ptr_eq(
            &delegate.instance.borrow().upgrade().unwrap(),
            &instance
        ));
        assert!(!instance.commit());
        assert_eq!(delegate.stores.get(), 1);
        assert_element(&instance, 1, b"during-done", CfxValue::Boolean(true));
        drop(interface);
        drop(delegate);
        assert!(!instance.commit());
        assert!(instance.release());
    }
    #[test]
    fn blank_string_keys_and_persistence_requests_do_not_change_existing_entries() {
        let store = CfxGlobalData::new(None);
        store.set_global_variable_string(b"stable", b"retained");
        let original = store.get_global_variable(b"stable").unwrap();
        for blank in [b"".as_slice(), b" ", b"\t\r\n\x0b\x0c"] {
            store.set_global_variable_string(blank, b"ignored");
            assert!(!store.set_global_variable_persistent(blank, true));
            assert!(!store.set_global_variable_persistent(blank, false));
            assert_eq!(store.get_size(), 1);
            assert!(Rc::ptr_eq(&original, &store.get_at(0).unwrap()));
            assert!(!original.borrow().persistent);
        }
        assert_element(&store, 0, b"stable", CfxValue::String(b"retained".to_vec()));
    }

    #[test]
    fn delegate_without_a_load_lease_never_receives_buffer_done() {
        #[derive(Default)]
        struct AbsentBuffer {
            loads: Cell<usize>,
            stores: Cell<usize>,
            done: Cell<usize>,
        }
        impl GlobalDataDelegate for AbsentBuffer {
            fn load_buffer(&self) -> Option<Vec<u8>> {
                self.loads.set(self.loads.get() + 1);
                None
            }
            fn buffer_done(&self) {
                self.done.set(self.done.get() + 1);
            }
            fn store_buffer(&self, buffer: &[u8]) -> bool {
                self.stores.set(self.stores.get() + 1);
                assert_eq!(
                    parse_persistent_buffer(&encrypted(buffer.to_vec())).unwrap(),
                    []
                );
                false
            }
        }
        let owner = Rc::new(AbsentBuffer::default());
        let delegate: Rc<dyn GlobalDataDelegate> = owner.clone();
        let store = CfxGlobalData::new(Some(&delegate));
        assert!(!store.load_global_persistent_variables());
        assert_eq!(owner.loads.get(), 1);
        assert_eq!(owner.done.get(), 0);
        assert_eq!(store.get_size(), 0);
        assert!(!store.commit());
        assert_eq!(owner.stores.get(), 1);
        drop(store);
        assert_eq!(owner.stores.get(), 2);
        assert_eq!(owner.done.get(), 0);
    }
}
