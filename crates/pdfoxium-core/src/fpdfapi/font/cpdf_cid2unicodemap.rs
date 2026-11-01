// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Translation of `core/fpdfapi/font/cpdf_cid2unicodemap.{h,cpp}`.

use super::cpdf_cmapparser::CidSet;
use crate::fpdfapi::cmaps::unicode_map_for_charset;

pub struct CpdfCid2UnicodeMap {
    charset: CidSet,
    embedded_map: &'static [u16],
}

impl CpdfCid2UnicodeMap {
    pub fn new(charset: CidSet) -> Self {
        Self {
            charset,
            embedded_map: unicode_map_for_charset(charset),
        }
    }

    pub fn is_loaded(&self) -> bool {
        !self.embedded_map.is_empty()
    }

    pub fn unicode_from_cid(&self, cid: u16) -> u32 {
        if self.charset == CidSet::Unicode {
            return u32::from(cid);
        }
        self.embedded_map
            .get(usize::from(cid))
            .copied()
            .map(u32::from)
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cid_unicode_tables_match_pinned_values_and_bounds() {
        for charset in [CidSet::GB1, CidSet::CNS1, CidSet::Japan1, CidSet::Korea1] {
            let map = CpdfCid2UnicodeMap::new(charset);
            assert!(map.is_loaded());
            assert_eq!(map.unicode_from_cid(0), 0xfffd);
            assert_eq!(map.unicode_from_cid(1), 0x20);
        }
        let unicode = CpdfCid2UnicodeMap::new(CidSet::Unicode);
        assert!(!unicode.is_loaded());
        assert_eq!(unicode.unicode_from_cid(0x1234), 0x1234);
        let unknown = CpdfCid2UnicodeMap::new(CidSet::Unknown);
        assert!(!unknown.is_loaded());
        assert_eq!(unknown.unicode_from_cid(1), 0);
    }
}
