// Pinned ht_dec.c::opj_t1_ht_decode_cblk. BSD-2-Clause notices are retained in
// ../../../LICENSE-OPENJPEG-HT. Pointer positions become contiguous indices;
// repeated four-sample/row bodies use loops with the original update order.

use super::luts::{VLC_TBL0, VLC_TBL1};
use super::reader::{Forward, Mel, Reverse};
use super::uvlc::{decode_init_uvlc, decode_noninit_uvlc};
use alloc::vec::Vec;

pub(crate) struct Block {
    pub(crate) width: usize,
    pub(crate) height: usize,
    pub(crate) mb: u32,
    pub(crate) numbps: u32,
    pub(crate) num_passes: u32,
    pub(crate) has_chunks: bool,
    pub(crate) lengths1: usize,
    pub(crate) lengths2: usize,
    pub(crate) roi_shift: u8,
    pub(crate) stripe_causal: bool,
}

pub(crate) struct Context {
    pub(crate) coefficients: Vec<u32>,
    sigma: [[u32; 132]; 2],
    mbr: [[u32; 132]; 2],
    line_state: [u8; 528],
}

impl Default for Context {
    fn default() -> Self {
        Self {
            coefficients: Vec::new(),
            sigma: [[0; 132]; 2],
            mbr: [[0; 132]; 2],
            line_state: [0; 528],
        }
    }
}

impl Context {
    pub(crate) fn decode(&mut self, data: &[u8], block: &Block) -> Option<()> {
        let Block {
            width,
            height,
            mb,
            numbps,
            num_passes,
            has_chunks,
            lengths1,
            lengths2,
            roi_shift,
            stripe_causal,
        } = *block;
        if roi_shift != 0 || width > 1024 || height > 1024 || width.checked_mul(height)? > 4096 {
            return None;
        }
        self.coefficients.clear();
        self.coefficients.try_reserve_exact(width * height).ok()?;
        self.coefficients.resize(width * height, 0);
        self.sigma = [[0; 132]; 2];
        self.mbr = [[0; 132]; 2];
        self.line_state.fill(0);
        // Native allocates/zeros first, then returns for Mb=0 or no chunks.
        if mb == 0 || !has_chunks {
            return Some(());
        }
        let zero_bplanes = mb.wrapping_add(1).wrapping_sub(numbps);
        let mut num_passes = num_passes;
        if num_passes > 1 && lengths2 == 0 {
            num_passes = 1;
        }
        if num_passes > 3 || mb > 30 || zero_bplanes > mb {
            return None;
        }
        if zero_bplanes == mb && num_passes > 1 {
            num_passes = 1;
        }
        let p = numbps;
        let zero_bplanes_p1 = zero_bplanes + 1;
        if lengths1 < 2 || lengths1 > data.len() || lengths1.checked_add(lengths2)? > data.len() {
            return None;
        }
        let lcup = lengths1;
        let scup = (usize::from(data[lcup - 1]) << 4) + usize::from(data[lcup - 2] & 15);
        if scup < 2 || scup > lcup || scup > 4079 {
            return None;
        }
        let mut mel = Mel::new(data, lcup, scup)?;
        let mut vlc = Reverse::vlc(data, lcup, scup)?;
        let mut magsgn = Forward::new(data, lcup - scup, 255)?;
        let mut sigprop = if num_passes > 1 {
            Some(Forward::new(&data[lengths1..], lengths2, 0)?)
        } else {
            None
        };
        let mut magref = if num_passes > 2 {
            Some(Reverse::mrp(data, lengths1, lengths2)?)
        } else {
            None
        };
        let mut run = mel.get_run();
        let mut sip_shift = 0;
        let mut y = 0;
        while y < height {
            let initial = y == 0;
            if !initial {
                sip_shift ^= 2;
                sip_shift &= !16;
            }
            let stripe = usize::from(y & 4 != 0);
            let mut sip = 0;
            let mut lsp = 0;
            let mut ls0 = self.line_state[0];
            self.line_state[0] = 0;
            let mut c_q = 0u32;
            for x in (0..width).step_by(4) {
                if !initial {
                    c_q |= u32::from(ls0 >> 7);
                    c_q |= u32::from((self.line_state[lsp + 1] >> 5) & 4);
                }
                let table = if initial { &VLC_TBL0 } else { &VLC_TBL1 };
                let mut vlc_val = vlc.fetch();
                let mut qinf = [0u32; 2];
                qinf[0] = u32::from(table[((c_q << 7) | (vlc_val & 127)) as usize]);
                if c_q == 0 {
                    run -= 2;
                    if run != -1 {
                        qinf[0] = 0;
                    }
                    if run < 0 {
                        run = mel.get_run();
                    }
                }
                c_q = if initial {
                    ((qinf[0] & 16) >> 4) | ((qinf[0] & 224) >> 5)
                } else {
                    ((qinf[0] & 64) >> 5) | ((qinf[0] & 128) >> 6)
                };
                vlc_val = vlc.advance(qinf[0] & 7);
                self.sigma[stripe][sip] |=
                    (((qinf[0] & 48) >> 4) | ((qinf[0] & 192) >> 2)) << sip_shift;
                if x + 2 < width {
                    if !initial {
                        c_q |= u32::from(self.line_state[lsp + 1] >> 7);
                        c_q |= u32::from((self.line_state[lsp + 2] >> 5) & 4);
                    }
                    qinf[1] = u32::from(table[((c_q << 7) | (vlc_val & 127)) as usize]);
                    if c_q == 0 {
                        run -= 2;
                        if run != -1 {
                            qinf[1] = 0;
                        }
                        if run < 0 {
                            run = mel.get_run();
                        }
                    }
                    c_q = if initial {
                        ((qinf[1] & 16) >> 4) | ((qinf[1] & 224) >> 5)
                    } else {
                        ((qinf[1] & 64) >> 5) | ((qinf[1] & 128) >> 6)
                    };
                    vlc_val = vlc.advance(qinf[1] & 7);
                }
                self.sigma[stripe][sip] |=
                    ((qinf[1] & 48) | ((qinf[1] & 192) << 2)) << (4 + sip_shift);
                sip += usize::from(x & 7 != 0);
                sip_shift ^= 16;
                let mut uvlc_mode = ((qinf[0] & 8) >> 3) | ((qinf[1] & 8) >> 2);
                if initial && uvlc_mode == 3 {
                    run -= 2;
                    uvlc_mode += u32::from(run == -1);
                    if run < 0 {
                        run = mel.get_run();
                    }
                }
                let (consumed, mut u_q) = if initial {
                    decode_init_uvlc(vlc_val, uvlc_mode)
                } else {
                    decode_noninit_uvlc(vlc_val, uvlc_mode)
                };
                vlc.advance(consumed);
                if !initial {
                    for quad in 0..2 {
                        let significance = qinf[quad] & 240;
                        if significance & significance.wrapping_sub(1) != 0 {
                            let left = if quad == 0 {
                                ls0
                            } else {
                                self.line_state[lsp + 1]
                            };
                            let right = self.line_state[lsp + quad + 1];
                            let e = u32::from((left & 127).max(right & 127));
                            u_q[quad] += e.saturating_sub(2);
                        }
                    }
                }
                if u_q[0] > zero_bplanes_p1 || u_q[1] > zero_bplanes_p1 {
                    return None;
                }
                if !initial {
                    ls0 = self.line_state[lsp + 2];
                    self.line_state[lsp + 1] = 0;
                    self.line_state[lsp + 2] = 0;
                }
                let mut locs = 255u32;
                if x + 4 > width {
                    locs >>= (x + 4 - width) * 2;
                }
                if y + 2 > height {
                    locs &= 85;
                }
                if (((qinf[0] & 240) >> 4) | (qinf[1] & 240)) & !locs != 0 {
                    return None;
                }
                // Each quad scans its first column top/bottom, then its second
                // column top/bottom. Line-state updates stay between columns.
                for quad in 0..2 {
                    for column in 0..2 {
                        if column == 1 {
                            lsp += 1;
                            if initial {
                                self.line_state[lsp] = 0;
                            }
                        }
                        for row in 0..2 {
                            let sample = column * 2 + row;
                            let location = 1 << (quad * 4 + sample);
                            let significant = qinf[quad] & (16 << sample) != 0;
                            if significant {
                                let ms_val = magsgn.fetch();
                                let m_n = u_q[quad] - ((qinf[quad] >> (12 + sample)) & 1);
                                magsgn.advance(m_n);
                                let sign = ms_val << 31;
                                let mut v_n = ms_val & ((1u32 << m_n) - 1);
                                v_n |= ((qinf[quad] >> (8 + sample)) & 1) << m_n;
                                v_n |= 1;
                                let index = (y + row) * width + x + quad * 2 + column;
                                *self.coefficients.get_mut(index)? =
                                    sign | v_n.wrapping_add(2).wrapping_shl(p - 1);
                                if row == 1 {
                                    let exponent = (32 - v_n.leading_zeros()) as u8;
                                    self.line_state[lsp] = 128
                                        | if column == 0 {
                                            (self.line_state[lsp] & 127).max(exponent)
                                        } else {
                                            exponent
                                        };
                                }
                            } else if locs & location != 0 {
                                let index = (y + row) * width + x + quad * 2 + column;
                                *self.coefficients.get_mut(index)? = 0;
                            }
                        }
                    }
                }
            }
            y += 2;
            if num_passes > 1 && y & 3 == 0 {
                let current = usize::from(y & 4 == 0);
                if let Some(reader) = &mut magref {
                    self.refine(reader, current, y - 4, width, p)?;
                }
                self.membership(current, width);
                if y >= 8 {
                    let previous = 1 - current;
                    self.membership_from_next(previous, current, width, stripe_causal);
                    self.propagate(
                        sigprop.as_mut()?,
                        previous,
                        current,
                        y - 8,
                        width,
                        p,
                        u32::MAX,
                    )?;
                    self.sigma[previous][..width.div_ceil(8) + 1].fill(0);
                }
            }
        }
        if num_passes > 1 {
            if matches!(height & 3, 1 | 2) {
                let current = usize::from(height & 4 != 0);
                if let Some(reader) = &mut magref {
                    self.refine(reader, current, height & !3, width, p)?;
                }
                self.membership(current, width);
            }
            let start = height
                - if height > 6 {
                    ((height + 1) & 3) + 3
                } else {
                    height
                };
            for y in (start..height).step_by(4) {
                let current = usize::from(y & 4 != 0);
                let next = 1 - current;
                let pattern = match height - y {
                    1 => 0x11111111,
                    2 => 0x33333333,
                    3 => 0x77777777,
                    _ => u32::MAX,
                };
                if height - y > 4 {
                    self.membership_from_next(current, next, width, stripe_causal);
                }
                self.propagate(sigprop.as_mut()?, current, next, y, width, p, pattern)?;
            }
        }
        // Sign/magnitude is kept as u32 in the safe buffer. The caller applies
        // native final signed conversion before /2 or dequantization.
        Some(())
    }

    fn refine(
        &mut self,
        reader: &mut Reverse<'_>,
        stripe: usize,
        y: usize,
        width: usize,
        p: u32,
    ) -> Option<()> {
        let half = 1u32 << (p - 2);
        for x in (0..width).step_by(8) {
            let mut cwd = reader.fetch();
            let sig = self.sigma[stripe][x / 8];
            for column in 0..8 {
                for row in 0..4 {
                    if sig & (1 << (column * 4 + row)) != 0 {
                        let sample = self.coefficients.get_mut((y + row) * width + x + column)?;
                        debug_assert!(*sample != 0);
                        *sample ^= (1 - (cwd & 1)) << (p - 1);
                        *sample |= half;
                        cwd >>= 1;
                    }
                }
            }
            reader.advance(sig.count_ones());
        }
        Some(())
    }

    fn membership(&mut self, stripe: usize, width: usize) {
        let mut previous = 0;
        for slot in 0..width.div_ceil(8) {
            let sig = self.sigma[stripe][slot];
            let mut t = sig
                | (previous >> 28)
                | (sig << 4)
                | (sig >> 4)
                | (self.sigma[stripe][slot + 1] << 28);
            previous = sig;
            t |= ((t & 0x77777777) << 1) | ((t & 0xeeeeeeee) >> 1);
            self.mbr[stripe][slot] = t & !sig;
        }
    }

    fn membership_from_next(
        &mut self,
        current: usize,
        next: usize,
        width: usize,
        stripe_causal: bool,
    ) {
        let mut previous = 0;
        for slot in 0..width.div_ceil(8) {
            let sig = self.sigma[next][slot];
            let t = sig
                | (previous >> 28)
                | (sig << 4)
                | (sig >> 4)
                | (self.sigma[next][slot + 1] << 28);
            previous = sig;
            if !stripe_causal {
                self.mbr[current][slot] |= (t & 0x11111111) << 3;
            }
            self.mbr[current][slot] &= !self.sigma[current][slot];
        }
    }

    fn propagate(
        &mut self,
        reader: &mut Forward<'_>,
        current: usize,
        next: usize,
        y: usize,
        width: usize,
        p: u32,
        pattern: u32,
    ) -> Option<()> {
        let val = 3u32 << (p - 2);
        for x in (0..width).step_by(8) {
            let slot = x / 8;
            let mut mbr = self.mbr[current][slot] & pattern;
            let mut new_sig = 0u32;
            if mbr != 0 {
                for n in [0, 4] {
                    let mut cwd = reader.fetch();
                    let mut count = 0;
                    let inv_sig = !self.sigma[current][slot] & pattern;
                    let end = (n + 4).min(width - x);
                    for column in n..end {
                        let column_mask = 15u32 << (4 * column);
                        if column_mask & mbr == 0 {
                            continue;
                        }
                        for (row, propagation) in
                            [0x32u32, 0x74, 0xe8, 0xc0].into_iter().enumerate()
                        {
                            let sample_mask = 1u32 << (4 * column + row);
                            if mbr & sample_mask == 0 {
                                continue;
                            }
                            debug_assert!(
                                *self.coefficients.get((y + row) * width + x + column)? == 0
                            );
                            if cwd & 1 != 0 {
                                new_sig |= sample_mask;
                                mbr |= (propagation << (column * 4)) & inv_sig;
                            }
                            cwd >>= 1;
                            count += 1;
                        }
                    }
                    if new_sig & (0xffffu32 << (4 * n)) != 0 {
                        for column in n..end {
                            for row in 0..4 {
                                if new_sig & (1 << (4 * column + row)) == 0 {
                                    continue;
                                }
                                let sample =
                                    self.coefficients.get_mut((y + row) * width + x + column)?;
                                debug_assert!(*sample == 0);
                                *sample |= ((cwd & 1) << 31) | val;
                                cwd >>= 1;
                                count += 1;
                            }
                        }
                    }
                    reader.advance(count);
                    if n == 4 {
                        let mut t = new_sig >> 28;
                        t |= ((t & 14) >> 1) | ((t & 7) << 1);
                        self.mbr[current][slot + 1] |= t & !self.sigma[current][slot + 1];
                    }
                }
            }
            new_sig |= self.sigma[current][slot];
            let ux = (new_sig & 0x88888888) >> 3;
            let tx = ux | (ux << 4) | (ux >> 4);
            if slot > 0 {
                self.mbr[next][slot - 1] |= (ux << 28) & !self.sigma[next][slot - 1];
            }
            self.mbr[next][slot] |= tx & !self.sigma[next][slot];
            self.mbr[next][slot + 1] |= (ux >> 28) & !self.sigma[next][slot + 1];
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(4))]
    struct Input([u8; 4]);

    #[test]
    fn native_ht_cleanup_and_refinement_keep_stripe_and_partial_edge_order() {
        // Fixed coefficient checksums from pinned ht_dec.c for this tiny block
        // stream, including SPP/MRP, full stripes and partial final rows/columns.
        let cases = [
            (2, 1, 8, [0xac8a934d, 0xac8a934d, 0x6c8a2e8d]),
            (4, 8, 2, [0x93f237a7, 0x936c87a2, 0x80b2b5bd]),
            (8, 10, 2, [0x7890f847, 0x31843f2e, 0x7ec4db81]),
            (9, 9, 2, [0xdee3514d, 0x48ad65e8, 0x49bf53b7]),
        ];
        let input = Input([0x22, 0, 0x5a, 0xa5]);
        let mut ctx = Context::default();
        for (width, height, numbps, expected) in cases {
            for (index, expected) in expected.into_iter().enumerate() {
                let passes = index as u32 + 1;
                let block = Block {
                    width,
                    height,
                    mb: 8,
                    numbps,
                    num_passes: passes,
                    has_chunks: true,
                    lengths1: 2,
                    lengths2: if passes > 1 { 2 } else { 0 },
                    roi_shift: 0,
                    stripe_causal: false,
                };
                ctx.decode(&input.0[..if passes > 1 { 4 } else { 2 }], &block)
                    .unwrap();
                let hash = ctx.coefficients.iter().fold(2166136261u32, |hash, &word| {
                    let magnitude = word & 0x7fffffff;
                    let signed = if word & 0x80000000 != 0 {
                        magnitude.wrapping_neg()
                    } else {
                        magnitude
                    };
                    (hash ^ signed).wrapping_mul(16777619)
                });
                assert_eq!(hash, expected, "{width}x{height}, passes {passes}");
            }
        }
    }

    #[test]
    fn native_ht_validation_keeps_empty_and_cleanup_only_branches() {
        let input = Input([0x22, 0, 0x5a, 0xa5]);
        let mut ctx = Context::default();
        let mut block = Block {
            width: 2,
            height: 1,
            mb: 8,
            numbps: 1,
            num_passes: 3,
            has_chunks: true,
            lengths1: 2,
            lengths2: 2,
            roi_shift: 0,
            stripe_causal: false,
        };
        // All but the final bitplane missing: native only decodes cleanup.
        ctx.decode(&input.0, &block).unwrap();
        assert_eq!(ctx.coefficients, [0, 0x80000003]);
        block.numbps = 0;
        assert!(ctx.decode(&input.0, &block).is_none());
        block.numbps = 8;
        block.mb = 31;
        assert!(ctx.decode(&input.0, &block).is_none());
        block.has_chunks = false;
        assert!(ctx.decode(&[], &block).is_some());
        block.roi_shift = 1; // ROI fails even for an empty codeblock.
        assert!(ctx.decode(&[], &block).is_none());
        block.roi_shift = 0;
        block.has_chunks = true;
        block.mb = 8;
        assert!(ctx.decode(&[], &block).is_none());
        assert!(ctx.decode(&[0x22, 255, 0x5a, 0xa5], &block).is_none());
    }
}
