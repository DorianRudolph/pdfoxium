// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Direct port of `core/fpdfapi/edit/cpdf_creator.{h,cpp}`. The retained document
//! handle is borrowed only between output operations, so writer callbacks may
//! query the document. The buffered archive flushes again on destruction.

use super::cpdf_fontsubsetter::CpdfFontSubsetter;
use crate::fpdfapi::parser::cpdf_array::CpdfArray;
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_encryptor::CpdfEncryptor;
use crate::fpdfapi::parser::cpdf_indirect_object_holder::CpdfIndirectObjectHolder;
use crate::fpdfapi::parser::cpdf_object::{ObjectRef, INVALID_OBJ_NUM};
use crate::fpdfapi::parser::cpdf_security_handler::CpdfSecurityHandler;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfapi::parser::fpdf_parser_utility::pdf_name_encode;
use crate::fpdfapi::parser::object_tree_traversal_util::ObjectTreeTraverser;
use crate::fxcrt::bytestring::eq_cstr;
use crate::fxcrt::fx_random;
use crate::fxcrt::fx_stream::{ArchiveStream, SeekableReadStream, WriteStream};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

const ARCHIVE_BUFFER_SIZE: usize = 32768;
struct FileBufferArchive<'sink> {
    offset: i64,
    buffer: Box<[u8; ARCHIVE_BUFFER_SIZE]>,
    used: usize,
    backing_file: &'sink mut dyn WriteStream,
}
impl<'sink> FileBufferArchive<'sink> {
    fn new(backing_file: &'sink mut dyn WriteStream) -> Self {
        Self {
            offset: 0,
            buffer: Box::new([0; ARCHIVE_BUFFER_SIZE]),
            used: 0,
            backing_file,
        }
    }
    fn flush(&mut self) -> bool {
        let used = self.used;
        self.used = 0;
        used == 0 || self.backing_file.write_block(&self.buffer[..used])
    }
}
impl WriteStream for FileBufferArchive<'_> {
    fn write_block(&mut self, buffer: &[u8]) -> bool {
        if buffer.is_empty() {
            return true;
        }
        let mut source = buffer;
        while !source.is_empty() {
            let copy_size = (ARCHIVE_BUFFER_SIZE - self.used).min(source.len());
            self.buffer[self.used..self.used + copy_size].copy_from_slice(&source[..copy_size]);
            self.used += copy_size;
            source = &source[copy_size..];
            if self.used == ARCHIVE_BUFFER_SIZE && !self.flush() {
                return false;
            }
        }
        let Some(offset) = i64::try_from(buffer.len())
            .ok()
            .and_then(|length| self.offset.checked_add(length))
        else {
            return false;
        };
        self.offset = offset;
        true
    }
}
impl ArchiveStream for FileBufferArchive<'_> {
    fn current_offset(&self) -> i64 {
        self.offset
    }
}
impl Drop for FileBufferArchive<'_> {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}
fn output_index(archive: &mut dyn ArchiveStream, offset: i64) -> bool {
    archive.write_byte((offset >> 24) as u8)
        && archive.write_byte((offset >> 16) as u8)
        && archive.write_byte((offset >> 8) as u8)
        && archive.write_byte(offset as u8)
        && archive.write_byte(0)
}
fn array<R>(object: &ObjectRef, f: impl FnOnce(&CpdfArray) -> R) -> R {
    let object = object.borrow();
    f(object.as_any().downcast_ref().unwrap())
}
fn array_mut<R>(object: &ObjectRef, f: impl FnOnce(&mut CpdfArray) -> R) -> R {
    let mut object = object.borrow_mut();
    f(object.as_any_mut().downcast_mut().unwrap())
}
fn dict<R>(object: &ObjectRef, f: impl FnOnce(&CpdfDictionary) -> R) -> R {
    let object = object.borrow();
    f(object.as_any().downcast_ref().unwrap())
}
fn random_file_id() -> CpdfString {
    let mut words = [0u32; 4];
    fx_random::fill(&mut words);
    let mut bytes = [0u8; 16];
    for (chunk, word) in bytes.chunks_exact_mut(4).zip(words) {
        chunk.copy_from_slice(&word.to_ne_bytes());
    }
    CpdfString::from_hex_bytes(None, &bytes)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
#[repr(i32)]
enum Stage {
    Invalid = -1,
    Init0 = 0,
    WriteHeader10 = 10,
    WriteIncremental15 = 15,
    InitWriteObjs20 = 20,
    WriteOldObjs21 = 21,
    InitWriteNewObjs25 = 25,
    WriteNewObjs26 = 26,
    WriteEncryptDict27 = 27,
    InitWriteXRefs80 = 80,
    WriteXrefsNotIncremental81 = 81,
    WriteXrefsIncremental82 = 82,
    WriteTrailerAndFinish90 = 90,
    Complete100 = 100,
}

pub struct CpdfCreator<'sink> {
    // Flush before releasing the retained document: the final writer callback
    // may query it even when this creator owns its last retained handle.
    archive: FileBufferArchive<'sink>,
    document: Rc<RefCell<CpdfDocument>>,
    has_parser: bool,
    encrypt_dict: Option<ObjectRef>,
    new_encrypt_dict: Option<ObjectRef>,
    security_handler: Option<Rc<CpdfSecurityHandler>>,
    last_obj_num: u32,
    saved_offset: i64,
    stage: Stage,
    cur_obj_num: u32,
    xref_start: i64,
    object_offsets: BTreeMap<u32, i64>,
    new_obj_num_array: Vec<u32>,
    id_array: Option<ObjectRef>,
    file_version: i32,
    security_changed: bool,
    is_incremental: bool,
    is_original: bool,
    subset_new_fonts: bool,
}
impl<'sink> CpdfCreator<'sink> {
    pub const NONE: u32 = 0;
    pub const INCREMENTAL: u32 = 1;
    pub const NO_ORIGINAL: u32 = 2;
    pub const REMOVE_SECURITY_DEPRECATED: u32 = 3;
    pub const REMOVE_SECURITY: u32 = 4;
    pub const SUBSET_NEW_FONTS: u32 = 8;
    pub fn new(document: Rc<RefCell<CpdfDocument>>, archive: &'sink mut dyn WriteStream) -> Self {
        let (has_parser, encrypt_dict, security_handler, last_obj_num) = {
            let document = document.borrow();
            let parser = document.parser();
            let has_parser = parser.document_size().is_some();
            let last_obj_num = document.holder().borrow().last_obj_num();
            (
                has_parser,
                if has_parser {
                    parser.encrypt_dict()
                } else {
                    None
                },
                if has_parser {
                    parser.security_handler_ref()
                } else {
                    None
                },
                last_obj_num,
            )
        };
        Self {
            document,
            has_parser,
            encrypt_dict,
            new_encrypt_dict: None,
            security_handler,
            last_obj_num,
            archive: FileBufferArchive::new(archive),
            saved_offset: 0,
            stage: Stage::Invalid,
            cur_obj_num: 0,
            xref_start: 0,
            object_offsets: BTreeMap::new(),
            new_obj_num_array: Vec::new(),
            id_array: None,
            file_version: 0,
            security_changed: false,
            is_incremental: false,
            is_original: false,
            subset_new_fonts: false,
        }
    }
    pub fn create(&mut self, mut flags: u32, file_version: i32) -> bool {
        let valid =
            Self::INCREMENTAL | Self::NO_ORIGINAL | Self::REMOVE_SECURITY | Self::SUBSET_NEW_FONTS;
        if flags & !valid != 0 {
            flags = Self::NONE;
        }
        if flags == Self::REMOVE_SECURITY_DEPRECATED || flags & Self::REMOVE_SECURITY != 0 {
            self.remove_security();
        }
        let conflicting = Self::INCREMENTAL | Self::NO_ORIGINAL;
        if flags & conflicting == conflicting {
            flags &= !conflicting;
        }
        self.is_incremental = flags & Self::INCREMENTAL != 0;
        self.is_original = flags & Self::NO_ORIGINAL == 0;
        self.subset_new_fonts = flags & Self::SUBSET_NEW_FONTS != 0;
        if (10..=17).contains(&file_version) {
            self.file_version = file_version;
        }
        self.stage = Stage::Init0;
        self.last_obj_num = self.document.borrow().holder().borrow().last_obj_num();
        self.object_offsets.clear();
        self.new_obj_num_array.clear();
        self.init_id();
        self.continue_create()
    }
    fn write_indirect_obj(&mut self, objnum: u32, object: &ObjectRef) -> bool {
        if !self.archive.write_dword(objnum) || !self.archive.write_string(b" 0 obj\r\n") {
            return false;
        }
        let encryptor = if self
            .encrypt_dict
            .as_ref()
            .is_some_and(|dict| Rc::ptr_eq(dict, object))
        {
            None
        } else {
            self.security_handler
                .as_ref()
                .and_then(|security| security.crypto_handler())
                .map(|crypto| CpdfEncryptor::new(crypto, objnum as i32))
        };
        if !object
            .borrow()
            .write_to(&mut self.archive, encryptor.as_ref())
        {
            return false;
        }
        self.archive.write_string(b"\r\nendobj\r\n")
    }
    fn write_old_indirect_object(&mut self, objnum: u32) -> bool {
        if self.document.borrow().parser().is_object_free(objnum) {
            return true;
        }
        self.object_offsets
            .insert(objnum, self.archive.current_offset());
        let objects = self.document.borrow().holder();
        let existed = objects.borrow().get_indirect_object(objnum).is_some();
        let object = CpdfIndirectObjectHolder::get_or_parse_indirect_object(&objects, objnum);
        let Some(object) = object else {
            self.object_offsets.remove(&objnum);
            return true;
        };
        let number = object.borrow().obj_num();
        if !self.write_indirect_obj(number, &object) {
            return false;
        }
        if !existed {
            objects.borrow_mut().delete_indirect_object(objnum);
        }
        true
    }
    fn write_old_objs(&mut self) -> bool {
        let last = self.document.borrow().parser().last_obj_num();
        if !self.document.borrow().parser().is_valid_object_number(last) || self.cur_obj_num > last
        {
            return true;
        }
        let traverser = ObjectTreeTraverser::new(&self.document.borrow());
        let referenced = traverser.get_objects_with_references();
        let mut last_written = 0;
        for objnum in self.cur_obj_num..=last {
            if !referenced.contains(&objnum) {
                continue;
            }
            if !self.write_old_indirect_object(objnum) {
                return false;
            }
            last_written = objnum;
        }
        if self.new_obj_num_array.is_empty() {
            self.last_obj_num = last_written;
        }
        true
    }
    fn write_new_objs(&mut self) -> bool {
        let overrides = if self.subset_new_fonts {
            CpdfFontSubsetter::new(Rc::clone(&self.document))
                .generate_object_overrides(&self.new_obj_num_array)
        } else {
            BTreeMap::new()
        };
        for i in self.cur_obj_num as usize..self.new_obj_num_array.len() {
            let number = self.new_obj_num_array[i];
            let object = self
                .document
                .borrow()
                .holder()
                .borrow()
                .get_indirect_object(number);
            let Some(object) = object else { continue };
            self.object_offsets
                .insert(number, self.archive.current_offset());
            let to_write = overrides.get(&number).unwrap_or(&object);
            if !self.write_indirect_obj(number, to_write) {
                return false;
            }
        }
        true
    }
    fn init_new_obj_num_offsets(&mut self) {
        let document = self.document.borrow();
        let objects = document.holder();
        let objects = objects.borrow();
        for (&number, object) in objects.iter() {
            let object = object
                .as_ref()
                .expect("indirect objects cannot be pending during save enumeration");
            if object.borrow().obj_num() == INVALID_OBJ_NUM {
                continue;
            }
            if !self.is_incremental
                && self.has_parser
                && document.parser().is_valid_object_number(number)
                && !document.parser().is_object_free(number)
            {
                continue;
            }
            let index = self.new_obj_num_array.partition_point(|n| *n < number);
            self.new_obj_num_array.insert(index, number);
        }
    }
    fn write_doc_stage1(&mut self) -> Stage {
        debug_assert!(self.stage > Stage::Invalid || self.stage < Stage::InitWriteObjs20);
        if self.stage == Stage::Init0 {
            if !self.has_parser || (self.security_changed && self.is_original) {
                self.is_incremental = false;
            }
            self.stage = Stage::WriteHeader10;
        }
        if self.stage == Stage::WriteHeader10 {
            if !self.is_incremental {
                if !self.archive.write_string(b"%PDF-1.") {
                    return Stage::Invalid;
                }
                let version = if self.file_version != 0 {
                    self.file_version
                } else if self.has_parser {
                    self.document.borrow().parser().file_version()
                } else {
                    7
                };
                if !self.archive.write_dword((version % 10) as u32)
                    || !self.archive.write_string(b"\r\n%\xA1\xB3\xC5\xD7\r\n")
                {
                    return Stage::Invalid;
                }
                self.stage = Stage::InitWriteObjs20;
            } else {
                self.saved_offset = self
                    .document
                    .borrow()
                    .parser()
                    .document_size()
                    .expect("loaded parser syntax");
                self.stage = Stage::WriteIncremental15;
            }
        }
        if self.stage == Stage::WriteIncremental15 {
            if self.is_original && self.saved_offset > 0 && !self.write_original_to_archive() {
                return Stage::Invalid;
            }
            let last_xref = self.document.borrow().parser().last_xref_offset();
            if self.is_original && last_xref == 0 {
                let last = self.document.borrow().parser().last_obj_num();
                for number in 0..=last {
                    let document = self.document.borrow();
                    let parser = document.parser();
                    if parser.is_object_free(number) {
                        continue;
                    }
                    self.object_offsets
                        .insert(number, parser.object_position_or_zero(number));
                }
            }
            self.stage = Stage::InitWriteObjs20;
        }
        self.init_new_obj_num_offsets();
        self.stage
    }
    fn write_original_to_archive(&mut self) -> bool {
        self.document.borrow_mut().parser_mut().set_save_position(0);
        let (source, header_offset) = self
            .document
            .borrow()
            .parser()
            .archive_source()
            .expect("loaded parser source");
        let mut remaining = self.saved_offset;
        let mut buffer = [0u8; 4096];
        while remaining != 0 {
            let block = remaining.min(buffer.len() as i64) as usize;
            let position = self
                .document
                .borrow()
                .parser()
                .save_position()
                .expect("loaded parser syntax");
            let Some(absolute) = position.checked_add(header_offset) else {
                return false;
            };
            // Both source reads and output writes may invoke external callbacks.
            // Match ReadBlock's order without holding the document borrow.
            if !source.read_block_at_offset(&mut buffer[..block], absolute) {
                return false;
            }
            self.document
                .borrow_mut()
                .parser_mut()
                .advance_save_position(block);
            if !self.archive.write_block(&buffer[..block]) {
                return false;
            }
            remaining -= block as i64;
        }
        true
    }
    fn write_doc_stage2(&mut self) -> Stage {
        debug_assert!(self.stage >= Stage::InitWriteObjs20 || self.stage < Stage::InitWriteXRefs80);
        if self.stage == Stage::InitWriteObjs20 {
            if !self.is_incremental && self.has_parser {
                self.cur_obj_num = 0;
                self.stage = Stage::WriteOldObjs21;
            } else {
                self.stage = Stage::InitWriteNewObjs25;
            }
        }
        if self.stage == Stage::WriteOldObjs21 {
            if !self.write_old_objs() {
                return Stage::Invalid;
            }
            self.stage = Stage::InitWriteNewObjs25;
        }
        if self.stage == Stage::InitWriteNewObjs25 {
            self.cur_obj_num = 0;
            self.stage = Stage::WriteNewObjs26;
        }
        if self.stage == Stage::WriteNewObjs26 {
            if !self.write_new_objs() {
                return Stage::Invalid;
            }
            self.stage = Stage::WriteEncryptDict27;
        }
        if self.stage == Stage::WriteEncryptDict27 {
            if let Some(encrypt) = self.encrypt_dict.clone().filter(|d| d.borrow().is_inline()) {
                self.last_obj_num = self.last_obj_num.wrapping_add(1);
                let offset = self.archive.current_offset();
                if !self.write_indirect_obj(self.last_obj_num, &encrypt) {
                    return Stage::Invalid;
                }
                self.object_offsets.insert(self.last_obj_num, offset);
                if self.is_incremental {
                    self.new_obj_num_array.push(self.last_obj_num);
                }
            }
            self.stage = Stage::InitWriteXRefs80;
        }
        self.stage
    }
    fn write_doc_stage3(&mut self) -> Stage {
        debug_assert!(
            self.stage >= Stage::InitWriteXRefs80 || self.stage < Stage::WriteTrailerAndFinish90
        );
        let last = self.last_obj_num;
        if self.stage == Stage::InitWriteXRefs80 {
            self.xref_start = self.archive.current_offset();
            let xref_stream = self.is_incremental && self.document.borrow().parser().xref_stream();
            if !self.is_incremental || !xref_stream {
                let last_xref = if self.has_parser {
                    self.document.borrow().parser().last_xref_offset()
                } else {
                    0
                };
                if !self.is_incremental || last_xref == 0 {
                    let header = if self.object_offsets.contains_key(&1) {
                        b"xref\r\n".as_slice()
                    } else {
                        b"xref\r\n0 1\r\n0000000000 65535 f\r\n"
                    };
                    if !self.archive.write_string(header) {
                        return Stage::Invalid;
                    }
                    self.cur_obj_num = 1;
                    self.stage = Stage::WriteXrefsNotIncremental81;
                } else {
                    if !self.archive.write_string(b"xref\r\n") {
                        return Stage::Invalid;
                    }
                    self.cur_obj_num = 0;
                    self.stage = Stage::WriteXrefsIncremental82;
                }
            } else {
                self.stage = Stage::WriteTrailerAndFinish90;
            }
        }
        if self.stage == Stage::WriteXrefsNotIncremental81 {
            let mut i = self.cur_obj_num;
            while i <= last {
                while i <= last && !self.object_offsets.contains_key(&i) {
                    i = i.wrapping_add(1);
                }
                if i > last {
                    break;
                }
                let mut j = i;
                while j <= last && self.object_offsets.contains_key(&j) {
                    j = j.wrapping_add(1);
                }
                let header = if i == 1 {
                    format!("0 {}\r\n0000000000 65535 f\r\n", j as i32)
                } else {
                    format!("{} {}\r\n", i as i32, j.wrapping_sub(i) as i32)
                };
                if !self.archive.write_string(header.as_bytes()) {
                    return Stage::Invalid;
                }
                while i < j {
                    let offset = self.object_offsets[&i];
                    i = i.wrapping_add(1);
                    if !self.write_xref_offset(offset) {
                        return Stage::Invalid;
                    }
                }
                if i > last {
                    break;
                }
            }
            self.stage = Stage::WriteTrailerAndFinish90;
        }
        if self.stage == Stage::WriteXrefsIncremental82 {
            let count = u32::try_from(self.new_obj_num_array.len()).expect("object count");
            let mut i = self.cur_obj_num;
            while i < count {
                let mut j = i;
                let mut number = self.new_obj_num_array[i as usize];
                while j < count {
                    j += 1;
                    if j == count {
                        break;
                    }
                    let current = self.new_obj_num_array[j as usize];
                    if current.wrapping_sub(number) > 1 {
                        break;
                    }
                    number = current;
                }
                number = self.new_obj_num_array[i as usize];
                let header = if number == 1 {
                    format!(
                        "0 {}\r\n0000000000 65535 f\r\n",
                        j.wrapping_sub(i).wrapping_add(1) as i32
                    )
                } else {
                    format!("{} {}\r\n", number as i32, j.wrapping_sub(i) as i32)
                };
                if !self.archive.write_string(header.as_bytes()) {
                    return Stage::Invalid;
                }
                while i < j {
                    number = self.new_obj_num_array[i as usize];
                    i += 1;
                    let offset = *self.object_offsets.entry(number).or_default();
                    if !self.write_xref_offset(offset) {
                        return Stage::Invalid;
                    }
                }
            }
            self.stage = Stage::WriteTrailerAndFinish90;
        }
        self.stage
    }
    fn write_xref_offset(&mut self, offset: i64) -> bool {
        // Native ByteString::Format("%010d", FX_FILESIZE) consumes the low signed
        // int word on the selected two's-complement desktop ABI.
        self.archive
            .write_string(format!("{:010} 00000 n\r\n", offset as i32).as_bytes())
    }
    fn write_doc_stage4(&mut self) -> Stage {
        debug_assert!(self.stage >= Stage::WriteTrailerAndFinish90);
        let xref_stream = self.is_incremental && self.document.borrow().parser().xref_stream();
        if !xref_stream {
            if !self.archive.write_string(b"trailer\r\n<<") {
                return Stage::Invalid;
            }
        } else {
            let number = self
                .document
                .borrow()
                .holder()
                .borrow()
                .last_obj_num()
                .wrapping_add(1);
            if !self.archive.write_dword(number) || !self.archive.write_string(b" 0 obj <<") {
                return Stage::Invalid;
            }
        }
        if self.has_parser {
            let trailer = self
                .document
                .borrow()
                .parser()
                .combined_trailer()
                .expect("loaded parser combined trailer");
            let skipped: [&[u8]; 11] = [
                b"Encrypt",
                b"Size",
                b"Filter",
                b"Index",
                b"Length",
                b"Prev",
                b"W",
                b"XRefStm",
                b"ID",
                b"DecodeParms",
                b"Type",
            ];
            let written = {
                let borrowed = trailer.borrow();
                let trailer = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                trailer.with_locked_entries(|entries| {
                    for (key, value) in entries {
                        if skipped.iter().any(|skipped| eq_cstr(key, skipped)) {
                            continue;
                        }
                        if !self.archive.write_string(b"/")
                            || !self.archive.write_string(&pdf_name_encode(key))
                            || !value.borrow().write_to(&mut self.archive, None)
                        {
                            return false;
                        }
                    }
                    true
                })
            };
            if !written {
                return Stage::Invalid;
            }
        } else {
            if !self.archive.write_string(b"\r\n/Root ") {
                return Stage::Invalid;
            }
            let root = self.document.borrow().root().expect("new document root");
            let number = root.borrow().obj_num();
            if !self.archive.write_dword(number) || !self.archive.write_string(b" 0 R\r\n") {
                return Stage::Invalid;
            }
            let has_info = self.document.borrow_mut().info().is_some();
            if has_info {
                if !self.archive.write_string(b"/Info ") {
                    return Stage::Invalid;
                }
                let info = self
                    .document
                    .borrow_mut()
                    .info()
                    .expect("new document info");
                let number = info.borrow().obj_num();
                if !self.archive.write_dword(number) || !self.archive.write_string(b" 0 R\r\n") {
                    return Stage::Invalid;
                }
            }
        }
        if let Some(encrypt) = self.encrypt_dict.as_ref() {
            if !self.archive.write_string(b"/Encrypt") {
                return Stage::Invalid;
            }
            let mut number = encrypt.borrow().obj_num();
            if number == 0 {
                number = self
                    .document
                    .borrow()
                    .holder()
                    .borrow()
                    .last_obj_num()
                    .wrapping_add(1);
            }
            if !self.archive.write_string(b" ")
                || !self.archive.write_dword(number)
                || !self.archive.write_string(b" 0 R ")
            {
                return Stage::Invalid;
            }
        }
        if !self.archive.write_string(b"/Size ")
            || !self
                .archive
                .write_dword(
                    self.last_obj_num
                        .wrapping_add(if xref_stream { 2 } else { 1 }),
                )
        {
            return Stage::Invalid;
        }
        if self.is_incremental {
            let previous = self.document.borrow().parser().last_xref_offset();
            if previous != 0
                && (!self.archive.write_string(b"/Prev ") || !self.archive.write_filesize(previous))
            {
                return Stage::Invalid;
            }
        }
        if let Some(id) = self.id_array.as_ref() {
            if !self.archive.write_string(b"/ID") || !id.borrow().write_to(&mut self.archive, None)
            {
                return Stage::Invalid;
            }
        }
        if !xref_stream {
            if !self.archive.write_string(b">>") {
                return Stage::Invalid;
            }
        } else {
            if !self.archive.write_string(b"/W[0 4 1]/Index[") {
                return Stage::Invalid;
            }
            let last_xref = self.document.borrow().parser().last_xref_offset();
            if self.is_incremental && self.has_parser && last_xref == 0 {
                for i in 0..self.last_obj_num {
                    if !self.object_offsets.contains_key(&i) {
                        continue;
                    }
                    if !self.archive.write_dword(i) || !self.archive.write_string(b" 1 ") {
                        return Stage::Invalid;
                    }
                }
                if !self.archive.write_string(b"]/Length ")
                    || !self.archive.write_dword(self.last_obj_num.wrapping_mul(5))
                    || !self.archive.write_string(b">>stream\r\n")
                {
                    return Stage::Invalid;
                }
                for i in 0..self.last_obj_num {
                    let Some(&offset) = self.object_offsets.get(&i) else {
                        continue;
                    };
                    if !output_index(&mut self.archive, offset) {
                        return Stage::Invalid;
                    }
                }
            } else {
                let count = i32::try_from(self.new_obj_num_array.len()).expect("object count");
                for &number in &self.new_obj_num_array {
                    if !self.archive.write_dword(number) || !self.archive.write_string(b" 1 ") {
                        return Stage::Invalid;
                    }
                }
                if !self.archive.write_string(b"]/Length ")
                    || !self.archive.write_dword(count.wrapping_mul(5) as u32)
                    || !self.archive.write_string(b">>stream\r\n")
                {
                    return Stage::Invalid;
                }
                for &number in &self.new_obj_num_array {
                    let offset = *self.object_offsets.entry(number).or_default();
                    if !output_index(&mut self.archive, offset) {
                        return Stage::Invalid;
                    }
                }
            }
            if !self.archive.write_string(b"\r\nendstream") {
                return Stage::Invalid;
            }
        }
        if !self.archive.write_string(b"\r\nstartxref\r\n")
            || !self.archive.write_filesize(self.xref_start)
            || !self.archive.write_string(b"\r\n%%EOF\r\n")
        {
            return Stage::Invalid;
        }
        self.stage = Stage::Complete100;
        self.stage
    }
    fn init_id(&mut self) {
        debug_assert!(self.id_array.is_none());
        let id = self.document.borrow().holder().borrow().new_array();
        self.id_array = Some(Rc::clone(&id));
        let old = if self.has_parser {
            let trailer = self.document.borrow().parser().trailer();
            trailer.and_then(|trailer| dict(&trailer, |d| d.get_array_for(b"ID")))
        } else {
            None
        };
        let first = old.as_ref().and_then(|a| array(a, |a| a.get_object_at(0)));
        if let Some(first) = first {
            let cloned = first.borrow().clone_object();
            array_mut(&id, |a| a.append(cloned));
        } else {
            array_mut(&id, |a| a.append_new(random_file_id()));
        }
        if let Some(old) = old {
            let second = array(&old, |a| a.get_object_at(1));
            if self.is_incremental && self.encrypt_dict.is_some() {
                if let Some(second) = second {
                    let cloned = second.borrow().clone_object();
                    array_mut(&id, |a| a.append(cloned));
                    return;
                }
            }
            array_mut(&id, |a| a.append_new(random_file_id()));
            return;
        }
        let first = array(&id, |a| a.get_object_at(0)).unwrap();
        let cloned = first.borrow().clone_object();
        array_mut(&id, |a| a.append(cloned));
        if let Some(encrypt) = self.encrypt_dict.clone() {
            debug_assert!(self.has_parser);
            let revision = dict(&encrypt, |d| d.get_integer_for(b"R", 0));
            if (revision == 2 || revision == 3)
                && eq_cstr(
                    &dict(&encrypt, |d| d.get_byte_string_for(b"Filter", b"")),
                    b"Standard",
                )
            {
                let cloned = encrypt.borrow().clone_object();
                self.new_encrypt_dict = Some(Rc::clone(&cloned));
                self.encrypt_dict = Some(Rc::clone(&cloned));
                let mut security = CpdfSecurityHandler::new();
                let password = self
                    .document
                    .borrow()
                    .parser()
                    .encoded_password()
                    .expect("encrypted parser password");
                security.on_create(cloned, Some(&id), &password);
                self.security_handler = Some(Rc::new(security));
                self.security_changed = true;
            }
        }
    }
    fn continue_create(&mut self) -> bool {
        if self.stage < Stage::Init0 {
            return false;
        }
        let mut result = Stage::Init0;
        while self.stage < Stage::Complete100 {
            result = if self.stage < Stage::InitWriteObjs20 {
                self.write_doc_stage1()
            } else if self.stage < Stage::InitWriteXRefs80 {
                self.write_doc_stage2()
            } else if self.stage < Stage::WriteTrailerAndFinish90 {
                self.write_doc_stage3()
            } else {
                self.write_doc_stage4()
            };
            if result < self.stage {
                break;
            }
        }
        if result <= Stage::Init0 || self.stage == Stage::Complete100 {
            self.stage = Stage::Invalid;
            return result > Stage::Init0;
        }
        self.stage > Stage::Invalid
    }
    fn remove_security(&mut self) {
        self.security_handler = None;
        self.security_changed = true;
        self.encrypt_dict = None;
        self.new_encrypt_dict = None;
    }
}

#[cfg(test)]
#[path = "cpdf_creator_embeddertest.rs"]
mod embedder_tests;

#[cfg(test)]
#[path = "cpdf_creator_roundtrip_tests.rs"]
mod roundtrip_tests;

#[cfg(test)]
#[path = "cpdf_creator_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "cpdf_creator_security_tests.rs"]
mod security_tests;
