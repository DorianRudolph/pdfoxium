// Copyright (c) 2026 DorianRudolph
// SPDX-License-Identifier: MIT

//! JPEG framing for the consumed source boundary of inline PDF images.

/// Find the first EOI after SOS, skipping marker payloads and byte-stuffed
/// entropy data. The input has already undergone the adapter's SOI/trailer
/// repair. This follows JPEG's marker framing, including multiple scans and
/// standalone restart/TEM markers, without interpreting compressed samples.
/// A missing endpoint or incomplete length-delimited marker is an error, not
/// permission to report all supplied bytes as consumed.
pub(super) fn jpeg_endpoint(source: &[u8]) -> Option<u32> {
    if !source.starts_with(&[0xff, 0xd8]) {
        return None;
    }
    let mut position = 2usize;
    let mut entropy = false;
    let mut saw_scan = false;
    loop {
        // In entropy data these are samples; between segments they are
        // extraneous bytes that libjpeg's marker reader also discards.
        let skip = source
            .get(position..)?
            .iter()
            .position(|&byte| byte == 0xff)?;
        position = position.checked_add(skip)?;
        while *source.get(position)? == 0xff {
            position = position.checked_add(1)?;
        }
        let marker = *source.get(position)?;
        position = position.checked_add(1)?;
        match marker {
            0x00 if entropy => continue,        // FF 00 encodes an FF sample byte.
            0xd0..=0xd7 if entropy => continue, // Restart; entropy resumes.
            0x01 => continue,                   // TEM is standalone and carries no length.
            0xd9 if saw_scan => return u32::try_from(position).ok(),
            0xd9 => return None,
            0x00 | 0xd0..=0xd8 => return None,
            _ => {}
        }

        let length_end = position.checked_add(2)?;
        let length_bytes = source.get(position..length_end)?;
        let length = usize::from(u16::from_be_bytes([length_bytes[0], length_bytes[1]]));
        if length < 2 {
            return None;
        }
        let end = position.checked_add(length)?;
        source.get(position..end)?;
        position = end;
        if marker == 0xda {
            // SOS starts another entropy segment; its header is not entropy.
            saw_scan = true;
            entropy = true;
        } else if marker != 0xdc {
            // DNL may occur within a scan and entropy continues after its
            // length-delimited payload. Other markers terminate that scan.
            entropy = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOS: &[u8] = &[0xff, 0xda, 0, 8, 1, 1, 0, 0, 63, 0];

    #[test]
    fn fixture_endpoints_exclude_inline_pdf_trailers() {
        for jpeg in [
            include_bytes!("../../../tests/fixtures/gray.jpg").as_slice(),
            include_bytes!("../../../tests/fixtures/cmyk.jpg").as_slice(),
        ] {
            assert_eq!(jpeg_endpoint(jpeg), Some(jpeg.len() as u32));
            let mut with_trailer = jpeg.to_vec();
            with_trailer.extend_from_slice(b" EI q /Next Do Q");
            assert_eq!(jpeg_endpoint(&with_trailer), Some(jpeg.len() as u32));
        }
    }

    #[test]
    fn endpoint_skips_payload_markers_stuffing_restarts_and_progressive_headers() {
        let mut source = vec![0xff, 0xd8];
        // APP1 has an apparent EOI inside its opaque payload.
        source.extend_from_slice(&[0xff, 0xe1, 0, 8, 0xff, 0xd9, 0xff, 0xda, 0, 0]);
        source.extend_from_slice(SOS);
        // FF 00 D9 is entropy data, not an EOI. Restart, TEM and fill bytes
        // leave entropy parsing active. DNL also resumes the current scan.
        source.extend_from_slice(&[
            0x12, 0xff, 0x00, 0xd9, 0xff, 0xd0, 0x34, 0xff, 0xff, 0xd7, 0xff, 0x01, 0x56, 0xff,
            0xdc, 0, 4, 0, 8, 0x78, 0xff, 0x00, 0xd9,
        ]);
        // An interscan DHT payload contains another false EOI.
        source.extend_from_slice(&[0xff, 0xc4, 0, 4, 0xff, 0xd9]);
        source.extend_from_slice(SOS);
        source.extend_from_slice(&[0x9a, 0xff, 0x00, 0xd9, 0xff, 0xff, 0xd9]);
        let endpoint = source.len() as u32;
        source.extend_from_slice(b" EI ");
        source.extend_from_slice(&[0xff, 0xd9]);
        assert_eq!(jpeg_endpoint(&source), Some(endpoint));
    }

    #[test]
    fn endpoint_rejects_incomplete_or_invalid_framing() {
        let mut source = vec![0xff, 0xd8];
        source.extend_from_slice(SOS);
        source.extend_from_slice(&[0x12, 0xff, 0x00, 0x34, 0xff, 0xd9]);
        for end in 0..source.len() {
            assert_eq!(jpeg_endpoint(&source[..end]), None, "prefix={end}");
        }
        for invalid in [
            vec![0xff, 0xd8, 0xff, 0xd9],       // No scan.
            vec![0xff, 0xd8, 0xff, 0xe1, 0, 0], // Invalid segment length.
            vec![0xff, 0xd8, 0xff, 0xe1, 0, 1],
            vec![0xff, 0xd8, 0xff, 0xe1, 0xff, 0xff, 0xff, 0xd9], // Past input.
            vec![0xff, 0xd8, 0xff, 0x00, 0xff, 0xd9],             // Stuffing outside scan.
            vec![0xff, 0xd8, 0xff, 0xd0, 0xff, 0xd9],             // Restart outside scan.
            vec![0xff, 0xd8, 0xff, 0xd8],                         // Nested SOI.
        ] {
            assert_eq!(jpeg_endpoint(&invalid), None, "input={invalid:02x?}");
        }
    }
}
