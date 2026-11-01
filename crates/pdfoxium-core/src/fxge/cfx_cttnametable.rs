// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! TrueType family-name table from `core/fxge/cfx_cttnametable.{h,cpp}`.
//! Record name IDs use the specified record offset. Both encodings are bounded;
//! a valid string ending exactly at the table boundary is accepted.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CfxCttNameTable {
    family_names: Vec<Vec<u32>>,
}
fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        data.get(offset..offset.checked_add(2)?)?.try_into().ok()?,
    ))
}
impl CfxCttNameTable {
    pub fn new(data: &[u8]) -> Self {
        Self {
            family_names: parse_family_names(data),
        }
    }
    pub fn family_names(&self) -> &[Vec<u32>] {
        &self.family_names
    }
    pub fn add_family_name(&mut self, name: &[u32]) {
        self.family_names.push(name.to_vec());
    }
}
fn parse_family_names(data: &[u8]) -> Vec<Vec<u32>> {
    let mut names = Vec::new();
    let Some(count) = read_u16(data, 2) else {
        return names;
    };
    let Some(start) = read_u16(data, 4) else {
        return names;
    };
    for i in 0..usize::from(count) {
        let record = 6 + i * 12;
        if read_u16(data, record + 6) != Some(1) {
            continue;
        }
        let Some(platform) = read_u16(data, record) else {
            break;
        };
        let Some(length) = read_u16(data, record + 8) else {
            break;
        };
        let Some(offset) = read_u16(data, record + 10) else {
            break;
        };
        let begin = usize::from(start) + usize::from(offset);
        let Some(value) = data.get(begin..begin + usize::from(length)) else {
            continue;
        };
        names.push(if platform == 1 {
            value.iter().copied().map(u32::from).collect()
        } else {
            value
                .chunks_exact(2)
                .map(|pair| u32::from(u16::from_be_bytes([pair[0], pair[1]])))
                .collect()
        });
    }
    names
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn family_encodings_records_exact_boundaries_and_owned_additions() {
        let mut bytes = vec![0, 0, 0, 3, 0, 42];
        for record in [
            [0u16, 0, 0, 1, 4, 0],
            [1, 0, 0, 1, 2, 4],
            [3, 1, 0, 4, 2, 6],
        ] {
            for value in record {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        bytes.extend_from_slice(&[0, 65, 0, 66, 0x43, 0xe9, 0, 90]);
        let mut table = CfxCttNameTable::new(&bytes);
        assert_eq!(table.family_names(), &[vec![65, 66], vec![67, 233]]);
        table.add_family_name(&[0x4e2d]);
        assert_eq!(table.family_names()[2], [0x4e2d]);
        let mut exact = bytes.clone();
        exact.truncate(48); // Macintosh string ends at the table boundary.
        assert_eq!(
            CfxCttNameTable::new(&exact).family_names(),
            &[vec![65, 66], vec![67, 233]]
        );
        for end in 0..18 {
            assert!(CfxCttNameTable::new(&bytes[..end])
                .family_names()
                .is_empty());
        }
        bytes[4..6].copy_from_slice(&u16::MAX.to_be_bytes());
        assert!(CfxCttNameTable::new(&bytes).family_names().is_empty());
        assert!(CfxCttNameTable::default().family_names().is_empty());
    }
}
