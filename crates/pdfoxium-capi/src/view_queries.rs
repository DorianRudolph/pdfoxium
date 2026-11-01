// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Remaining document queries from fpdfsdk/fpdf_view.cpp.
use super::*;
use pdfoxium_core::fpdfapi::parser::{
    cpdf_array::CpdfArray,
    cpdf_object::{get_direct_object, ObjectRef},
    cpdf_scalar_object::ObjectType,
    cpdf_stream::CpdfStream,
    cpdf_stream_acc::CpdfStreamAcc,
    fpdf_parser_decode::pdf_decode_text,
};
use pdfoxium_core::fpdfdoc::cpdf_nametree::CpdfNameTree;
use std::ffi::c_long;

fn entry(dict: &ObjectRef, name: &[u8]) -> Option<ObjectRef> {
    let raw = dict
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()?
        .get_object_for(name)?;
    get_direct_object(&raw)
}
fn typed_entry(dict: &ObjectRef, name: &[u8], kind: ObjectType) -> Option<ObjectRef> {
    entry(dict, name).filter(|value| value.borrow().object_type() == kind)
}
fn destination(value: ObjectRef) -> Option<ObjectRef> {
    let kind = value.borrow().object_type();
    match kind {
        ObjectType::Array => Some(value),
        ObjectType::Dictionary => typed_entry(&value, b"D", ObjectType::Array),
        _ => None,
    }
}

#[no_mangle]
pub unsafe extern "C" fn FPDF_CountNamedDests(document: *mut c_void) -> c_ulong {
    let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let Some(root) = handle.document.borrow().root() else {
        return 0;
    };
    let count = CpdfNameTree::create_from_root(&root, b"Dests").map_or(0, |tree| tree.count());
    let old_count = typed_entry(&root, b"Dests", ObjectType::Dictionary).map_or(0, |dict| {
        dict.borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .unwrap()
            .size()
    });
    count
        .checked_add(old_count)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(0) as c_ulong
}

#[no_mangle]
pub unsafe extern "C" fn FPDF_GetNamedDestByName(
    document: *mut c_void,
    name: *const c_char,
) -> *mut c_void {
    if name.is_null() {
        return std::ptr::null_mut();
    }
    let name = unsafe { CStr::from_ptr(name) }.to_bytes();
    if name.is_empty() {
        return std::ptr::null_mut();
    }
    let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let document = handle.document.clone();
    let cache = handle.borrowed_objects.clone();
    let Some(root) = document.borrow().root() else {
        return std::ptr::null_mut();
    };
    let value = CpdfNameTree::create_from_root(&root, b"Dests")
        .and_then(|tree| tree.lookup_value(&pdf_decode_text(name)))
        .and_then(destination)
        .or_else(|| {
            typed_entry(&root, b"Dests", ObjectType::Dictionary)
                .and_then(|dict| entry(&dict, name))
                .and_then(destination)
        });
    value.map_or(std::ptr::null_mut(), |value| {
        doc::borrow_object(&cache, value)
    })
}

#[no_mangle]
pub unsafe extern "C" fn FPDF_GetNamedDest(
    document: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    buflen: *mut c_long,
) -> *mut c_void {
    // Unlike the native unchecked output dereference, reject a missing length.
    if buflen.is_null() {
        return std::ptr::null_mut();
    }
    if buffer.is_null() {
        unsafe {
            *buflen = 0;
        }
    }
    let Ok(index) = usize::try_from(index) else {
        return std::ptr::null_mut();
    };
    let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return std::ptr::null_mut();
    };
    let document = handle.document.clone();
    let cache = handle.borrowed_objects.clone();
    let Some(root) = document.borrow().root() else {
        return std::ptr::null_mut();
    };
    let tree = CpdfNameTree::create_from_root(&root, b"Dests");
    let count = tree.as_ref().map_or(0, CpdfNameTree::count);
    let found = if index < count {
        tree.unwrap().lookup_value_and_name(index)
    } else {
        typed_entry(&root, b"Dests", ObjectType::Dictionary).and_then(|dict| {
            let pair = {
                let borrowed = dict.borrow();
                let dict = borrowed.as_any().downcast_ref::<CpdfDictionary>().unwrap();
                let total = count.checked_add(dict.size())?;
                if total > i32::MAX as usize || index >= total {
                    return None;
                }
                dict.with_locked_entries(|entries| {
                    entries
                        .iter()
                        .nth(index - count)
                        .map(|(key, value)| (key.clone(), value.clone()))
                })
            }?;
            Some((get_direct_object(&pair.1)?, pdf_decode_text(&pair.0)))
        })
    };
    let Some((value, name)) = found else {
        return std::ptr::null_mut();
    };
    let Some(value) = destination(value) else {
        return std::ptr::null_mut();
    };
    let bytes: Vec<u8> = pdfoxium_core::fxcrt::fx_string::utf16_encode(&name)
        .into_iter()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    let Ok(required) = c_long::try_from(bytes.len()) else {
        return std::ptr::null_mut();
    };
    if buffer.is_null() {
        unsafe {
            *buflen = required;
        }
    } else if unsafe { *buflen } >= required {
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), buffer.cast(), bytes.len());
            *buflen = required;
        }
    } else {
        unsafe {
            *buflen = -1;
        }
    }
    doc::borrow_object(&cache, value)
}

struct XfaPacket {
    name: Vec<u8>,
    stream: ObjectRef,
}
fn xfa_packets(document: &Rc<RefCell<CpdfDocument>>) -> Vec<XfaPacket> {
    let Some(root) = document.borrow().root() else {
        return Vec::new();
    };
    let Some(xfa) = typed_entry(&root, b"AcroForm", ObjectType::Dictionary)
        .and_then(|acro| entry(&acro, b"XFA"))
    else {
        return Vec::new();
    };
    if xfa.borrow().object_type() == ObjectType::Stream {
        return vec![XfaPacket {
            name: Vec::new(),
            stream: xfa,
        }];
    }
    let count = xfa
        .borrow()
        .as_any()
        .downcast_ref::<CpdfArray>()
        .map_or(0, CpdfArray::size);
    let mut packets = Vec::with_capacity(count / 2);
    for index in (0..count.saturating_sub(1)).step_by(2) {
        let (name, data) = {
            let borrowed = xfa.borrow();
            let array = borrowed.as_any().downcast_ref::<CpdfArray>().unwrap();
            (array.get_object_at(index), array.get_object_at(index + 1))
        };
        let Some(name) = name.and_then(|name| get_direct_object(&name)) else {
            continue;
        };
        if name.borrow().object_type() != ObjectType::String {
            continue;
        }
        let Some(stream) = data.and_then(|data| get_direct_object(&data)) else {
            continue;
        };
        if stream.borrow().object_type() != ObjectType::Stream {
            continue;
        }
        packets.push(XfaPacket {
            name: name.borrow().get_string(),
            stream,
        });
    }
    packets
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetXFAPacketCount(document: *mut c_void) -> c_int {
    let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return -1;
    };
    let document = handle.document.clone();
    c_int::try_from(xfa_packets(&document).len()).unwrap_or(-1)
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetXFAPacketName(
    document: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    buflen: c_ulong,
) -> c_ulong {
    let (Some(handle), Ok(index)) = (
        (unsafe { document.cast::<DocumentHandle>().as_ref() }),
        usize::try_from(index),
    ) else {
        return 0;
    };
    let document = handle.document.clone();
    let packets = xfa_packets(&document);
    let Some(packet) = packets.get(index) else {
        return 0;
    };
    unsafe { doc::copy_terminated(&packet.name, buffer, buflen) }
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetXFAPacketContent(
    document: *mut c_void,
    index: c_int,
    buffer: *mut c_void,
    buflen: c_ulong,
    out_buflen: *mut c_ulong,
) -> c_int {
    if out_buflen.is_null() {
        return 0;
    }
    let (Some(handle), Ok(index)) = (
        (unsafe { document.cast::<DocumentHandle>().as_ref() }),
        usize::try_from(index),
    ) else {
        return 0;
    };
    let document = handle.document.clone();
    let packets = xfa_packets(&document);
    let Some(packet) = packets.get(index) else {
        return 0;
    };
    let borrowed = packet.stream.borrow();
    let stream = borrowed.as_any().downcast_ref::<CpdfStream>().unwrap();
    let mut acc = CpdfStreamAcc::new(stream);
    acc.load_all_data_filtered();
    unsafe {
        *out_buflen = doc::copy_bytes(acc.get_span(), buffer, buflen);
    }
    1
}
#[no_mangle]
pub unsafe extern "C" fn FPDF_GetTrailerEnds(
    document: *mut c_void,
    buffer: *mut u32,
    length: c_ulong,
) -> c_ulong {
    let Some(handle) = (unsafe { document.cast::<DocumentHandle>().as_ref() }) else {
        return 0;
    };
    let document = handle.document.clone();
    let Some(read) = document.borrow().parser().trailer_ends_reader() else {
        return 0;
    };
    let ends = read();
    if !buffer.is_null() && length as usize >= ends.len() {
        unsafe {
            std::ptr::copy_nonoverlapping(ends.as_ptr(), buffer, ends.len());
        }
    }
    ends.len() as c_ulong
}

#[cfg(test)]
#[path = "view_queries_tests.rs"]
mod tests;
