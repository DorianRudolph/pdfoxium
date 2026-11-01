// Pinned j2k.c::read_unk/read_siz/read_tlm/read_plm/read_plt/read_crg/read_com.
// BSD-2-Clause notices: ../../LICENSE-OPENJPEG-J2K.

use crate::reader::BitReader;

fn known(marker: u8) -> bool {
    matches!(
        marker,
        0x50 | 0x51
            | 0x52
            | 0x53
            | 0x55
            | 0x57
            | 0x58
            | 0x59
            | 0x5c
            | 0x5d
            | 0x5e
            | 0x5f
            | 0x60
            | 0x61
            | 0x63
            | 0x64
            | 0x74
            | 0x75
            | 0x77
            | 0x78
            | 0x90
            | 0x91
    )
}

pub(crate) fn recover_main(reader: &mut BitReader<'_>, before_siz: bool) -> Option<()> {
    let current = reader.peek_marker()?;
    if known(current) {
        return None;
    }
    reader.skip_bytes(2)?;
    loop {
        let pair = reader.peek_bytes(2)?;
        if pair[0] == 255 {
            let marker = pair[1];
            if known(marker) {
                let allowed = if before_siz {
                    marker == 0x51
                } else {
                    !matches!(marker, 0x51 | 0x58 | 0x61 | 0x91)
                };
                return allowed.then_some(());
            }
            if before_siz {
                return None;
            }
        }
        // Native scans two-byte pairs, not individual bytes or marker lengths.
        // Leave the recognized pair for the caller's normal marker dispatch.
        reader.skip_bytes(2)?;
    }
}

pub(crate) fn siz(reader: &BitReader<'_>) -> Option<()> {
    // Reader points at Lsiz after its marker, including two length bytes.
    let fixed = reader.peek_bytes(38)?;
    let length = usize::from(u16::from_be_bytes([fixed[0], fixed[1]]));
    let components = usize::from(u16::from_be_bytes([fixed[36], fixed[37]]));
    if components == 0 || components > 16384 || length != 38 + 3 * components {
        return None;
    }
    let descriptors = &reader.peek_bytes(length)?[38..];
    if descriptors
        .chunks_exact(3)
        .any(|descriptor| descriptor[0] & 127 >= 31)
    {
        return None;
    }
    Some(())
}

pub(crate) fn ignored(reader: &mut BitReader<'_>, marker: u8, components: u16) -> Option<()> {
    let size = usize::from(reader.read_u16()?.checked_sub(2)?);
    let data = reader.read_bytes(size)?;
    match marker {
        0x55 => (size >= 2).then_some(()), // TLM: invalid index entries are ignored.
        0x57 => (size >= 1).then_some(()), // PLM: native ignores its length records.
        0x63 => (size == 4 * usize::from(components)).then_some(()),
        0x64 => Some(()), // COM: no payload validation in pinned source.
        0x58 => {
            let mut length = 0u32;
            for &byte in data.get(1..)? {
                length |= u32::from(byte & 127);
                if byte & 128 != 0 {
                    length <<= 7;
                } else {
                    length = 0;
                }
            }
            (length == 0).then_some(())
        }
        _ => unreachable!("ignored marker dispatch"),
    }
}
