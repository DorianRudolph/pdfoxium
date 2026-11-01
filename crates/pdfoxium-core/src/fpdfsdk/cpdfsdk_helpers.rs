// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Selected `fpdfsdk/cpdfsdk_helpers.cpp` page-import and string helpers. Other SDK
//! helper branches remain tracked in the source inventory.

/// Copy the complete byte string and its terminator only when the buffer fits.
/// Return the required byte count even for a null/short public output buffer.
pub fn nul_terminate_maybe_copy_and_return_length(text: &[u8], output: &mut [u8]) -> usize {
    let length = text
        .len()
        .checked_add(1)
        .expect("terminated string length overflow");
    if output.len() >= length {
        output[..text.len()].copy_from_slice(text);
        output[text.len()] = 0;
    }
    length
}

pub fn parse_page_range_string(page_range: &[u8], count: u32) -> Vec<u32> {
    if !page_range.iter().all(|ch| b" 0123456789-,".contains(ch)) {
        return Vec::new();
    }
    let stripped: Vec<u8> = page_range
        .iter()
        .copied()
        .filter(|&ch| ch != b' ')
        .collect();
    // Inputs here contain digits only. Native atoi overflow has undefined
    // behavior; make that unrepresentable int a checked panic in safe Rust.
    let atoi = |digits: &[u8]| -> u32 {
        digits.iter().fold(0i32, |value, ch| {
            value
                .checked_mul(10)
                .and_then(|value| value.checked_add(i32::from(ch - b'0')))
                .expect("native atoi page number overflow")
        }) as u32
    };
    let mut result = Vec::new();
    for entry in stripped.split(|&ch| ch == b',') {
        let args: Vec<_> = entry.split(|&ch| ch == b'-').collect();
        match args.as_slice() {
            [single] => {
                let page = atoi(single);
                if page == 0 || page > count {
                    return Vec::new();
                }
                result.push(page - 1);
            }
            [first, last] => {
                let first = atoi(first);
                if first == 0 {
                    return Vec::new();
                }
                let last = atoi(last);
                if last == 0 || first > last || last > count {
                    return Vec::new();
                }
                result.extend((first..=last).map(|page| page - 1));
            }
            _ => return Vec::new(),
        }
    }
    result
}

#[cfg(test)]
#[path = "cpdfsdk_helpers_unittest.rs"]
mod upstream_tests;
