// Pinned j2k.c::opj_j2k_read_SQcd_SQcc and copy_tile_quantization_parameters.
// BSD-2-Clause notices: ../../LICENSE-OPENJPEG-J2K.

use super::codestream::{QuantizationInfo, QuantizationStyle, StepSize};
use crate::reader::BitReader;
use alloc::vec;

fn payload<'a>(reader: &mut BitReader<'a>) -> Option<BitReader<'a>> {
    let length = reader.read_u16()?.checked_sub(2)?;
    Some(BitReader::new(reader.read_bytes(usize::from(length))?))
}

pub(crate) fn qcd(
    reader: &mut BitReader<'_>,
    previous: Option<&QuantizationInfo>,
) -> Option<QuantizationInfo> {
    parameters(&mut payload(reader)?, previous)
}

pub(crate) fn qcc<'a>(
    reader: &mut BitReader<'_>,
    components: u16,
    previous: impl FnOnce(u16) -> Option<&'a QuantizationInfo>,
) -> Option<(u16, QuantizationInfo)> {
    let mut reader = payload(reader)?;
    let index = if components <= 256 {
        u16::from(reader.read_byte()?)
    } else {
        reader.read_u16()?
    };
    if index >= components {
        return None;
    }
    Some((index, parameters(&mut reader, previous(index))?))
}

fn parameters(
    reader: &mut BitReader<'_>,
    previous: Option<&QuantizationInfo>,
) -> Option<QuantizationInfo> {
    let sqcx = reader.read_byte()?;
    let style = sqcx & 31;
    let bytes = reader.tail()?.len();
    let count = if style == 1 {
        1
    } else if style == 0 {
        bytes
    } else {
        bytes / 2
    };
    // Native has 97 initialized entries. A partial marker overwrites only its
    // prefix; scalar-derived updates all entries and clamps derived exponents.
    let mut steps = previous.map_or_else(|| vec![], |info| info.step_sizes.clone());
    steps.resize(
        97,
        StepSize {
            exponent: 0,
            mantissa: 0,
        },
    );
    for index in 0..count {
        let step = if style == 0 {
            StepSize {
                exponent: u16::from(reader.read_byte()? >> 3),
                mantissa: 0,
            }
        } else {
            let value = reader.read_u16()?;
            StepSize {
                exponent: value >> 11,
                mantissa: value & 2047,
            }
        };
        if index < 97 {
            steps[index] = step;
        }
    }
    if !reader.at_end() {
        return None;
    }
    if style == 1 {
        for index in 1..97 {
            steps[index] = StepSize {
                exponent: steps[0].exponent.saturating_sub(((index - 1) / 3) as u16),
                mantissa: steps[0].mantissa,
            };
        }
    }
    Some(QuantizationInfo {
        quantization_style: QuantizationStyle::Native(style),
        guard_bits: sqcx >> 5,
        step_sizes: steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_quantization_keeps_fixed_tail_and_clamps_scalar_derived_exponents() {
        let original = qcd(&mut BitReader::new(&[0, 6, 64, 64, 72, 80]), None).unwrap();
        let updated = qcd(&mut BitReader::new(&[0, 4, 96, 48]), Some(&original)).unwrap();
        assert_eq!(updated.step_sizes.len(), 97);
        assert_eq!(updated.guard_bits, 3);
        assert_eq!(
            updated.step_sizes[..4]
                .iter()
                .map(|step| step.exponent)
                .collect::<Vec<_>>(),
            [6, 9, 10, 0]
        );
        let derived = qcd(&mut BitReader::new(&[0, 5, 65, 8, 17]), None).unwrap();
        assert_eq!(derived.step_sizes[3].exponent, 1);
        assert_eq!(derived.step_sizes[4].exponent, 0);
        assert_eq!(derived.step_sizes[96].exponent, 0);
        assert_eq!(derived.step_sizes[96].mantissa, 17);
        let unknown = qcd(&mut BitReader::new(&[0, 7, 69, 48, 0, 40, 1]), None).unwrap();
        assert_eq!(unknown.quantization_style, QuantizationStyle::Native(5));
        assert_eq!(unknown.step_sizes[1].exponent, 5);
        assert_eq!(unknown.step_sizes[1].mantissa, 1);
        assert!(qcd(&mut BitReader::new(&[0, 4, 69, 48]), None).is_none());
        assert!(qcd(&mut BitReader::new(&[0, 6, 65, 8, 17, 0]), None).is_none());
    }
}
