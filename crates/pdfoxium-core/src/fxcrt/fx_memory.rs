// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2022 The PDFium Authors
// Use of this source code is governed by the BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Recoverable general-data allocation from fx_memory_malloc.cpp. Typed Rust
//! ownership replaces raw allocator callbacks; bitmap allocation keeps the
//! native checked product, size limit, zero initialization and failure result.

pub const MALLOC_SIZE_LIMIT: usize = i32::MAX as usize - (1 << 12);

pub fn checked_allocation_size(count: usize, member_size: usize) -> Option<usize> {
    count
        .checked_mul(member_size)
        .filter(|&size| size < MALLOC_SIZE_LIMIT)
}

pub fn try_alloc_zeroed(size: usize) -> Option<Vec<u8>> {
    let size = checked_allocation_size(size, 1)?;
    let mut data = Vec::new();
    data.try_reserve_exact(size).ok()?;
    data.resize(size, 0);
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_general_allocation_boundaries_do_not_allocate() {
        assert_eq!(
            checked_allocation_size(MALLOC_SIZE_LIMIT - 1, 1),
            Some(MALLOC_SIZE_LIMIT - 1)
        );
        assert_eq!(checked_allocation_size(MALLOC_SIZE_LIMIT, 1), None);
        assert_eq!(checked_allocation_size(usize::MAX, 2), None);
        assert_eq!(checked_allocation_size(0, usize::MAX), Some(0));
    }
}

#[cfg(test)]
#[path = "fx_memory_unittest.rs"]
mod upstream_tests;
