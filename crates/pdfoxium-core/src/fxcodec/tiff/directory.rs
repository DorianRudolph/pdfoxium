// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Small TIFF directory view for the published decoder's raw-sample boundary.
//! Compressed chunks stay borrowed at their original offsets. Only an IFD and
//! its overridden values are appended; no compressed image is copied or parsed
//! by a second codec. Both standard TIFF and BigTIFF retain their byte order.
use super::super::progressive_decoder_context::{bytes, CodecError, CodecResult};
use std::{
    collections::{BTreeMap, HashSet},
    io::{self, Read, Seek, SeekFrom},
};

#[derive(Clone, Copy)]
pub(super) struct Endian(pub bool);
impl Endian {
    pub fn value(self, data: &[u8]) -> u64 {
        if self.0 {
            data.iter().rev().fold(0, |v, &b| v * 256 + u64::from(b))
        } else {
            data.iter().fold(0, |v, &b| v * 256 + u64::from(b))
        }
    }
    fn put(self, value: u64, size: usize) -> Vec<u8> {
        let data = if self.0 {
            value.to_le_bytes()
        } else {
            value.to_be_bytes()
        };
        if self.0 {
            data[..size].to_vec()
        } else {
            data[8 - size..].to_vec()
        }
    }
}
pub(super) struct Override {
    pub tag: u16,
    kind: u16,
    count: usize,
    data: Vec<u8>,
}
impl Override {
    pub fn short(tag: u16, values: &[u16], endian: Endian) -> Self {
        Self {
            tag,
            kind: 3,
            count: values.len(),
            data: values
                .iter()
                .flat_map(|&v| endian.put(u64::from(v), 2))
                .collect(),
        }
    }
    pub fn long(tag: u16, value: u32, endian: Endian) -> Self {
        Self {
            tag,
            kind: 4,
            count: 1,
            data: endian.put(u64::from(value), 4),
        }
    }
}
pub(super) struct DirectoryView<'a> {
    data: &'a [u8],
    pub endian: Endian,
    big: bool,
    entries: BTreeMap<u16, &'a [u8]>,
    pointer: u64,
    prefix: Vec<u8>,
    suffix: Vec<u8>,
    position: u64,
    reverse_ranges: Vec<std::ops::Range<usize>>,
}
impl<'a> DirectoryView<'a> {
    pub fn new(data: &'a [u8], frame: usize) -> CodecResult<Self> {
        let endian = match bytes(data, 0, 2)? {
            b"II" => Endian(true),
            b"MM" => Endian(false),
            _ => return Err(CodecError::InvalidData),
        };
        let magic = endian.value(bytes(data, 2, 2)?);
        let big = match magic {
            42 => false,
            43 => true,
            _ => return Err(CodecError::InvalidData),
        };
        let (header_size, pointer_offset, pointer_size, count_size, entry_size) = if big {
            (16, 8, 8, 8, 20)
        } else {
            (8, 4, 4, 2, 12)
        };
        if big && (endian.value(bytes(data, 4, 2)?) != 8 || endian.value(bytes(data, 6, 2)?) != 0) {
            return Err(CodecError::InvalidData);
        }
        if frame > u16::MAX as usize {
            return Err(CodecError::InvalidData);
        }
        let mut pointer = endian.value(bytes(data, pointer_offset, pointer_size)?);
        let mut visited = HashSet::new();
        let mut entries = BTreeMap::new();
        for index in 0..=frame {
            if pointer == 0 || !visited.insert(pointer) {
                return Err(CodecError::InvalidData);
            }
            let offset = usize::try_from(pointer).map_err(|_| CodecError::InvalidData)?;
            let count = usize::try_from(endian.value(bytes(data, offset, count_size)?))
                .map_err(|_| CodecError::InvalidData)?;
            if count > u16::MAX as usize {
                return Err(CodecError::InvalidData);
            }
            let table = bytes(
                data,
                offset
                    .checked_add(count_size)
                    .ok_or(CodecError::InvalidData)?,
                count
                    .checked_mul(entry_size)
                    .ok_or(CodecError::InvalidData)?,
            )?;
            if index == frame {
                for entry in table.chunks_exact(entry_size) {
                    let tag = endian.value(&entry[..2]) as u16;
                    if entries.insert(tag, entry).is_some() {
                        return Err(CodecError::InvalidData);
                    }
                }
            } else {
                pointer = endian.value(bytes(
                    data,
                    offset + count_size + table.len(),
                    pointer_size,
                )?);
            }
        }
        let mut prefix = bytes(data, 0, header_size)?.to_vec();
        prefix[pointer_offset..pointer_offset + pointer_size]
            .copy_from_slice(&endian.put(pointer, pointer_size));
        Ok(Self {
            data,
            endian,
            big,
            entries,
            pointer,
            prefix,
            suffix: Vec::new(),
            position: 0,
            reverse_ranges: Vec::new(),
        })
    }
    pub fn reverse_payloads(mut self, ranges: Vec<(usize, usize)>) -> CodecResult<Self> {
        let mut sorted = Vec::new();
        for (start, length) in ranges {
            let end = start.checked_add(length).ok_or(CodecError::InvalidData)?;
            if start < self.prefix.len() || end > self.data.len() {
                return Err(CodecError::InvalidData);
            }
            sorted.push(start..end);
        }
        sorted.sort_by_key(|r| r.start);
        for range in sorted {
            if let Some(last) = self.reverse_ranges.last_mut() {
                if range.start <= last.end {
                    last.end = last.end.max(range.end);
                    continue;
                }
            }
            self.reverse_ranges.push(range);
        }
        Ok(self)
    }
    pub fn count(&self, tag: u16) -> CodecResult<usize> {
        let Some(entry) = self.entries.get(&tag) else {
            return Ok(0);
        };
        let size = if self.big { 8 } else { 4 };
        usize::try_from(self.endian.value(&entry[4..4 + size])).map_err(|_| CodecError::InvalidData)
    }
    /// Only used for native defaults needed before Decoder::new can accept an
    /// otherwise valid directory with its optional Photometric tag omitted.
    pub fn unsigned(&self, tag: u16) -> CodecResult<Option<u64>> {
        let Some(entry) = self.entries.get(&tag) else {
            return Ok(None);
        };
        let kind = self.endian.value(&entry[2..4]);
        let size = match kind {
            1 => 1,
            3 => 2,
            4 | 13 => 4,
            16 | 18 => 8,
            _ => return Err(CodecError::InvalidData),
        };
        let (count_size, value_start, value_size) = if self.big { (8, 12, 8) } else { (4, 8, 4) };
        let count = self.endian.value(&entry[4..4 + count_size]);
        if count != 1 {
            return Err(CodecError::InvalidData);
        }
        let value = if size <= value_size {
            &entry[value_start..value_start + size]
        } else {
            let pointer = usize::try_from(self.endian.value(&entry[value_start..]))
                .map_err(|_| CodecError::InvalidData)?;
            bytes(self.data, pointer, size)?
        };
        Ok(Some(self.endian.value(value)))
    }
    pub fn overrides(mut self, overrides: Vec<Override>) -> CodecResult<Self> {
        if overrides.is_empty() {
            return Ok(self);
        }
        let (count_size, entry_size, pointer_size, pointer_offset) = if self.big {
            (8, 20, 8, 8)
        } else {
            (2, 12, 4, 4)
        };
        let mut entries: BTreeMap<_, _> = self
            .entries
            .iter()
            .map(|(&tag, &bytes)| (tag, bytes.to_vec()))
            .collect();
        let mut replacements = BTreeMap::new();
        for value in overrides {
            replacements.insert(value.tag, value);
        }
        for &tag in replacements.keys() {
            entries.entry(tag).or_default();
        }
        if entries.len() > u16::MAX as usize {
            return Err(CodecError::InvalidData);
        }
        let start = self.data.len();
        let mut extra_offset = start
            .checked_add(count_size + entries.len() * entry_size + pointer_size)
            .ok_or(CodecError::InvalidData)?;
        let mut values = Vec::new();
        self.suffix = self.endian.put(entries.len() as u64, count_size);
        for (tag, entry) in entries {
            let Some(value) = replacements.remove(&tag) else {
                self.suffix.extend_from_slice(&entry);
                continue;
            };
            self.suffix.extend(self.endian.put(u64::from(tag), 2));
            self.suffix
                .extend(self.endian.put(u64::from(value.kind), 2));
            self.suffix
                .extend(self.endian.put(value.count as u64, pointer_size));
            if value.data.len() <= pointer_size {
                self.suffix.extend_from_slice(&value.data);
                self.suffix
                    .resize(self.suffix.len() + pointer_size - value.data.len(), 0);
            } else {
                if !self.big && extra_offset > u32::MAX as usize {
                    return Err(CodecError::InvalidData);
                }
                self.suffix
                    .extend(self.endian.put(extra_offset as u64, pointer_size));
                extra_offset = extra_offset
                    .checked_add(value.data.len())
                    .ok_or(CodecError::InvalidData)?;
                values.extend(value.data);
            }
        }
        self.suffix.extend(self.endian.put(0, pointer_size));
        self.suffix.extend(values);
        if !self.big && start > u32::MAX as usize {
            return Err(CodecError::InvalidData);
        }
        self.pointer = start as u64;
        self.prefix[pointer_offset..pointer_offset + pointer_size]
            .copy_from_slice(&self.endian.put(self.pointer, pointer_size));
        Ok(self)
    }
}
impl Read for DirectoryView<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let Ok(mut offset) = usize::try_from(self.position) else {
            return Ok(0);
        };
        let mut count = 0;
        while count < buffer.len() {
            let source = if offset < self.prefix.len() {
                &self.prefix[offset..]
            } else if offset < self.data.len() {
                &self.data[offset..]
            } else if offset - self.data.len() < self.suffix.len() {
                &self.suffix[offset - self.data.len()..]
            } else {
                break;
            };
            let len = source.len().min(buffer.len() - count);
            buffer[count..count + len].copy_from_slice(&source[..len]);
            if offset >= self.prefix.len() && offset < self.data.len() {
                let first = self
                    .reverse_ranges
                    .partition_point(|range| range.end <= offset);
                for range in self.reverse_ranges[first..]
                    .iter()
                    .take_while(|range| range.start < offset + len)
                {
                    let start = offset.max(range.start);
                    let end = (offset + len).min(range.end);
                    if start < end {
                        for byte in &mut buffer[count + start - offset..count + end - offset] {
                            *byte = byte.reverse_bits();
                        }
                    }
                }
            }

            count += len;
            offset += len;
        }
        self.position += count as u64;
        Ok(count)
    }
}
impl Seek for DirectoryView<'_> {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        let position = match position {
            SeekFrom::Start(value) => i128::from(value),
            SeekFrom::End(value) => {
                (self.data.len() + self.suffix.len()) as i128 + i128::from(value)
            }
            SeekFrom::Current(value) => i128::from(self.position) + i128::from(value),
        };
        self.position = u64::try_from(position)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid TIFF seek"))?;
        Ok(self.position)
    }
}
