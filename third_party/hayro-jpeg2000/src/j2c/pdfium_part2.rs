// Pinned j2k.c::read_mct/read_mcc/read_mco/add_mct/read_cbd and element readers.
// BSD-2-Clause notices: ../../LICENSE-OPENJPEG-J2K. Stable vector indices replace
// MCT record pointers, including their retargeting after native reallocations.

use super::codestream::ComponentSizeInfo;
use crate::reader::BitReader;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
struct Mct<'a> {
    index: u8,
    element_type: u8,
    data: Option<&'a [u8]>,
}

#[derive(Clone, Copy, Debug, Default)]
struct Mcc {
    index: u8,
    components: u16,
    irreversible: bool,
    decorrelation: Option<usize>,
    offset: Option<usize>,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct State<'a> {
    pub(crate) dc_offsets: Vec<i32>,
    mct: Vec<Mct<'a>>,
    mcc: Vec<Mcc>,
    // COD accepts only MCT 0/1 on decoding. add_mct still allocates/converts
    // this matrix, but the custom MCT=2 path is reached only by encoder setup.
    decoding_matrix: Option<Vec<f32>>,
}

impl<'a> State<'a> {
    pub(crate) fn new(components: &[ComponentSizeInfo]) -> Self {
        Self {
            dc_offsets: components
                .iter()
                .map(|comp| {
                    if comp.is_signed {
                        0
                    } else {
                        1i32 << (comp.precision - 1)
                    }
                })
                .collect(),
            ..Self::default()
        }
    }

    pub(crate) fn read(&mut self, marker: u8, reader: &mut BitReader<'a>) -> Option<()> {
        let length = reader.read_u16()?.checked_sub(2)?;
        let payload = reader.read_bytes(usize::from(length))?;
        let mut reader = BitReader::new(payload);
        match marker {
            0x74 => self.read_mct(&mut reader),
            0x75 => self.read_mcc(&mut reader),
            0x77 => self.read_mco(&mut reader),
            _ => unreachable!("Part 2 dispatch accepts MCT/MCC/MCO"),
        }
    }

    fn read_mct(&mut self, reader: &mut BitReader<'a>) -> Option<()> {
        let size = reader.tail()?.len();
        if reader.read_u16()? != 0 {
            return Some(());
        } // Unsupported Zmct.
        if size <= 6 {
            return None;
        }
        let imct = reader.read_u16()?;
        let index = imct as u8;
        let existing = self.mct.iter().position(|record| record.index == index);
        let record = Mct {
            index,
            // Imct's array type (bits 8..9) is encoder metadata. Like native,
            // decoding selects matrix/offset roles through MCC references.
            element_type: ((imct >> 10) & 3) as u8,
            data: None,
        };
        if let Some(existing) = existing {
            self.mct[existing] = record.clone();
        }
        // An unsupported Ymct clears a prior record's data and updates its
        // metadata, but does not register a new record in native's count.
        if reader.read_u16()? != 0 {
            return Some(());
        }
        let mut record = record;
        record.data = Some(reader.tail()?);
        if let Some(existing) = existing {
            self.mct[existing] = record;
        } else {
            self.mct.try_reserve(1).ok()?;
            self.mct.push(record);
        }
        Some(())
    }

    fn read_mcc(&mut self, reader: &mut BitReader<'_>) -> Option<()> {
        let size = reader.tail()?.len();
        if reader.read_u16()? != 0 {
            return Some(());
        }
        if size < 7 {
            return None;
        }
        let index = reader.read_byte()?;
        let existing = self.mcc.iter().position(|record| record.index == index);
        let mut record = existing.map_or_else(Mcc::default, |index| self.mcc[index]);
        record.index = index;
        let result = (|| {
            if reader.read_u16()? != 0 {
                return Some(false);
            }
            let collections = reader.read_u16()?;
            if collections > 1 {
                return Some(false);
            }
            for _ in 0..collections {
                reader.peek_bytes(3)?;
                if reader.read_byte()? != 1 {
                    return Some(false);
                }
                let count = reader.read_u16()?;
                let bytes = 1 + usize::from(count >> 15);
                record.components = count & 0x7fff;
                // Source validates room for the list and following count
                // before a shuffled index can take its successful skip path.
                reader.peek_bytes(bytes * usize::from(record.components) + 2)?;
                for component in 0..record.components {
                    let index = if bytes == 1 {
                        u16::from(reader.read_byte()?)
                    } else {
                        reader.read_u16()?
                    };
                    if index != component {
                        return Some(false);
                    }
                }
                let count = reader.read_u16()?;
                let bytes = 1 + usize::from(count >> 15);
                if count & 0x7fff != record.components {
                    return Some(false);
                }
                reader.peek_bytes(bytes * usize::from(record.components) + 3)?;
                for component in 0..record.components {
                    let index = if bytes == 1 {
                        u16::from(reader.read_byte()?)
                    } else {
                        reader.read_u16()?
                    };
                    if index != component {
                        return Some(false);
                    }
                }
                let wmcc = (u32::from(reader.read_byte()?) << 16) | u32::from(reader.read_u16()?);
                record.irreversible = (wmcc >> 16) & 1 == 0;
                record.decorrelation = None;
                record.offset = None;
                for (field, id) in [
                    (&mut record.decorrelation, wmcc as u8),
                    (&mut record.offset, (wmcc >> 8) as u8),
                ] {
                    if id != 0 {
                        *field = Some(self.mct.iter().position(|mct| mct.index == id)?);
                    }
                }
            }
            reader.at_end().then_some(true)
        })();
        // Ignored unsupported forms can partially change an existing record;
        // new records become visible only after the complete native procedure.
        if let Some(existing) = existing {
            self.mcc[existing] = record;
        } else if result == Some(true) {
            self.mcc.try_reserve(1).ok()?;
            self.mcc.push(record);
        }
        result.map(|_| ())
    }

    fn read_mco(&mut self, reader: &mut BitReader<'_>) -> Option<()> {
        let stages = reader.read_byte()?;
        if stages > 1 {
            return Some(());
        }
        if reader.tail()?.len() != usize::from(stages) {
            return None;
        }
        self.dc_offsets.fill(0);
        self.decoding_matrix = None;
        if stages == 0 {
            return Some(());
        }
        let index = reader.read_byte()?;
        let Some(record) = self.mcc.iter().find(|record| record.index == index) else {
            return Some(());
        };
        let components = self.dc_offsets.len();
        if usize::from(record.components) != components {
            return Some(());
        }
        if let Some(index) = record.decorrelation {
            let mct = &self.mct[index];
            let data = mct.elements(components.checked_mul(components)?)?;
            let mut matrix = Vec::new();
            matrix.try_reserve_exact(components * components).ok()?;
            for element in data.chunks_exact(mct.element_size()) {
                matrix.push(mct.float(element));
            }
            self.decoding_matrix = Some(matrix);
        }
        if let Some(index) = record.offset {
            let mct = &self.mct[index];
            let data = mct.elements(components)?;
            for (offset, element) in self
                .dc_offsets
                .iter_mut()
                .zip(data.chunks_exact(mct.element_size()))
            {
                *offset = mct.integer(element);
            }
        }
        Some(())
    }
}

impl Mct<'_> {
    fn element_size(&self) -> usize {
        [2, 4, 4, 8][usize::from(self.element_type)]
    }
    fn elements(&self, count: usize) -> Option<&[u8]> {
        let data = self.data?;
        (data.len() == count.checked_mul(self.element_size())?).then_some(data)
    }
    fn float(&self, data: &[u8]) -> f32 {
        // Native integer-to-float readers zero-extend both 16/32-bit inputs.
        match self.element_type {
            0 => u16::from_be_bytes(data.try_into().unwrap()) as f32,
            1 => u32::from_be_bytes(data.try_into().unwrap()) as f32,
            2 => f32::from_be_bytes(data.try_into().unwrap()),
            3 => f64::from_be_bytes(data.try_into().unwrap()) as f32,
            _ => unreachable!(),
        }
    }
    fn integer(&self, data: &[u8]) -> i32 {
        match self.element_type {
            0 => i32::from(u16::from_be_bytes(data.try_into().unwrap())),
            1 => u32::from_be_bytes(data.try_into().unwrap()) as i32,
            // Rust defines saturation/NaN for native's undefined out-of-range
            // float casts. Finite representable values truncate as native.
            2 => f32::from_be_bytes(data.try_into().unwrap()) as i32,
            3 => f64::from_be_bytes(data.try_into().unwrap()) as i32,
            _ => unreachable!(),
        }
    }
}

pub(crate) fn read_cbd(
    reader: &mut BitReader<'_>,
    components: &mut [ComponentSizeInfo],
) -> Option<()> {
    if usize::from(reader.read_u16()?) != components.len() + 4 {
        return None;
    }
    if usize::from(reader.read_u16()?) != components.len() {
        return None;
    }
    for component in components {
        let definition = reader.read_byte()?;
        component.is_signed = definition & 128 != 0;
        component.precision = (definition & 127) + 1;
        if component.precision > 31 {
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read<'a>(state: &mut State<'a>, kind: u8, data: &'a [u8]) -> Option<()> {
        state.read(kind, &mut BitReader::new(data))
    }

    #[test]
    fn native_mct_record_indices_survive_replacement_and_cleared_data() {
        let mut state = State {
            dc_offsets: alloc::vec![128],
            ..State::default()
        };
        // Native int32-to-float zero extends, while int32 offsets retain bits.
        read(
            &mut state,
            0x74,
            &[0, 12, 0, 0, 4, 1, 0, 0, 255, 255, 255, 255],
        )
        .unwrap();
        read(
            &mut state,
            0x75,
            &[0, 19, 0, 0, 3, 0, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 0, 1],
        )
        .unwrap();
        read(&mut state, 0x77, &[0, 4, 1, 3]).unwrap();
        assert_eq!(
            state.decoding_matrix.as_deref(),
            Some(&[u32::MAX as f32][..])
        );
        assert_eq!(state.dc_offsets, [0]);
        // An ignored Ymct still clears an existing record, whose MCC link
        // points to the same stable record index. Applying it must now fail.
        read(&mut state, 0x74, &[0, 9, 0, 0, 8, 1, 0, 1, 0]).unwrap();
        assert!(read(&mut state, 0x77, &[0, 4, 1, 3]).is_none());
    }

    #[test]
    fn unsupported_mcc_keeps_partial_existing_record_changes() {
        let mut state = State {
            dc_offsets: alloc::vec![128],
            ..State::default()
        };
        read(&mut state, 0x74, &[0, 10, 0, 0, 2, 1, 0, 0, 0, 7]).unwrap();
        read(
            &mut state,
            0x75,
            &[0, 19, 0, 0, 3, 0, 0, 0, 1, 1, 0, 1, 0, 0, 1, 0, 0, 1, 0],
        )
        .unwrap();
        read(&mut state, 0x77, &[0, 4, 1, 3]).unwrap();
        assert_eq!(state.dc_offsets, [7]);
        // Two input components with a shuffled first index: successful native
        // skip after updating the existing record's component count to two.
        read(
            &mut state,
            0x75,
            &[0, 16, 0, 0, 3, 0, 0, 0, 1, 1, 0, 2, 1, 0, 0, 2],
        )
        .unwrap();
        read(&mut state, 0x77, &[0, 4, 1, 3]).unwrap();
        assert_eq!(state.dc_offsets, [0]);
    }
}
