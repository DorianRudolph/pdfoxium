// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! `core/fpdfapi/parser/cpdf_parser.{h,cpp}`.

use super::cpdf_array::CpdfArray;
use super::cpdf_cross_ref_table::{CpdfCrossRefTable, ObjectInfo, ObjectType, MAX_OBJECT_NUMBER};
use super::cpdf_dictionary::CpdfDictionary;
use super::cpdf_indirect_object_holder::{CpdfIndirectObjectHolder, HolderRef};
use super::cpdf_linearized_header::CpdfLinearizedHeader;
use super::cpdf_object::{get_dict_object, ObjectRef, PdfObject, INVALID_OBJ_NUM};
use super::cpdf_object_stream::CpdfObjectStream;
use super::cpdf_read_validator::CpdfReadValidator;
use super::cpdf_reference::CpdfReference;
use super::cpdf_security_handler::CpdfSecurityHandler;
use super::cpdf_stream::{CpdfStream, FILE_BUF_SIZE};
use super::cpdf_stream_acc::CpdfStreamAcc;
use super::cpdf_syntax_parser::{CpdfSyntaxParser, ParseType};
use super::fpdf_parser_utility::get_header_offset;
use crate::fxcrt::fx_stream::WriteStream;
use crate::fxcrt::fx_stream::{SeekableReadStream, StreamWithSize};
use crate::fxcrt::fx_string::string_to_int;
use crate::fxcrt::fx_system::{atoi64, atoui};
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};
use std::rc::{Rc, Weak};

const MAX_XREF_SIZE: usize = MAX_OBJECT_NUMBER as usize + 1;
const ENTRY_SIZE: usize = 20;

#[derive(Clone, Copy)]
pub struct CrossRefObjData {
    pub obj_num: u32,
    pub info: ObjectInfo,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CrossRefStreamIndexEntry {
    pub start_obj_num: u32,
    pub obj_count: u32,
}

pub fn get_cross_ref_stream_indices(
    array: Option<&CpdfArray>,
    size: u32,
) -> Vec<CrossRefStreamIndexEntry> {
    let mut indices = Vec::new();
    if let Some(array) = array {
        for i in 0..array.size() / 2 {
            let (Some(start), Some(count)) =
                (array.get_number_at(i * 2), array.get_number_at(i * 2 + 1))
            else {
                continue;
            };
            let start = start.borrow().get_integer();
            let count = count.borrow().get_integer();
            if start < 0 || count <= 0 {
                continue;
            }
            indices.push(CrossRefStreamIndexEntry {
                start_obj_num: start as u32,
                obj_count: count as u32,
            });
        }
    }
    if indices.is_empty() {
        indices.push(CrossRefStreamIndexEntry {
            start_obj_num: 0,
            obj_count: size,
        });
    }
    indices
}

pub fn get_field_widths(array: Option<&CpdfArray>) -> Vec<u32> {
    array.map_or_else(Vec::new, |array| {
        array.with_locked_objects(|objects| {
            objects
                .iter()
                .map(|object| object.borrow().get_integer() as u32)
                .collect()
        })
    })
}

fn get_var_int(input: &[u8]) -> u32 {
    input.iter().fold(0u32, |value, &byte| {
        value.wrapping_mul(256).wrapping_add(u32::from(byte))
    })
}

pub fn process_cross_ref_stream_entry(
    table: &mut CpdfCrossRefTable,
    entry: &[u8],
    widths: &[u32],
    obj_num: u32,
) {
    if widths.len() < 3 {
        return;
    }
    let Some(first_end) = usize::try_from(widths[0]).ok() else {
        return;
    };
    let Some(second_end) = first_end.checked_add(widths[1] as usize) else {
        return;
    };
    let Some(third_end) = second_end.checked_add(widths[2] as usize) else {
        return;
    };
    let Some(first) = entry.get(..first_end) else {
        return;
    };
    let Some(second) = entry.get(first_end..second_end) else {
        return;
    };
    let Some(third) = entry.get(second_end..third_end) else {
        return;
    };
    let entry_type = if widths[0] == 0 {
        1
    } else {
        get_var_int(first)
    };
    match entry_type {
        0 => {
            let gen_num = get_var_int(third);
            if let Ok(gen_num) = u16::try_from(gen_num) {
                table.set_free(obj_num, gen_num);
            }
        }
        1 => {
            let offset = get_var_int(second);
            let gen_num = get_var_int(third);
            if let Ok(gen_num) = u16::try_from(gen_num) {
                table.add_normal(obj_num, gen_num, false, i64::from(offset));
            }
        }
        2 => {
            let archive_obj_num = get_var_int(second);
            if table
                .objects_info()
                .last_key_value()
                .is_none_or(|(&last, _)| archive_obj_num > last)
            {
                return;
            }
            table.add_compressed(obj_num, archive_obj_num, get_var_int(third));
        }
        _ => {}
    }
}

pub fn parse_and_append_cross_ref_subsection_data(
    syntax: &mut CpdfSyntaxParser,
    start_obj_num: u32,
    count: u32,
    out_objects: Option<&mut Vec<CrossRefObjData>>,
) -> bool {
    if count == 0 {
        return true;
    }
    let Some(out_objects) = out_objects else {
        let Some(next_pos) = i64::from(count)
            .checked_mul(ENTRY_SIZE as i64)
            .and_then(|bytes| syntax.get_pos().checked_add(bytes))
        else {
            return false;
        };
        syntax.set_pos(next_pos);
        return true;
    };
    let Some(new_size) = out_objects.len().checked_add(count as usize) else {
        return false;
    };
    if new_size > MAX_XREF_SIZE || new_size > syntax.get_document_size() as usize / ENTRY_SIZE {
        return false;
    }
    let start_obj_index = out_objects.len();
    out_objects.resize(
        new_size,
        CrossRefObjData {
            obj_num: 0,
            info: ObjectInfo::default(),
        },
    );
    let mut entries_to_read = count;
    let mut buffer = vec![0u8; 1024 * ENTRY_SIZE + 1];
    while entries_to_read > 0 {
        let entries_in_block = entries_to_read.min(1024);
        let bytes_to_read = entries_in_block as usize * ENTRY_SIZE;
        if !syntax.read_block(&mut buffer[..bytes_to_read]) {
            return false;
        }
        for i in 0..entries_in_block {
            let index = count - entries_to_read + i;
            let obj_num = start_obj_num.wrapping_add(index);
            let entry = &buffer[i as usize * ENTRY_SIZE..(i as usize + 1) * ENTRY_SIZE];
            let object = &mut out_objects[start_obj_index + index as usize];
            object.obj_num = obj_num;
            let info = &mut object.info;
            if entry[17] == b'f' {
                info.object_type = ObjectType::Free;
            } else {
                let offset = atoi64(&buffer[i as usize * ENTRY_SIZE..]);
                if offset == 0 && !entry[..10].iter().all(u8::is_ascii_digit) {
                    return false;
                }
                info.pos = offset;
                info.gen_num = string_to_int(&buffer[i as usize * ENTRY_SIZE + 11..]) as u16;
                info.object_type = ObjectType::Normal;
            }
        }
        entries_to_read -= entries_in_block;
    }
    true
}

pub fn parse_cross_ref_table(
    syntax: &mut CpdfSyntaxParser,
    out_objects: Option<&mut Vec<CrossRefObjData>>,
) -> bool {
    let mut out_objects = out_objects;
    if let Some(out) = &mut out_objects {
        out.clear();
    }
    if syntax.get_keyword() != b"xref" {
        return false;
    }
    let mut result_objects = Vec::new();
    loop {
        let saved_pos = syntax.get_pos();
        let word = syntax.get_next_word();
        if word.word.is_empty() {
            return false;
        }
        if !word.is_number {
            syntax.set_pos(saved_pos);
            break;
        }
        let start_objnum = atoui(&word.word);
        if start_objnum > MAX_OBJECT_NUMBER {
            return false;
        }
        let count = syntax.get_direct_num();
        syntax.to_next_word();
        if !parse_and_append_cross_ref_subsection_data(
            syntax,
            start_objnum,
            count,
            out_objects.as_ref().map(|_| &mut result_objects),
        ) {
            return false;
        }
    }
    if let Some(out) = out_objects {
        *out = result_objects;
    }
    true
}

pub fn merge_cross_ref_objects_data(
    table: &mut CpdfCrossRefTable,
    objects: &[CrossRefObjData],
) -> bool {
    for obj in objects {
        let info = obj.info;
        let okay = match info.object_type {
            ObjectType::Free => info.gen_num == 0 || table.set_free(obj.obj_num, info.gen_num),
            ObjectType::Normal => table.add_normal(
                obj.obj_num,
                info.gen_num,
                info.is_object_stream_flag,
                info.pos,
            ),
            ObjectType::Compressed => {
                table.add_compressed(obj.obj_num, info.archive_obj_num, info.archive_obj_index)
            }
        };
        if !okay {
            return false;
        }
    }
    true
}

pub fn load_cross_ref_table(
    syntax: &mut CpdfSyntaxParser,
    table: &mut CpdfCrossRefTable,
    pos: i64,
    skip: bool,
) -> bool {
    syntax.set_pos(pos);
    let mut objects = Vec::new();
    if !parse_cross_ref_table(syntax, (!skip).then_some(&mut objects)) {
        return false;
    }
    merge_cross_ref_objects_data(table, &objects)
}

pub fn load_cross_ref_stream(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
    table: &mut CpdfCrossRefTable,
    pos: &mut i64,
    is_main_xref: bool,
) -> bool {
    let saved_pos = syntax.get_pos();
    syntax.set_pos(*pos);
    let object = syntax.get_indirect_object(holder, ParseType::Loose);
    syntax.set_pos(saved_pos);
    let Some(object) = object else { return false };
    let object_borrow = object.borrow();
    let Some(stream) = object_borrow.as_any().downcast_ref::<CpdfStream>() else {
        return false;
    };
    if stream.obj_num() == 0 {
        return false;
    }
    let dict_ref = stream.get_dict();
    let dict_borrow = dict_ref.borrow();
    let dict = dict_borrow
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .expect("PDF stream dictionary");
    let prev = dict.get_integer_for(b"Prev", 0);
    if prev < 0 {
        return false;
    }
    let size = dict.get_integer_for(b"Size", 0);
    if size < 0 || size as usize > MAX_XREF_SIZE {
        return false;
    }
    *pos = i64::from(prev);

    let new_trailer: ObjectRef = dict.clone_object();
    let new_table = CpdfCrossRefTable::new(Some(new_trailer), stream.obj_num());
    if is_main_xref {
        *table = new_table;
        table.set_object_map_size(size as u32);
    } else {
        let current = std::mem::take(table);
        *table = CpdfCrossRefTable::merge_up(Some(new_table), Some(current)).unwrap();
    }

    let indices = dict.get_array_for(b"Index").map_or_else(
        || get_cross_ref_stream_indices(None, size as u32),
        |array| {
            let borrowed = array.borrow();
            get_cross_ref_stream_indices(borrowed.as_any().downcast_ref::<CpdfArray>(), size as u32)
        },
    );
    let field_widths = dict.get_array_for(b"W").map_or_else(Vec::new, |array| {
        let borrowed = array.borrow();
        get_field_widths(borrowed.as_any().downcast_ref::<CpdfArray>())
    });
    if field_widths.len() < 3 {
        return false;
    }
    let Some(total_width) = field_widths
        .iter()
        .try_fold(0u32, |sum, &width| sum.checked_add(width))
    else {
        return false;
    };
    drop(dict_borrow);

    let mut stream_acc = CpdfStreamAcc::new(stream);
    stream_acc.load_all_data_filtered();
    let data = stream_acc.get_span();
    let mut segindex = 0u32;
    for index in indices {
        let Some(seg_end) = segindex
            .checked_add(index.obj_count)
            .and_then(|count| count.checked_mul(total_width))
        else {
            continue;
        };
        if seg_end as usize > data.len() {
            continue;
        }
        let Some(new_size) = index.start_obj_num.checked_add(index.obj_count) else {
            continue;
        };
        let current_size = table
            .objects_info()
            .last_key_value()
            .map_or(0, |(&last, _)| last + 1);
        let new_size = new_size.min(MAX_XREF_SIZE as u32);
        if new_size > current_size {
            table.set_object_map_size(new_size);
        }
        let segment_start = segindex as usize * total_width as usize;
        for i in 0..index.obj_count {
            let obj_num = index.start_obj_num + i;
            if obj_num > MAX_OBJECT_NUMBER {
                break;
            }
            let entry_start = segment_start + i as usize * total_width as usize;
            process_cross_ref_stream_entry(
                table,
                &data[entry_start..entry_start + total_width as usize],
                &field_widths,
                obj_num,
            );
        }
        segindex += index.obj_count;
    }
    true
}

pub fn parse_file_version(syntax: &mut CpdfSyntaxParser) -> Option<i32> {
    let major = syntax.get_char_at(5)?;
    let minor = syntax.get_char_at(7)?;
    Some(
        i32::from(major.is_ascii_digit().then_some(major - b'0').unwrap_or(0)) * 10
            + i32::from(minor.is_ascii_digit().then_some(minor - b'0').unwrap_or(0)),
    )
}

pub fn parse_start_xref(syntax: &mut CpdfSyntaxParser) -> i64 {
    const KEYWORD: &[u8] = b"startxref";
    let document_size = syntax.get_document_size();
    syntax.set_pos((document_size - KEYWORD.len() as i64).max(0));
    if !syntax.backwards_search_to_word(KEYWORD, 4096) {
        return 0;
    }
    syntax.get_keyword();
    let result = syntax.get_next_word();
    if !result.is_number || result.word.is_empty() {
        return 0;
    }
    let value = atoi64(&result.word);
    if value >= document_size {
        0
    } else {
        value
    }
}

pub fn load_trailer(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
) -> Option<ObjectRef> {
    if syntax.get_keyword() != b"trailer" {
        return None;
    }
    let trailer = syntax.get_object_body(holder)?;
    let is_dict = trailer.borrow().as_any().is::<CpdfDictionary>();
    is_dict.then_some(trailer)
}

fn trailer_integer(table: &CpdfCrossRefTable, key: &[u8], direct: bool) -> i32 {
    let Some(trailer) = table.trailer() else {
        return 0;
    };
    let borrowed = trailer.borrow();
    let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
        return 0;
    };
    if direct {
        dict.get_direct_integer_for(key)
    } else {
        dict.get_integer_for(key, 0)
    }
}

pub fn verify_cross_ref_table(syntax: &mut CpdfSyntaxParser, table: &CpdfCrossRefTable) -> bool {
    for (&obj_num, info) in table.objects_info() {
        if info.pos <= 0 {
            continue;
        }
        let saved_pos = syntax.get_pos();
        syntax.set_pos(info.pos);
        let word = syntax.get_next_word();
        syntax.set_pos(saved_pos);
        if !word.is_number || word.word.is_empty() || atoui(&word.word) != obj_num {
            return false;
        }
        break;
    }
    true
}

pub fn find_all_cross_reference_tables_and_streams(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
    table: &mut CpdfCrossRefTable,
    main_xref_offset: i64,
    xref_list: &mut Vec<i64>,
    xref_stream_list: &mut Vec<i64>,
) -> bool {
    let mut seen = BTreeSet::from([main_xref_offset]);
    let mut xref_offset = i64::from(trailer_integer(table, b"Prev", true));
    while xref_offset > 0 {
        if !seen.insert(xref_offset) {
            return false;
        }
        let mut xref_offset_copy = xref_offset;
        if load_cross_ref_stream(syntax, holder, table, &mut xref_offset_copy, false) {
            xref_list.insert(0, 0);
            xref_stream_list.insert(0, xref_offset);
            xref_offset = xref_offset_copy;
        } else {
            load_cross_ref_table(syntax, table, xref_offset, true);
            let Some(trailer) = load_trailer(syntax, holder) else {
                return false;
            };
            let borrowed = trailer.borrow();
            let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
            xref_list.insert(0, xref_offset);
            xref_stream_list.insert(0, i64::from(dict.get_integer_for(b"XRefStm", 0)));
            xref_offset = i64::from(dict.get_direct_integer_for(b"Prev"));
            drop(borrowed);
            let current = std::mem::take(table);
            let older = CpdfCrossRefTable::new(Some(trailer), 0);
            *table = CpdfCrossRefTable::merge_up(Some(older), Some(current)).unwrap();
        }
    }
    true
}

pub fn load_all_cross_ref_tables_and_streams(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
    table: &mut CpdfCrossRefTable,
    xref_offset: i64,
) -> bool {
    let is_xref_stream = !load_cross_ref_table(syntax, table, xref_offset, true);
    if is_xref_stream {
        let mut copy = xref_offset;
        if !load_cross_ref_stream(syntax, holder, table, &mut copy, true) {
            return false;
        }
    } else {
        let Some(trailer) = load_trailer(syntax, holder) else {
            return false;
        };
        table.set_trailer(Some(trailer), 0);
        let size = trailer_integer(table, b"Size", true);
        if size > 0 && size as usize <= MAX_XREF_SIZE {
            table.set_object_map_size(size as u32);
        }
    }
    let mut xref_list;
    let mut xref_stream_list;
    if is_xref_stream {
        xref_list = vec![0];
        xref_stream_list = vec![xref_offset];
    } else {
        xref_list = vec![xref_offset];
        xref_stream_list = vec![i64::from(trailer_integer(table, b"XRefStm", true))];
    }
    if !find_all_cross_reference_tables_and_streams(
        syntax,
        holder,
        table,
        xref_offset,
        &mut xref_list,
        &mut xref_stream_list,
    ) {
        return false;
    }
    if xref_list[0] > 0 {
        if !load_cross_ref_table(syntax, table, xref_list[0], false)
            || !verify_cross_ref_table(syntax, table)
        {
            return false;
        }
    }
    for i in 1..xref_list.len() {
        if xref_stream_list[i] > 0
            && !load_cross_ref_stream(syntax, holder, table, &mut xref_stream_list[i], false)
        {
            return false;
        }
        if xref_list[i] > 0 && !load_cross_ref_table(syntax, table, xref_list[i], false) {
            return false;
        }
    }
    true
}

pub fn rebuild_cross_ref(syntax: &mut CpdfSyntaxParser, table: &mut CpdfCrossRefTable) -> bool {
    let mut rebuilt = CpdfCrossRefTable::default();
    syntax.set_read_buffer_size(4096);
    syntax.set_pos(0);
    let mut numbers: Vec<(u32, i64)> = Vec::new();
    loop {
        let result = syntax.get_next_word();
        let word = result.word;
        if word.is_empty() {
            break;
        }
        if result.is_number {
            numbers.push((atoui(&word), syntax.get_pos() - word.len() as i64));
            if numbers.len() > 2 {
                numbers.remove(0);
            }
            continue;
        }
        if word == b"(" {
            syntax.read_string();
        } else if word == b"<" {
            syntax.read_hex_string();
        } else if word == b"trailer" {
            if let Some(trailer) = syntax.get_object_body(None) {
                let obj_num = trailer.borrow().obj_num();
                let dict = {
                    let borrowed = trailer.borrow();
                    if let Some(stream) = borrowed.as_any().downcast_ref::<CpdfStream>() {
                        Some(stream.get_dict())
                    } else if borrowed.as_any().is::<CpdfDictionary>() {
                        Some(trailer.clone())
                    } else {
                        None
                    }
                };
                rebuilt = CpdfCrossRefTable::merge_up(
                    Some(rebuilt),
                    Some(CpdfCrossRefTable::new(dict, obj_num)),
                )
                .unwrap();
            }
        } else if word == b"obj" && numbers.len() == 2 {
            let obj_pos = numbers[0].1;
            let obj_num = numbers[0].0;
            let gen_num = numbers[1].0;
            syntax.set_pos(obj_pos);
            let object = syntax.get_indirect_object(None, ParseType::Strict);
            let stream = object.as_ref().and_then(|object| {
                let borrowed = object.borrow();
                borrowed
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .map(|_| object.clone())
            });
            if let Some(stream_ref) = &stream {
                let borrowed = stream_ref.borrow();
                let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
                let dict_ref = stream.get_dict();
                let dict_borrow = dict_ref.borrow();
                let dict = dict_borrow
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .unwrap();
                if crate::fxcrt::bytestring::eq_cstr(&dict.get_name_for(b"Type"), b"XRef") {
                    rebuilt = CpdfCrossRefTable::merge_up(
                        Some(rebuilt),
                        Some(CpdfCrossRefTable::new(
                            Some(dict.clone_object()),
                            stream.obj_num(),
                        )),
                    )
                    .unwrap();
                }
            }
            if rebuilt.add_normal(obj_num, gen_num as u16, false, obj_pos) {
                if let Some(stream_ref) = &stream {
                    let borrowed = stream_ref.borrow();
                    let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
                    if let Some(object_stream) = CpdfObjectStream::create(stream) {
                        for (index, info) in object_stream.object_info().iter().enumerate() {
                            rebuilt.add_compressed(info.obj_num, obj_num, index as u32);
                        }
                    }
                }
            }
        }
        numbers.clear();
    }
    let current = std::mem::take(table);
    *table = CpdfCrossRefTable::merge_up(Some(current), Some(rebuilt)).unwrap();
    syntax.set_read_buffer_size(FILE_BUF_SIZE as u32);
    table.trailer().is_some() && !table.objects_info().is_empty()
}

pub fn load_linearized_all_cross_ref_table(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
    table: &mut CpdfCrossRefTable,
    main_xref_offset: i64,
) -> bool {
    if !load_cross_ref_table(syntax, table, main_xref_offset, false) {
        return false;
    }
    let Some(main_trailer) = load_trailer(syntax, holder) else {
        return false;
    };
    if trailer_integer(table, b"Size", true) == 0 {
        return false;
    }
    let xref_stm = i64::from(trailer_integer(table, b"XRefStm", true));
    let mut xref_list = vec![main_xref_offset];
    let mut xref_stream_list = vec![xref_stm];
    let first_page_table = std::mem::take(table);
    let main_table = CpdfCrossRefTable::new(Some(main_trailer), 0);
    *table = CpdfCrossRefTable::merge_up(Some(main_table), Some(first_page_table)).unwrap();
    if !find_all_cross_reference_tables_and_streams(
        syntax,
        holder,
        table,
        main_xref_offset,
        &mut xref_list,
        &mut xref_stream_list,
    ) {
        return false;
    }
    if xref_stream_list[0] > 0
        && !load_cross_ref_stream(syntax, holder, table, &mut xref_stream_list[0], false)
    {
        return false;
    }
    for i in 1..xref_list.len() {
        if xref_stream_list[i] > 0
            && !load_cross_ref_stream(syntax, holder, table, &mut xref_stream_list[i], false)
        {
            return false;
        }
        if xref_list[i] > 0 && !load_cross_ref_table(syntax, table, xref_list[i], false) {
            return false;
        }
    }
    true
}

pub fn load_linearized_all_cross_ref_stream(
    syntax: &mut CpdfSyntaxParser,
    holder: Option<&HolderRef>,
    table: &mut CpdfCrossRefTable,
    main_xref_offset: i64,
) -> bool {
    let mut offset = main_xref_offset;
    if !load_cross_ref_stream(syntax, holder, table, &mut offset, false) {
        return false;
    }
    let mut seen = BTreeSet::new();
    while offset != 0 {
        seen.insert(offset);
        if !load_cross_ref_stream(syntax, holder, table, &mut offset, false) {
            return false;
        }
        if seen.contains(&offset) {
            return false;
        }
    }
    true
}

pub fn get_trailer_ends(syntax: &mut CpdfSyntaxParser) -> Vec<u32> {
    let ends = Rc::new(RefCell::new(Vec::new()));
    syntax.set_trailer_ends(Some(&ends));
    syntax.set_pos(0);
    loop {
        let mut word = syntax.get_next_word();
        if word.is_number {
            word = syntax.get_next_word();
            if !word.is_number {
                break;
            }
            word = syntax.get_next_word();
            if word.word != b"obj" {
                break;
            }
            syntax.get_object_body(None);
            if syntax.get_next_word().word != b"endobj" {
                break;
            }
        } else if word.word == b"trailer" {
            syntax.get_object_body(None);
        } else if word.word == b"startxref" {
            syntax.get_next_word();
        } else if word.word == b"xref" {
            loop {
                word = syntax.get_next_word();
                if word.word.is_empty() || word.word == b"startxref" {
                    break;
                }
            }
            syntax.get_next_word();
        } else {
            break;
        }
    }
    syntax.set_trailer_ends(None);
    Rc::try_unwrap(ends)
        .expect("trailer ends have no other owner")
        .into_inner()
}

pub fn write_to_archive(
    syntax: &mut CpdfSyntaxParser,
    archive: &mut dyn WriteStream,
    mut src_size: i64,
) -> bool {
    if src_size < 0 {
        return false;
    }
    let mut buffer = [0u8; 4096];
    syntax.set_pos(0);
    while src_size != 0 {
        let block_size = src_size.min(buffer.len() as i64) as usize;
        if !syntax.read_block(&mut buffer[..block_size])
            || !archive.write_block(&buffer[..block_size])
        {
            return false;
        }
        src_size -= block_size as i64;
    }
    true
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParserError {
    Success,
    FileError,
    FormatError,
    PasswordError,
    HandlerError,
}

struct ParserState {
    holder: HolderRef,
    validator: RefCell<Option<Rc<CpdfReadValidator>>>,
    header_offset: Cell<i64>,
    cross_ref_table: RefCell<CpdfCrossRefTable>,
    object_stream_map: RefCell<BTreeMap<u32, Option<Rc<CpdfObjectStream>>>>,
    parsing_obj_nums: RefCell<BTreeSet<u32>>,
    security_handler: RefCell<Option<Rc<CpdfSecurityHandler>>>,
    metadata_objnum: Cell<u32>,
}

struct ParsingGuard<'a> {
    state: &'a ParserState,
    obj_num: u32,
}

impl Drop for ParsingGuard<'_> {
    fn drop(&mut self) {
        self.state
            .parsing_obj_nums
            .borrow_mut()
            .remove(&self.obj_num);
    }
}

impl ParserState {
    fn guard(&self, obj_num: u32) -> Option<ParsingGuard<'_>> {
        self.parsing_obj_nums
            .borrow_mut()
            .insert(obj_num)
            .then(|| ParsingGuard {
                state: self,
                obj_num,
            })
    }

    fn last_obj_num(&self) -> u32 {
        self.cross_ref_table
            .borrow()
            .objects_info()
            .last_key_value()
            .map_or(0, |(&num, _)| num)
    }

    fn parse_indirect_object(&self, obj_num: u32) -> Option<ObjectRef> {
        let _guard = self.guard(obj_num)?;
        let info = {
            // An indirect /Length may be requested while an xref stream is
            // still being loaded. Its entries are not available for parsing
            // until that update completes, so let syntax scan for endstream.
            let table = self.cross_ref_table.try_borrow().ok()?;
            if obj_num
                > table
                    .objects_info()
                    .last_key_value()
                    .map_or(0, |(&num, _)| num)
            {
                return None;
            }
            *table.get_object_info(obj_num)?
        };
        match info.object_type {
            ObjectType::Free => None,
            ObjectType::Normal => (info.pos > 0)
                .then(|| self.parse_indirect_object_at(info.pos, obj_num))
                .flatten(),
            ObjectType::Compressed => {
                let stream = self.get_object_stream(info.archive_obj_num)?;
                stream.parse_object(Some(&self.holder), obj_num, info.archive_obj_index)
            }
        }
    }

    fn get_object_stream(&self, obj_num: u32) -> Option<Rc<CpdfObjectStream>> {
        if self.parsing_obj_nums.borrow().contains(&obj_num) {
            return None;
        }
        if let Some(cached) = self.object_stream_map.borrow().get(&obj_num) {
            return cached.clone();
        }
        let info = *self
            .cross_ref_table
            .try_borrow()
            .ok()?
            .get_object_info(obj_num)?;
        if !info.is_object_stream_flag || info.pos <= 0 {
            return None;
        }
        let _guard = self.guard(obj_num)?;
        let object = self.parse_indirect_object_at(info.pos, obj_num)?;
        let stream = {
            let borrowed = object.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfStream>()
                .and_then(CpdfObjectStream::create)
                .map(Rc::new)
        };
        self.object_stream_map
            .borrow_mut()
            .insert(obj_num, stream.clone());
        stream
    }

    fn parse_indirect_object_at(&self, pos: i64, obj_num: u32) -> Option<ObjectRef> {
        let validator = self.validator.borrow().clone()?;
        let mut syntax = CpdfSyntaxParser::new(validator, self.header_offset.get());
        syntax.set_pos(pos);
        let result = syntax.get_indirect_object(Some(&self.holder), ParseType::Loose)?;
        if obj_num != 0 && result.borrow().obj_num() != obj_num {
            return None;
        }
        if obj_num != self.metadata_objnum.get() {
            if let Some(security) = self.security_handler.borrow().as_ref() {
                if let Some(crypto) = security.crypto_handler() {
                    if !crypto.decrypt_object_tree(Some(result.clone())) {
                        return None;
                    }
                }
            }
        }
        Some(result)
    }
}

pub struct CpdfParser {
    state: Rc<ParserState>,
    syntax: Option<CpdfSyntaxParser>,
    has_parsed: bool,
    xref_stream: bool,
    xref_table_rebuilt: bool,
    file_version: i32,
    last_xref_offset: i64,
    password: Vec<u8>,
    linearized: Option<CpdfLinearizedHeader>,
    try_init: Option<Rc<dyn Fn(&CpdfParser) -> bool>>,
}

impl Default for CpdfParser {
    fn default() -> Self {
        Self::new()
    }
}

impl CpdfParser {
    pub fn new() -> Self {
        Self::with_holder(CpdfIndirectObjectHolder::new())
    }

    pub fn with_holder(holder: HolderRef) -> Self {
        let state = Rc::new(ParserState {
            holder: holder.clone(),
            validator: RefCell::new(None),
            header_offset: Cell::new(0),
            cross_ref_table: RefCell::new(CpdfCrossRefTable::default()),
            object_stream_map: RefCell::new(BTreeMap::new()),
            parsing_obj_nums: RefCell::new(BTreeSet::new()),
            security_handler: RefCell::new(None),
            metadata_objnum: Cell::new(0),
        });
        let weak: Weak<ParserState> = Rc::downgrade(&state);
        holder.borrow_mut().set_parser(Some(Rc::new(move |num| {
            weak.upgrade()?.parse_indirect_object(num)
        })));
        Self {
            state,
            syntax: None,
            has_parsed: false,
            xref_stream: false,
            xref_table_rebuilt: false,
            file_version: 0,
            last_xref_offset: 0,
            password: Vec::new(),
            linearized: None,
            try_init: None,
        }
    }

    // CPDF_DataAvail is a native friend. Keep its narrow parser operations
    // internal while preserving the shared lazy-object state and algorithms.
    pub(super) fn init_availability(
        &mut self,
        validator: Rc<CpdfReadValidator>,
        header_offset: i64,
    ) {
        self.state.header_offset.set(header_offset);
        *self.state.validator.borrow_mut() = Some(validator.clone());
        self.syntax = Some(CpdfSyntaxParser::new(validator, header_offset));
    }
    pub(super) fn syntax_for_availability(&mut self) -> Option<&mut CpdfSyntaxParser> {
        self.syntax.as_mut()
    }
    pub(super) fn load_all_cross_refs_for_availability(&mut self, offset: i64) -> bool {
        let Some(syntax) = self.syntax.as_mut() else {
            return false;
        };
        let success = load_all_cross_ref_tables_and_streams(
            syntax,
            Some(&self.state.holder),
            &mut self.state.cross_ref_table.borrow_mut(),
            offset,
        );
        if success {
            self.last_xref_offset = offset;
        }
        success
    }
    // Snapshot the owner before I/O. The owned table is parked locally while
    // callbacks run, so reentry observes no half-written cross-reference map.
    pub(super) fn availability_main_xref_loader(
        &self,
        offset: i64,
    ) -> impl FnOnce() -> (ParserError, bool) + 'static {
        let state = self.state.clone();
        move || {
            if offset < 0 {
                return (ParserError::FormatError, false);
            }
            if offset == 0 {
                return (ParserError::Success, false);
            }
            let Some(validator) = state.validator.borrow().clone() else {
                return (ParserError::FormatError, false);
            };
            let mut syntax = CpdfSyntaxParser::new(validator, state.header_offset.get());
            let metadata = state.metadata_objnum.replace(0);
            state.object_stream_map.borrow_mut().clear();
            let mut table = std::mem::take(&mut *state.cross_ref_table.borrow_mut());
            let table_loaded = load_linearized_all_cross_ref_table(
                &mut syntax,
                Some(&state.holder),
                &mut table,
                offset,
            );
            let loaded = table_loaded
                || load_linearized_all_cross_ref_stream(
                    &mut syntax,
                    Some(&state.holder),
                    &mut table,
                    offset,
                );
            *state.cross_ref_table.borrow_mut() = table;
            state.metadata_objnum.set(metadata);
            (
                if loaded {
                    ParserError::Success
                } else {
                    ParserError::FormatError
                },
                loaded && !table_loaded,
            )
        }
    }
    pub(super) fn finish_availability_main_xref(&mut self, error: ParserError, xref_stream: bool) {
        if error != ParserError::Success {
            self.last_xref_offset = 0;
        }
        if xref_stream {
            self.xref_stream = true;
        }
    }
    pub(super) fn availability_rebuilder(&self) -> impl FnOnce() -> bool + 'static {
        let state = self.state.clone();
        move || {
            let Some(validator) = state.validator.borrow().clone() else {
                return false;
            };
            let mut syntax = CpdfSyntaxParser::new(validator, state.header_offset.get());
            let mut table = std::mem::take(&mut *state.cross_ref_table.borrow_mut());
            let success = rebuild_cross_ref(&mut syntax, &mut table);
            *state.cross_ref_table.borrow_mut() = table;
            success
        }
    }
    pub(super) fn finish_availability_rebuild(&mut self, success: bool) {
        if success {
            self.xref_table_rebuilt = true;
        }
    }
    pub(super) fn availability_object_at_reader(
        &self,
    ) -> impl Fn(i64, u32) -> Option<ObjectRef> + 'static {
        let state = self.state.clone();
        move |position, number| state.parse_indirect_object_at(position, number)
    }
    pub(super) fn availability_object_reader(&self) -> impl Fn(u32) -> Option<ObjectRef> + 'static {
        let state = self.state.clone();
        move |number| state.parse_indirect_object(number)
    }

    pub fn holder(&self) -> HolderRef {
        self.state.holder.clone()
    }
    pub fn set_try_init(&mut self, callback: Option<Rc<dyn Fn(&CpdfParser) -> bool>>) {
        self.try_init = callback;
    }
    pub fn password(&self) -> &[u8] {
        &self.password
    }
    pub fn encoded_password(&self) -> Option<Vec<u8>> {
        self.state
            .security_handler
            .borrow()
            .as_ref()
            .map(|handler| handler.get_encoded_password(&self.password))
    }
    pub fn file_version(&self) -> i32 {
        self.file_version
    }
    pub fn last_xref_offset(&self) -> i64 {
        self.last_xref_offset
    }
    pub fn xref_stream(&self) -> bool {
        self.xref_stream
    }
    pub fn xref_table_rebuilt(&self) -> bool {
        self.xref_table_rebuilt
    }
    pub fn document_size(&self) -> Option<i64> {
        self.syntax
            .as_ref()
            .map(CpdfSyntaxParser::get_document_size)
    }
    pub fn first_page_no(&self) -> u32 {
        self.linearized
            .as_ref()
            .map_or(0, CpdfLinearizedHeader::first_page_no)
    }
    pub fn last_obj_num(&self) -> u32 {
        self.state.last_obj_num()
    }
    pub fn is_valid_object_number(&self, obj_num: u32) -> bool {
        obj_num <= self.last_obj_num()
    }
    pub fn object_position_or_zero(&self, obj_num: u32) -> i64 {
        self.state
            .cross_ref_table
            .borrow()
            .get_object_info(obj_num)
            .filter(|info| info.object_type == ObjectType::Normal)
            .map_or(0, |info| info.pos)
    }
    pub fn is_object_free(&self, obj_num: u32) -> bool {
        assert!(self.is_valid_object_number(obj_num));
        self.state
            .cross_ref_table
            .borrow()
            .get_object_info(obj_num)
            .is_none_or(|info| info.object_type == ObjectType::Free)
    }
    pub fn trailer(&self) -> Option<ObjectRef> {
        self.state.cross_ref_table.borrow().trailer()
    }
    pub fn trailer_object_number(&self) -> u32 {
        self.state.cross_ref_table.borrow().trailer_object_number()
    }
    pub fn combined_trailer(&self) -> Option<ObjectRef> {
        self.trailer()
            .map(|trailer| trailer.borrow().clone_object())
    }
    pub fn cross_ref_table(&self) -> std::cell::Ref<'_, CpdfCrossRefTable> {
        self.state.cross_ref_table.borrow()
    }
    pub fn security_handler(&self) -> std::cell::Ref<'_, Option<Rc<CpdfSecurityHandler>>> {
        self.state.security_handler.borrow()
    }
    /// Retain the security handler without a RefCell guard during archive
    /// callbacks. This mirrors the creator's native RetainPtr.
    pub fn security_handler_ref(&self) -> Option<Rc<CpdfSecurityHandler>> {
        self.state.security_handler.borrow().clone()
    }

    /// Borrow-free access to the same validated source and absolute header
    /// offset used by syntax ReadBlock. The creator drops document borrows
    /// before both file-read callbacks and archive-write callbacks.
    pub(crate) fn archive_source(&self) -> Option<(Rc<CpdfReadValidator>, i64)> {
        self.syntax.as_ref()?;
        Some((
            self.state.validator.borrow().clone()?,
            self.state.header_offset.get(),
        ))
    }
    pub(crate) fn save_position(&self) -> Option<i64> {
        self.syntax.as_ref().map(CpdfSyntaxParser::get_pos)
    }
    pub(crate) fn set_save_position(&mut self, pos: i64) -> bool {
        let Some(syntax) = self.syntax.as_mut() else {
            return false;
        };
        syntax.set_pos(pos);
        true
    }
    pub(crate) fn advance_save_position(&mut self, size: usize) {
        self.syntax
            .as_mut()
            .expect("loaded parser syntax")
            .advance_position_after_save_read(size);
    }

    pub fn permissions(&self, owner: bool) -> u32 {
        self.state
            .security_handler
            .borrow()
            .as_ref()
            .map_or(u32::MAX, |handler| handler.get_permissions(owner))
    }

    fn trailer_reference_num(&self, key: &[u8]) -> u32 {
        let Some(trailer) = self.trailer() else {
            return INVALID_OBJ_NUM;
        };
        let borrowed = trailer.borrow();
        let Some(dict) = borrowed.as_any().downcast_ref::<CpdfDictionary>() else {
            return INVALID_OBJ_NUM;
        };
        let Some(object) = dict.get_object_for(key) else {
            return INVALID_OBJ_NUM;
        };
        let borrowed = object.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfReference>()
            .map_or(INVALID_OBJ_NUM, CpdfReference::ref_obj_num)
    }

    pub fn root_obj_num(&self) -> u32 {
        self.trailer_reference_num(b"Root")
    }
    pub fn info_obj_num(&self) -> u32 {
        self.trailer_reference_num(b"Info")
    }
    pub fn id_array(&self) -> Option<ObjectRef> {
        let trailer = self.trailer()?;
        let borrowed = trailer.borrow();
        borrowed
            .as_any()
            .downcast_ref::<CpdfDictionary>()?
            .get_array_for(b"ID")
    }
    pub fn encrypt_dict(&self) -> Option<ObjectRef> {
        let trailer = self.trailer()?;
        let encrypt = {
            let borrowed = trailer.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()?
                .get_object_for(b"Encrypt")?
        };
        if encrypt.borrow().as_any().is::<CpdfDictionary>() {
            return Some(encrypt);
        }
        let ref_num = {
            let borrowed = encrypt.borrow();
            borrowed
                .as_any()
                .downcast_ref::<CpdfReference>()?
                .ref_obj_num()
        };
        let object =
            CpdfIndirectObjectHolder::get_or_parse_indirect_object(&self.state.holder, ref_num)?;
        let is_dict = object.borrow().as_any().is::<CpdfDictionary>();
        is_dict.then_some(object)
    }
    pub fn root(&self) -> Option<ObjectRef> {
        let object = CpdfIndirectObjectHolder::get_or_parse_indirect_object(
            &self.state.holder,
            self.root_obj_num(),
        )?;
        get_dict_object(&object)
    }
    pub fn parse_indirect_object(&self, obj_num: u32) -> Option<ObjectRef> {
        self.state.parse_indirect_object(obj_num)
    }
    pub fn parse_indirect_object_at(&self, pos: i64, obj_num: u32) -> Option<ObjectRef> {
        self.state.parse_indirect_object_at(pos, obj_num)
    }

    fn init_syntax_parser(&mut self, validator: Rc<CpdfReadValidator>) -> bool {
        let Some(header_offset) = get_header_offset(validator.as_ref()) else {
            return false;
        };
        if validator.get_size() < header_offset + 9 {
            return false;
        }
        self.state.header_offset.set(header_offset);
        *self.state.validator.borrow_mut() = Some(validator.clone());
        let mut syntax = CpdfSyntaxParser::new(validator, header_offset);
        let Some(version) = parse_file_version(&mut syntax) else {
            return false;
        };
        self.file_version = version;
        self.syntax = Some(syntax);
        true
    }

    pub fn start_parse(
        &mut self,
        file: Rc<dyn SeekableReadStream>,
        password: &[u8],
    ) -> ParserError {
        if !self.init_syntax_parser(Rc::new(CpdfReadValidator::new(file, None))) {
            return ParserError::FormatError;
        }
        self.password = password.to_vec();
        self.start_parse_internal()
    }

    pub fn start_linearized_parse(
        &mut self,
        validator: Rc<CpdfReadValidator>,
        password: &[u8],
    ) -> ParserError {
        assert!(!self.has_parsed && !self.xref_table_rebuilt);
        self.password = password.to_vec();
        self.xref_stream = false;
        self.last_xref_offset = 0;
        if !self.init_syntax_parser(validator) {
            return ParserError::FormatError;
        }
        self.linearized = CpdfLinearizedHeader::parse(self.syntax.as_mut().unwrap());
        if self.linearized.is_none() {
            return self.start_parse_internal();
        }
        self.has_parsed = true;
        self.last_xref_offset = self.linearized.as_ref().unwrap().last_xref_offset();
        let mut first_xref_offset = self.last_xref_offset;
        let loaded_table = load_cross_ref_table(
            self.syntax.as_mut().unwrap(),
            &mut self.state.cross_ref_table.borrow_mut(),
            first_xref_offset,
            false,
        );
        if !loaded_table
            && !load_cross_ref_stream(
                self.syntax.as_mut().unwrap(),
                Some(&self.state.holder),
                &mut self.state.cross_ref_table.borrow_mut(),
                &mut first_xref_offset,
                true,
            )
        {
            if !rebuild_cross_ref(
                self.syntax.as_mut().unwrap(),
                &mut self.state.cross_ref_table.borrow_mut(),
            ) {
                return ParserError::FormatError;
            }
            self.xref_table_rebuilt = true;
            self.last_xref_offset = 0;
        }
        if loaded_table {
            let Some(trailer) =
                load_trailer(self.syntax.as_mut().unwrap(), Some(&self.state.holder))
            else {
                return ParserError::Success;
            };
            self.state
                .cross_ref_table
                .borrow_mut()
                .set_trailer(Some(trailer), 0);
            let size = trailer_integer(&self.state.cross_ref_table.borrow(), b"Size", true);
            if size > 0
                && self.last_obj_num() != (size - 1) as u32
                && !rebuild_cross_ref(
                    self.syntax.as_mut().unwrap(),
                    &mut self.state.cross_ref_table.borrow_mut(),
                )
            {
                return ParserError::FormatError;
            }
        }
        let mut error = self.set_encrypt_handler();
        if error != ParserError::Success {
            return error;
        }
        if self.root().is_none() || !self.try_init() {
            if self.xref_table_rebuilt {
                return ParserError::FormatError;
            }
            *self.state.security_handler.borrow_mut() = None;
            if !rebuild_cross_ref(
                self.syntax.as_mut().unwrap(),
                &mut self.state.cross_ref_table.borrow_mut(),
            ) {
                return ParserError::FormatError;
            }
            error = self.set_encrypt_handler();
            if error != ParserError::Success {
                return error;
            }
            self.try_init();
            if self.root().is_none() {
                return ParserError::FormatError;
            }
        }
        if self.root_obj_num() == INVALID_OBJ_NUM {
            *self.state.security_handler.borrow_mut() = None;
            if !rebuild_cross_ref(
                self.syntax.as_mut().unwrap(),
                &mut self.state.cross_ref_table.borrow_mut(),
            ) || self.root_obj_num() == INVALID_OBJ_NUM
            {
                return ParserError::FormatError;
            }
            error = self.set_encrypt_handler();
            if error != ParserError::Success {
                return error;
            }
        }
        let metadata_encrypted = self
            .state
            .security_handler
            .borrow()
            .as_ref()
            .is_some_and(|handler| handler.is_metadata_encrypted());
        if metadata_encrypted {
            if let Some(root) = self.root() {
                let metadata = {
                    let borrowed = root.borrow();
                    borrowed
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .and_then(|dict| dict.get_object_for(b"Metadata"))
                };
                if let Some(metadata) = metadata {
                    let borrowed = metadata.borrow();
                    if let Some(reference) = borrowed.as_any().downcast_ref::<CpdfReference>() {
                        self.state.metadata_objnum.set(reference.ref_obj_num());
                    }
                }
            }
        }
        ParserError::Success
    }

    #[cfg(test)]
    pub(super) fn set_linearized_header_for_testing(&mut self, header: CpdfLinearizedHeader) {
        self.linearized = Some(header);
    }

    pub fn linearized_header(&self) -> Option<&CpdfLinearizedHeader> {
        self.linearized.as_ref()
    }

    pub fn load_linearized_main_xref_table(&mut self) -> ParserError {
        if self.trailer().is_none() {
            return ParserError::FormatError;
        }
        let main_xref_offset = i64::from(trailer_integer(
            &self.state.cross_ref_table.borrow(),
            b"Prev",
            false,
        ));
        if main_xref_offset < 0 {
            return ParserError::FormatError;
        }
        if main_xref_offset == 0 {
            return ParserError::Success;
        }
        let saved_metadata_objnum = self.state.metadata_objnum.replace(0);
        self.state.object_stream_map.borrow_mut().clear();
        let table_loaded = load_linearized_all_cross_ref_table(
            self.syntax.as_mut().unwrap(),
            Some(&self.state.holder),
            &mut self.state.cross_ref_table.borrow_mut(),
            main_xref_offset,
        );
        let loaded = table_loaded
            || load_linearized_all_cross_ref_stream(
                self.syntax.as_mut().unwrap(),
                Some(&self.state.holder),
                &mut self.state.cross_ref_table.borrow_mut(),
                main_xref_offset,
            );
        self.state.metadata_objnum.set(saved_metadata_objnum);
        if !loaded {
            self.last_xref_offset = 0;
            return ParserError::FormatError;
        }
        if !table_loaded {
            self.xref_stream = true;
        }
        ParserError::Success
    }

    /// Own the scan before invoking a host stream, so the document itself can
    /// be queried reentrantly while recording the canonical trailer offsets.
    pub fn trailer_ends_reader(&self) -> Option<impl FnOnce() -> Vec<u32>> {
        self.syntax.as_ref()?;
        let validator = self.state.validator.borrow().clone()?;
        let header_offset = self.state.header_offset.get();
        Some(move || {
            let mut syntax = CpdfSyntaxParser::new(validator, header_offset);
            get_trailer_ends(&mut syntax)
        })
    }

    pub fn trailer_ends(&mut self) -> Option<Vec<u32>> {
        self.syntax.as_mut().map(get_trailer_ends)
    }

    pub fn write_to_archive(&mut self, archive: &mut dyn WriteStream, src_size: i64) -> bool {
        self.syntax
            .as_mut()
            .is_some_and(|syntax| write_to_archive(syntax, archive, src_size))
    }

    fn set_encrypt_handler(&self) -> ParserError {
        *self.state.security_handler.borrow_mut() = None;
        if self.trailer().is_none() {
            return ParserError::FormatError;
        }
        let Some(encrypt_dict) = self.encrypt_dict() else {
            return ParserError::Success;
        };
        let is_standard = {
            let borrowed = encrypt_dict.borrow();
            crate::fxcrt::bytestring::eq_cstr(
                &borrowed
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .unwrap()
                    .get_name_for(b"Filter"),
                b"Standard",
            )
        };
        if !is_standard {
            return ParserError::HandlerError;
        }
        let mut handler = CpdfSecurityHandler::new();
        if !handler.on_init(encrypt_dict, self.id_array().as_ref(), &self.password) {
            return ParserError::PasswordError;
        }
        *self.state.security_handler.borrow_mut() = Some(Rc::new(handler));
        ParserError::Success
    }

    fn try_init(&self) -> bool {
        self.try_init.as_ref().is_none_or(|callback| callback(self))
    }

    fn start_parse_internal(&mut self) -> ParserError {
        assert!(!self.has_parsed && !self.xref_table_rebuilt);
        self.has_parsed = true;
        self.xref_stream = false;
        let syntax = self.syntax.as_mut().unwrap();
        self.last_xref_offset = parse_start_xref(syntax);
        if self.last_xref_offset >= 9 {
            syntax.set_pos(self.last_xref_offset);
            self.xref_stream = syntax.peek_next_word() != b"xref";
            let loaded = load_all_cross_ref_tables_and_streams(
                syntax,
                Some(&self.state.holder),
                &mut self.state.cross_ref_table.borrow_mut(),
                self.last_xref_offset,
            );
            if !loaded {
                self.xref_stream = false;
                if !rebuild_cross_ref(syntax, &mut self.state.cross_ref_table.borrow_mut()) {
                    return ParserError::FormatError;
                }
                self.xref_table_rebuilt = true;
                self.last_xref_offset = 0;
            }
        } else {
            if !rebuild_cross_ref(syntax, &mut self.state.cross_ref_table.borrow_mut()) {
                return ParserError::FormatError;
            }
            self.xref_table_rebuilt = true;
        }
        let mut error = self.set_encrypt_handler();
        if error != ParserError::Success {
            return error;
        }
        if self.root().is_none() || !self.try_init() {
            if self.xref_table_rebuilt {
                return ParserError::FormatError;
            }
            *self.state.security_handler.borrow_mut() = None;
            if !rebuild_cross_ref(
                self.syntax.as_mut().unwrap(),
                &mut self.state.cross_ref_table.borrow_mut(),
            ) {
                return ParserError::FormatError;
            }
            error = self.set_encrypt_handler();
            if error != ParserError::Success {
                return error;
            }
            self.try_init();
            if self.root().is_none() {
                return ParserError::FormatError;
            }
        }
        if self.root_obj_num() == INVALID_OBJ_NUM {
            *self.state.security_handler.borrow_mut() = None;
            if !rebuild_cross_ref(
                self.syntax.as_mut().unwrap(),
                &mut self.state.cross_ref_table.borrow_mut(),
            ) || self.root_obj_num() == INVALID_OBJ_NUM
            {
                return ParserError::FormatError;
            }
            error = self.set_encrypt_handler();
            if error != ParserError::Success {
                return error;
            }
        }
        let metadata_unencrypted = self
            .state
            .security_handler
            .borrow()
            .as_ref()
            .is_some_and(|handler| !handler.is_metadata_encrypted());
        if metadata_unencrypted {
            if let Some(root) = self.root() {
                let metadata = {
                    let borrowed = root.borrow();
                    borrowed
                        .as_any()
                        .downcast_ref::<CpdfDictionary>()
                        .and_then(|dict| dict.get_object_for(b"Metadata"))
                };
                if let Some(metadata) = metadata {
                    let borrowed = metadata.borrow();
                    if let Some(reference) = borrowed.as_any().downcast_ref::<CpdfReference>() {
                        self.state.metadata_objnum.set(reference.ref_obj_num());
                    }
                }
            }
        }
        ParserError::Success
    }
}

#[cfg(test)]
mod tests {
    use std::rc::Rc;

    use super::{
        load_all_cross_ref_tables_and_streams, load_cross_ref_stream, load_cross_ref_table,
        load_linearized_all_cross_ref_table, parse_and_append_cross_ref_subsection_data,
        parse_cross_ref_table, parse_file_version, parse_start_xref,
        process_cross_ref_stream_entry, rebuild_cross_ref, CpdfParser, ParserError,
    };
    use crate::fpdfapi::parser::cpdf_cross_ref_table::{CpdfCrossRefTable, ObjectInfo, ObjectType};
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_read_validator::CpdfReadValidator;
    use crate::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject;
    use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
    use crate::fpdfapi::parser::cpdf_stream_acc::CpdfStreamAcc;
    use crate::fpdfapi::parser::cpdf_syntax_parser::CpdfSyntaxParser;
    use crate::fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream;

    fn make_syntax(bytes: &[u8]) -> CpdfSyntaxParser {
        CpdfSyntaxParser::from_file(Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec())))
    }

    #[test]
    fn reads_fixed_width_entries_and_leaves_trailer_for_caller() {
        let mut syntax = make_syntax(
            b"xref\n0 2\n0000000000 65535 f\r\n0000000042 00000 n\r\ntrailer << /Size 2 >>",
        );
        let mut table = CpdfCrossRefTable::default();
        assert!(load_cross_ref_table(&mut syntax, &mut table, 0, false));
        assert_eq!(
            table.get_object_info(1).unwrap().object_type,
            ObjectType::Normal
        );
        assert_eq!(table.get_object_info(1).unwrap().pos, 42);
        assert_eq!(syntax.get_keyword(), b"trailer");
    }

    #[test]
    fn xref_generation_signs_block_tails_and_failure_storage_follow_native() {
        let mut syntax = make_syntax(b"0000000009  ++07 n\r\n");
        let mut objects = Vec::new();
        assert!(parse_and_append_cross_ref_subsection_data(
            &mut syntax,
            1,
            1,
            Some(&mut objects)
        ));
        assert_eq!(objects[0].info.gen_num, 7);

        // C++ parses from each entry into the rest of its input block, not a
        // separately bounded 20-byte slice. Malformed numeric tails can join.
        let mut syntax = make_syntax(b"000000000000000000000000000009 00000 n\r\n");
        let mut objects = Vec::new();
        assert!(parse_and_append_cross_ref_subsection_data(
            &mut syntax,
            1,
            2,
            Some(&mut objects)
        ));
        assert_eq!((objects[0].info.pos, objects[0].info.gen_num), (9, 9));

        let mut syntax = make_syntax(b"0000000009 00000 n\r\n00000x0000 00000 n\r\n");
        let mut objects = Vec::new();
        assert!(!parse_and_append_cross_ref_subsection_data(
            &mut syntax,
            10,
            2,
            Some(&mut objects)
        ));
        // Native resizes first and assigns the object number before validating
        // its offset, retaining that partial state when a later entry fails.
        assert_eq!(objects.len(), 2);
        assert_eq!(objects[1].obj_num, 11);
        assert_eq!(objects[1].info, ObjectInfo::default());
    }

    #[test]
    fn rejects_invalid_zero_offset_and_out_of_range_subsection() {
        let mut syntax = make_syntax(b"xref\n0 1\n00000x0000 00000 n\ntrailer");
        assert!(!parse_cross_ref_table(&mut syntax, Some(&mut Vec::new())));
        let mut syntax = make_syntax(b"xref\n4294967296 1\n0000000001 00000 n\r\ntrailer");
        assert!(!parse_cross_ref_table(&mut syntax, Some(&mut Vec::new())));
        let mut syntax = make_syntax(b"xref\n26000000 1\n0000000001 00000 n\r\ntrailer");
        assert!(!parse_cross_ref_table(&mut syntax, Some(&mut Vec::new())));
    }

    #[test]
    fn decodes_stream_entries_with_default_type_and_archive_bounds() {
        let mut table = CpdfCrossRefTable::default();
        table.set_object_map_size(4);
        process_cross_ref_stream_entry(&mut table, &[0, 42, 0], &[0, 2, 1], 1);
        assert_eq!(table.get_object_info(1).unwrap().pos, 42);
        process_cross_ref_stream_entry(&mut table, &[2, 3, 5], &[1, 1, 1], 2);
        assert_eq!(
            table.get_object_info(2).unwrap().object_type,
            ObjectType::Compressed
        );
        assert_eq!(table.get_object_info(2).unwrap().archive_obj_index, 5);
        process_cross_ref_stream_entry(&mut table, &[2, 4, 0], &[1, 1, 1], 3);
        assert_eq!(
            table.get_object_info(3).unwrap().object_type,
            ObjectType::Free
        );
    }

    #[test]
    fn loads_cross_reference_stream_entries() {
        let mut bytes =
            b"%PDF-1.7\n1 0 obj << /Type /XRef /Size 3 /W [1 1 1] /Length 9 >> stream\n".to_vec();
        bytes.extend_from_slice(&[0, 0, 255, 1, 42, 0, 2, 1, 0]);
        bytes.extend_from_slice(b"\nendstream\nendobj\n");
        let mut syntax = make_syntax(&bytes);
        let mut table = CpdfCrossRefTable::default();
        let mut offset = 9;
        assert!(load_cross_ref_stream(
            &mut syntax,
            None,
            &mut table,
            &mut offset,
            true
        ));
        assert_eq!(offset, 0);
        assert_eq!(table.trailer_object_number(), 1);
        assert_eq!(table.get_object_info(1).unwrap().pos, 42);
        assert_eq!(table.get_object_info(2).unwrap().archive_obj_num, 1);
    }

    #[test]
    fn loads_document_cross_reference_table_from_startxref() {
        let mut bytes = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n".to_vec();
        let xref_pos = bytes.len();
        bytes.extend_from_slice(
            format!("xref\n0 2\n0000000000 65535 f\r\n{:010} 00000 n\r\ntrailer << /Size 2 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF", 9).as_bytes(),
        );
        let mut syntax = make_syntax(&bytes);
        assert_eq!(parse_file_version(&mut syntax), Some(17));
        assert_eq!(parse_start_xref(&mut syntax), xref_pos as i64);
        let mut table = CpdfCrossRefTable::default();
        assert!(load_all_cross_ref_tables_and_streams(
            &mut syntax,
            None,
            &mut table,
            xref_pos as i64
        ));
        assert_eq!(table.get_object_info(1).unwrap().pos, 9);
        assert!(table.trailer().is_some());
    }

    #[test]
    fn rebuilds_object_positions_without_a_cross_reference_section() {
        let mut syntax = make_syntax(
            b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R /Size 2 >>\n",
        );
        let mut table = CpdfCrossRefTable::default();
        assert!(rebuild_cross_ref(&mut syntax, &mut table));
        assert_eq!(table.get_object_info(1).unwrap().pos, 9);
        assert!(table.trailer().is_some());
    }

    #[test]
    fn merges_main_linearized_trailer_with_first_page_trailer() {
        let mut bytes = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n".to_vec();
        let main_offset = bytes.len() as i64;
        bytes.extend_from_slice(
            b"xref\n0 2\n0000000000 65535 f\r\n0000000009 00000 n\r\ntrailer << /Root 1 0 R /Size 2 >>\n",
        );
        let mut first_page = CpdfDictionary::new();
        first_page.set_new_for(b"Size", CpdfScalarObject::number_from_signed(2));
        first_page.set_new_for(b"Prev", CpdfScalarObject::number_from_signed(0));
        let mut table = CpdfCrossRefTable::new(Some(retain_object(first_page)), 0);
        let mut syntax = make_syntax(&bytes);
        assert!(load_linearized_all_cross_ref_table(
            &mut syntax,
            None,
            &mut table,
            main_offset
        ));
        assert_eq!(table.get_object_info(1).unwrap().pos, 9);
        assert_eq!(super::trailer_integer(&table, b"Size", true), 2);
    }

    #[test]
    fn starts_document_and_resolves_catalog_reference() {
        let mut bytes = b"%PDF-1.7\n1 0 obj << /Type /Catalog >> endobj\n".to_vec();
        let xref_pos = bytes.len();
        bytes.extend_from_slice(
            format!("xref\n0 2\n0000000000 65535 f\r\n0000000009 00000 n\r\ntrailer << /Size 2 /Root 1 0 R >>\nstartxref\n{xref_pos}\n%%EOF").as_bytes(),
        );
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes));
        let mut parser = CpdfParser::new();
        assert_eq!(parser.start_parse(file, b""), ParserError::Success);
        assert_eq!(parser.file_version(), 17);
        assert_eq!(parser.root_obj_num(), 1);
        assert_eq!(parser.object_position_or_zero(1), 9);
        assert_eq!(parser.permissions(false), u32::MAX);
        let root = parser.root().unwrap();
        let borrowed = root.borrow();
        assert_eq!(
            borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .unwrap()
                .get_name_for(b"Type"),
            b"Catalog"
        );
    }

    #[test]
    fn starts_encrypted_pdfium_fixture_and_decrypts_content_stream() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/encrypted_hello_world_r6.pdf"
        ));
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
        let mut parser = CpdfParser::new();
        assert_eq!(
            parser.start_parse(file, b"\xc3\xa2ge"),
            ParserError::Success
        );
        assert_eq!(parser.root_obj_num(), 1);
        let object = parser.parse_indirect_object(4).unwrap();
        let borrowed = object.borrow();
        let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
        let mut acc = CpdfStreamAcc::new(stream);
        acc.load_all_data_filtered();
        assert!(acc.get_span().windows(5).any(|window| window == b"Hello"));
    }

    #[test]
    fn loads_linearized_pdfium_fixture_and_main_xref() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/linearized.pdf"
        ));
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
        let validator = Rc::new(CpdfReadValidator::new(file, None));
        let mut parser = CpdfParser::new();
        assert_eq!(
            parser.start_linearized_parse(validator, b""),
            ParserError::Success
        );
        assert!(parser.linearized_header().is_some());
        assert_eq!(parser.first_page_no(), 0);
        assert_eq!(parser.root_obj_num(), 29);
        assert_eq!(
            parser.load_linearized_main_xref_table(),
            ParserError::Success
        );
        assert!(parser.parse_indirect_object(25).is_some());
    }

    #[test]
    fn resolves_compressed_object_from_pdfium_xref_stream_fixture() {
        let bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/bug_1399.pdf"
        ));
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes.to_vec()));
        let mut parser = CpdfParser::new();
        assert_eq!(parser.start_parse(file, b""), ParserError::Success);
        assert!(parser.xref_stream());
        let pages = parser.parse_indirect_object(2).unwrap();
        let borrowed = pages.borrow();
        let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
        assert_eq!(dict.get_name_for(b"Type"), b"Pages");
    }

    #[test]
    fn xref_stream_with_indirect_length_uses_stream_boundary() {
        let mut bytes = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/bug_1399.pdf"
        ))
        .to_vec();
        let length_pos = bytes
            .windows(10)
            .rposition(|word| word == b"/Length 73")
            .unwrap();
        bytes.splice(
            length_pos..length_pos + 10,
            b"/Length 3 0 R".iter().copied(),
        );
        let file = Rc::new(CfxReadOnlyContainerStream::new(bytes));
        let mut parser = CpdfParser::new();
        assert_eq!(parser.start_parse(file, b""), ParserError::Success);
        assert!(parser.parse_indirect_object(2).is_some());
    }
}

#[cfg(test)]
#[path = "cpdf_parser_unittest.rs"]
mod upstream_tests;

#[cfg(test)]
#[path = "../../fxcrt/scoped_set_insertion_unittest.rs"]
mod scoped_set_insertion_upstream_tests;

#[cfg(test)]
#[path = "cpdf_parser_copy_tests.rs"]
mod copy_tests;

#[cfg(test)]
#[path = "cpdf_parser_security_recovery_tests.rs"]
mod security_recovery_tests;
