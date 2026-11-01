pub mod fpdf_cmaps;

#[path = "data/cns1.rs"]
#[rustfmt::skip]
mod cns1;
#[path = "data/gb1.rs"]
#[rustfmt::skip]
mod gb1;
#[path = "data/japan1.rs"]
#[rustfmt::skip]
mod japan1;
#[path = "data/korea1.rs"]
#[rustfmt::skip]
mod korea1;
#[path = "data/unicode.rs"]
#[rustfmt::skip]
mod unicode;

use crate::fpdfapi::font::cpdf_cmapparser::CidSet;
use fpdf_cmaps::CMap;

pub fn maps_for_charset(charset: CidSet) -> &'static [CMap] {
    match charset {
        CidSet::GB1 => &gb1::MAPS,
        CidSet::CNS1 => &cns1::MAPS,
        CidSet::Japan1 => &japan1::MAPS,
        CidSet::Korea1 => &korea1::MAPS,
        CidSet::Unknown | CidSet::Unicode => &[],
    }
}

pub fn unicode_map_for_charset(charset: CidSet) -> &'static [u16] {
    match charset {
        CidSet::GB1 => &unicode::GB1,
        CidSet::CNS1 => &unicode::CNS1,
        CidSet::Japan1 => &unicode::JAPAN1,
        CidSet::Korea1 => &unicode::KOREA1,
        CidSet::Unknown | CidSet::Unicode => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fpdf_cmaps::{char_code_from_cid, cid_from_char_code, find_cmap};

    #[test]
    fn bundled_maps_match_pinned_ranges_and_inheritance() {
        let gb = maps_for_charset(CidSet::GB1);
        let gb_h = find_cmap(gb, b"GB-EUC-H").unwrap();
        let gb_v = find_cmap(gb, b"GB-EUC-V").unwrap();
        assert_eq!(cid_from_char_code(gb, gb_h, 0x20), 0x1e24);
        assert_eq!(cid_from_char_code(gb, gb_v, 0x20), 0x1e24);
        assert_eq!(char_code_from_cid(gb, gb_h, 0x1e24), 0x20);

        let cns = maps_for_charset(CidSet::CNS1);
        let cns_h = find_cmap(cns, b"CNS-EUC-H").unwrap();
        assert_eq!(cid_from_char_code(cns, cns_h, 0x8ea1_a1a2), 0x64);

        let japan = maps_for_charset(CidSet::Japan1);
        let japan_h = find_cmap(japan, b"H").unwrap();
        assert_eq!(cid_from_char_code(japan, japan_h, 0x2121), 0x279);
        assert!(!maps_for_charset(CidSet::Korea1).is_empty());
    }
}
