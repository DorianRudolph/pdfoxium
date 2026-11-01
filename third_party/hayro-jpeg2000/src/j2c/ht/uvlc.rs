// Pinned ht_dec.c::decode_init_uvlc / decode_noninit_uvlc.
// BSD-2-Clause notices: ../../../LICENSE-OPENJPEG-HT.

// Prefix length (2 bits), suffix length (3 bits), prefix value (3 bits).
const DEC: [u32; 8] = [
    3 | (5 << 2) | (5 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
    3 | (1 << 2) | (3 << 5),
    1 | (1 << 5),
    2 | (2 << 5),
    1 | (1 << 5),
];

pub(super) fn decode_init_uvlc(vlc: u32, mode: u32) -> (u32, [u32; 2]) {
    decode(vlc, mode, true)
}

pub(super) fn decode_noninit_uvlc(vlc: u32, mode: u32) -> (u32, [u32; 2]) {
    decode(vlc, mode, false)
}

fn decode(mut vlc: u32, mode: u32, initial: bool) -> (u32, [u32; 2]) {
    let mut consumed_bits = 0;
    let mut u = [1, 1];
    match mode {
        0 => {}
        1 | 2 => {
            let mut d = DEC[(vlc & 7) as usize];
            vlc >>= d & 3;
            consumed_bits += d & 3;
            let suffix_len = (d >> 2) & 7;
            consumed_bits += suffix_len;
            d = (d >> 5) + (vlc & ((1 << suffix_len) - 1));
            u = if mode == 1 { [d + 1, 1] } else { [1, d + 1] };
        }
        3 | 4 => {
            debug_assert!(mode != 4 || initial);
            let mut d1 = DEC[(vlc & 7) as usize];
            vlc >>= d1 & 3;
            consumed_bits += d1 & 3;
            if initial && mode == 3 && d1 & 3 > 2 {
                u[1] = (vlc & 1) + 2;
                consumed_bits += 1;
                vlc >>= 1;
                let suffix_len = (d1 >> 2) & 7;
                consumed_bits += suffix_len;
                d1 = (d1 >> 5) + (vlc & ((1 << suffix_len) - 1));
                u[0] = d1 + 1;
            } else {
                let mut d2 = DEC[(vlc & 7) as usize];
                vlc >>= d2 & 3;
                consumed_bits += d2 & 3;
                let suffix_len = (d1 >> 2) & 7;
                consumed_bits += suffix_len;
                d1 = (d1 >> 5) + (vlc & ((1 << suffix_len) - 1));
                let offset = if mode == 4 { 3 } else { 1 };
                u[0] = d1 + offset;
                vlc >>= suffix_len;
                let suffix_len = (d2 >> 2) & 7;
                consumed_bits += suffix_len;
                d2 = (d2 >> 5) + (vlc & ((1 << suffix_len) - 1));
                u[1] = d2 + offset;
            }
        }
        _ => unreachable!("quad-pair U-offset bits produce modes 0..=4"),
    }
    (consumed_bits, u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_uvlc_initial_pair_escape_differs_from_later_pairs() {
        for (mode, expected) in [
            (0, (0, [1, 1])),
            (1, (8, [6, 1])),
            (2, (8, [1, 6])),
            (3, (9, [6, 2])),
            (4, (16, [8, 8])),
        ] {
            assert_eq!(decode_init_uvlc(0, mode), expected);
        }
        assert_eq!(decode_noninit_uvlc(0, 3), (16, [6, 6]));
        assert_eq!(decode_init_uvlc(u32::MAX, 3), (2, [2, 2]));
        assert_eq!(decode_init_uvlc(u32::MAX, 4), (2, [4, 4]));
    }
}
