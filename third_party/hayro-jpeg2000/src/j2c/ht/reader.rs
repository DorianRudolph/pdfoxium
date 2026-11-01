// Pinned ht_dec.c MEL, reverse VLC/MRP and forward MagSgn/SPP readers.
// BSD-2-Clause notices: ../../../LICENSE-OPENJPEG-HT.

pub(super) struct Mel<'a> {
    data: &'a [u8],
    position: usize,
    tmp: u64,
    bits: u32,
    size: i32,
    unstuff: bool,
    k: usize,
    num_runs: u32,
    runs: u64,
}

impl<'a> Mel<'a> {
    pub(super) fn new(data: &'a [u8], lcup: usize, scup: usize) -> Option<Self> {
        if scup < 2 || scup > lcup || lcup > data.len() {
            return None;
        }
        let position = lcup.checked_sub(scup)?;
        let mut mel = Self {
            data,
            position,
            tmp: 0,
            bits: 0,
            size: scup as i32 - 1,
            unstuff: false,
            k: 0,
            num_runs: 0,
            runs: 0,
        };
        // Native batches initial bytes to align its next four-byte read. Only
        // the address's low bits are inspected; all memory reads use slices.
        let num = 4 - ((data.as_ptr() as usize + position) & 3);
        for _ in 0..num {
            if mel.unstuff && *data.get(mel.position)? > 0x8f {
                return None;
            }
            let mut d = if mel.size > 0 {
                u64::from(*data.get(mel.position)?)
            } else {
                255
            };
            if mel.size == 1 {
                d |= 15;
            }
            if mel.size > 0 {
                mel.position += 1;
            }
            mel.size -= 1;
            let bits = 8 - u32::from(mel.unstuff);
            mel.tmp = (mel.tmp << bits) | d;
            mel.bits += bits;
            mel.unstuff = d & 255 == 255;
        }
        mel.tmp <<= 64 - mel.bits;
        Some(mel)
    }

    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }
        let mut val = u32::MAX;
        if self.size > 4 {
            val = u32::from_le_bytes(
                self.data[self.position..self.position + 4]
                    .try_into()
                    .unwrap(),
            );
            self.position += 4;
            self.size -= 4;
        } else if self.size > 0 {
            let mut shift = 0;
            while self.size > 1 {
                let byte = u32::from(self.data[self.position]);
                self.position += 1;
                val = (val & !(255 << shift)) | (byte << shift);
                self.size -= 1;
                shift += 8;
            }
            let byte = u32::from(self.data[self.position]) | 15;
            self.position += 1;
            val = (val & !(255 << shift)) | (byte << shift);
            self.size -= 1;
        }
        let mut bits = 32 - u32::from(self.unstuff);
        let mut t = val & 255;
        let mut unstuff = t == 255;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);
        t |= (val >> 8) & 255;
        unstuff = (val >> 8) & 255 == 255;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);
        t |= (val >> 16) & 255;
        unstuff = (val >> 16) & 255 == 255;
        bits -= u32::from(unstuff);
        t <<= 8 - u32::from(unstuff);
        t |= val >> 24;
        self.unstuff = val >> 24 == 255;
        self.tmp |= u64::from(t) << (64 - bits - self.bits);
        self.bits += bits;
    }

    fn decode(&mut self) {
        const EXP: [u32; 13] = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 4, 5];
        if self.bits < 6 {
            self.read();
        }
        while self.bits >= 6 && self.num_runs < 8 {
            let exponent = EXP[self.k];
            let run;
            if self.tmp & (1 << 63) != 0 {
                run = ((1u64 << exponent) - 1) << 1;
                self.k = (self.k + 1).min(12);
                self.tmp <<= 1;
                self.bits -= 1;
            } else {
                run = (((self.tmp >> (63 - exponent)) & ((1 << exponent) - 1)) << 1) + 1;
                self.k = self.k.saturating_sub(1);
                self.tmp <<= exponent + 1;
                self.bits -= exponent + 1;
            }
            let shift = self.num_runs * 7;
            self.runs &= !(0x3fu64 << shift);
            self.runs |= run << shift;
            self.num_runs += 1;
        }
    }

    pub(super) fn get_run(&mut self) -> i32 {
        if self.num_runs == 0 {
            self.decode();
        }
        let run = (self.runs & 127) as i32;
        self.runs >>= 7;
        self.num_runs -= 1;
        run
    }
}

pub(super) struct Reverse<'a> {
    data: &'a [u8],
    position: isize,
    tmp: u64,
    bits: u32,
    size: i32,
    unstuff: bool,
}

impl<'a> Reverse<'a> {
    pub(super) fn vlc(data: &'a [u8], lcup: usize, scup: usize) -> Option<Self> {
        if scup < 2 || scup > lcup || lcup > data.len() {
            return None;
        }
        let position = lcup.checked_sub(2)?;
        let d = u64::from(*data.get(position)?);
        let mut reader = Self {
            data,
            position: position as isize - 1,
            tmp: d >> 4,
            bits: 4 - u32::from((d >> 4) & 7 == 7),
            size: scup as i32 - 2,
            unstuff: d | 15 > 0x8f,
        };
        let num = 1 + ((data.as_ptr() as usize).wrapping_add_signed(reader.position) & 3);
        let count = (num as i32).min(reader.size);
        for _ in 0..count {
            let d = u64::from(*data.get(reader.position as usize)?);
            reader.position -= 1;
            reader.tmp |= d << reader.bits;
            reader.bits += 8 - u32::from(reader.unstuff && d & 127 == 127);
            reader.unstuff = d > 0x8f;
        }
        reader.size -= count;
        reader.read();
        Some(reader)
    }

    pub(super) fn mrp(data: &'a [u8], lcup: usize, len2: usize) -> Option<Self> {
        if lcup.checked_add(len2)? > data.len() {
            return None;
        }
        let position = lcup.checked_add(len2)?.checked_sub(1)?;
        let mut reader = Self {
            data,
            position: position as isize,
            tmp: 0,
            bits: 0,
            size: i32::try_from(len2).ok()?,
            unstuff: true,
        };
        let num = 1 + ((data.as_ptr() as usize + position) & 3);
        for _ in 0..num {
            let d = if reader.size > 0 {
                let byte = u64::from(*data.get(reader.position as usize)?);
                reader.position -= 1;
                byte
            } else {
                0
            };
            reader.size -= 1;
            reader.tmp |= d << reader.bits;
            reader.bits += 8 - u32::from(reader.unstuff && d & 127 == 127);
            reader.unstuff = d > 0x8f;
        }
        reader.read();
        Some(reader)
    }

    // Native VLC and MRP refill have identical operations (zero fill once
    // size <= 0); keep their common byte assembly and unstuffing together.
    fn read(&mut self) {
        if self.bits > 32 {
            return;
        }
        let mut val = 0;
        if self.size > 3 {
            let start = self.position as usize - 3;
            val = u32::from_le_bytes(self.data[start..start + 4].try_into().unwrap());
            self.position -= 4;
            self.size -= 4;
        } else {
            let mut shift = 24;
            while self.size > 0 {
                val |= u32::from(self.data[self.position as usize]) << shift;
                self.position -= 1;
                self.size -= 1;
                shift -= 8;
            }
        }
        let mut tmp = val >> 24;
        let mut bits = 8 - u32::from(self.unstuff && tmp & 127 == 127);
        let mut unstuff = tmp > 0x8f;
        for shift in [16, 8, 0] {
            let byte = (val >> shift) & 255;
            tmp |= byte << bits;
            bits += 8 - u32::from(unstuff && byte & 127 == 127);
            unstuff = byte > 0x8f;
        }
        self.tmp |= u64::from(tmp) << self.bits;
        self.bits += bits;
        self.unstuff = unstuff;
    }

    pub(super) fn fetch(&mut self) -> u32 {
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        self.tmp as u32
    }

    pub(super) fn advance(&mut self, bits: u32) -> u32 {
        debug_assert!(bits <= self.bits);
        self.tmp >>= bits;
        self.bits -= bits;
        self.tmp as u32
    }
}

pub(super) struct Forward<'a> {
    data: &'a [u8],
    position: usize,
    tmp: u64,
    bits: u32,
    unstuff: bool,
    size: i32,
    x: u32,
}

impl<'a> Forward<'a> {
    pub(super) fn new(data: &'a [u8], size: usize, x: u32) -> Option<Self> {
        if size > data.len() {
            return None;
        }
        debug_assert!(x == 0 || x == 255);
        let mut reader = Self {
            data,
            position: 0,
            tmp: 0,
            bits: 0,
            unstuff: false,
            size: i32::try_from(size).ok()?,
            x,
        };
        let num = 4 - ((data.as_ptr() as usize) & 3);
        for _ in 0..num {
            let d = if reader.size > 0 {
                let byte = data[reader.position];
                reader.position += 1;
                u64::from(byte)
            } else {
                u64::from(x)
            };
            reader.size -= 1;
            reader.tmp |= d << reader.bits;
            reader.bits += 8 - u32::from(reader.unstuff);
            reader.unstuff = d & 255 == 255;
        }
        reader.read();
        Some(reader)
    }

    fn read(&mut self) {
        debug_assert!(self.bits <= 32);
        let mut val = if self.x != 0 { u32::MAX } else { 0 };
        if self.size > 3 {
            val = u32::from_le_bytes(
                self.data[self.position..self.position + 4]
                    .try_into()
                    .unwrap(),
            );
            self.position += 4;
            self.size -= 4;
        } else {
            let mut shift = 0;
            while self.size > 0 {
                let byte = u32::from(self.data[self.position]);
                self.position += 1;
                val = (val & !(255 << shift)) | (byte << shift);
                self.size -= 1;
                shift += 8;
            }
        }
        let mut bits = 8 - u32::from(self.unstuff);
        let mut t = val & 255;
        let mut unstuff = t == 255;
        for shift in [8, 16, 24] {
            let byte = (val >> shift) & 255;
            t |= byte << bits;
            bits += 8 - u32::from(unstuff);
            unstuff = byte == 255;
        }
        self.unstuff = unstuff;
        self.tmp |= u64::from(t) << self.bits;
        self.bits += bits;
    }

    pub(super) fn fetch(&mut self) -> u32 {
        if self.bits < 32 {
            self.read();
            if self.bits < 32 {
                self.read();
            }
        }
        self.tmp as u32
    }

    pub(super) fn advance(&mut self, bits: u32) {
        debug_assert!(bits <= self.bits);
        self.tmp >>= bits;
        self.bits -= bits;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[repr(align(4))]
    struct Input([u8; 28]);

    #[test]
    fn native_ht_reader_unstuffing_and_exhausted_stream_values() {
        // Expected values from the pinned ht_dec.c readers with these bytes.
        let mut input = Input([
            0x11, 0x22, 0x33, 0x44, 0xff, 0x7f, 0x08, 0x90, 0x7f, 0x8f, 0, 0xaa, 0x55, 0xff, 0x7f,
            0x12, 0x34, 0x56, 0x72, 0, 0x7f, 0xff, 0x8f, 0x7f, 0x11, 0x22, 0x33, 0x44,
        ]);
        for (fill, expected) in [
            (
                0,
                [
                    0x44332211, 0x4023fffa, 0x4011eff2, 0x7fff55aa, 0xc958d04b, 0x0000000e, 0, 0,
                ],
            ),
            (
                255,
                [
                    0x44332211, 0x4023fffa, 0x4011eff2, 0x7fff55aa, 0xc958d04b, 0xffffe00e,
                    0xffffffff, 0xffffffff,
                ],
            ),
        ] {
            let mut reader = Forward::new(&input.0, 20, fill).unwrap();
            for value in expected {
                assert_eq!(reader.fetch(), value);
                reader.advance(29);
            }
        }
        let mut vlc = Reverse::vlc(&input.0, 20, 8).unwrap();
        for value in [0xf891a2b7, 0x7fdfc48d, 0x00abfefe, 0x0000055f, 0, 0, 0, 0] {
            assert_eq!(vlc.fetch(), value);
            vlc.advance(13);
        }
        let mut mrp = Reverse::mrp(&input.0, 20, 8).unwrap();
        for value in [
            0x11223344, 0x7bf88911, 0xffe3dfc4, 0x00ffff1e, 0x000007ff, 0, 0, 0,
        ] {
            assert_eq!(mrp.fetch(), value);
            mrp.advance(13);
        }
        let mut mel = Mel::new(&input.0, 20, 8).unwrap();
        for run in [
            1, 0, 1, 0, 1, 0, 1, 0, 0, 0, 2, 2, 2, 6, 6, 6, 14, 14, 30, 62,
        ] {
            assert_eq!(mel.get_run(), run);
        }
        input.0[14] = 0x90;
        assert!(Mel::new(&input.0, 20, 8).is_none());
    }
}
