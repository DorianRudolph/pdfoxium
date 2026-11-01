// Translated from pinned third_party/libopenjpeg/jp2.c.
// Copyright notices and BSD-2-Clause terms: OPENJPEG-JP2-LICENSE.

//! JP2 boxes and component postprocessing at CJPX_Decoder's codec boundary.

use super::JpxColorSpace;
use hayro_jpeg2000::PdfiumComponent;

const SIGNATURE: &[u8] = &[0, 0, 0, 12, b'j', b'P', b' ', b' ', 13, 10, 135, 10];

struct ImageHeader {
    width: u32,
    height: u32,
    channels: usize,
}

struct Column {
    precision: u8,
    signed: bool,
}

struct Mapping {
    component: usize,
    kind: u8,
    column: usize,
}

struct Palette {
    columns: Vec<Column>,
    entries: Vec<u32>,
    entry_count: usize,
    mapping: Option<Vec<Mapping>>,
}

struct Definition {
    channel: usize,
    kind: u16,
    association: u16,
}

pub(super) struct Header<'a> {
    pub codestream: &'a [u8],
    pub colorspace: JpxColorSpace,
    is_jp2: bool,
    signature: bool,
    file_type: bool,
    header: bool,
    image: Option<ImageHeader>,
    color_seen: bool,
    palette: Option<Palette>,
    definitions: Option<Vec<Definition>>,
}

struct BoxHeader {
    kind: [u8; 4],
    length: u32,
    header_size: usize,
}

fn take<'a>(source: &'a [u8], offset: &mut usize, count: usize) -> Option<&'a [u8]> {
    let end = offset.checked_add(count)?;
    let data = source.get(*offset..end)?;
    *offset = end;
    Some(data)
}

fn u16_at(source: &[u8], offset: usize) -> Option<u16> {
    Some(u16::from_be_bytes(
        source.get(offset..offset + 2)?.try_into().ok()?,
    ))
}

fn u32_at(source: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_be_bytes(
        source.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

/// Stream reads consume partial headers before reporting failure. The native
/// outer header loop treats that failure as the end of its box sequence.
fn stream_box(source: &[u8], offset: &mut usize) -> Option<BoxHeader> {
    let start = *offset;
    let available = (source.len() - start).min(8);
    *offset += available;
    if available != 8 {
        return None;
    }
    let mut length = u32_at(source, start)?;
    let kind = source[start + 4..start + 8].try_into().ok()?;
    let mut header_size = 8;
    if length == 0 {
        length = u32::try_from(source.len() - start).ok()?;
    } else if length == 1 {
        let available = (source.len() - *offset).min(8);
        let extended = *offset;
        *offset += available;
        if available != 8 || u32_at(source, extended)? != 0 {
            return None;
        }
        length = u32_at(source, extended + 4)?;
        header_size = 16;
    }
    Some(BoxHeader {
        kind,
        length,
        header_size,
    })
}

fn image_box(kind: &[u8; 4]) -> bool {
    matches!(
        kind,
        b"ihdr" | b"colr" | b"bpcc" | b"pclr" | b"cmap" | b"cdef"
    )
}

impl<'a> Header<'a> {
    pub fn parse(source: &'a [u8]) -> Option<Self> {
        let is_jp2 = source.starts_with(SIGNATURE);
        let mut result = Self {
            codestream: source,
            colorspace: if is_jp2 {
                JpxColorSpace::Unknown
            } else {
                JpxColorSpace::Unspecified
            },
            is_jp2,
            signature: false,
            file_type: false,
            header: false,
            image: None,
            color_seen: false,
            palette: None,
            definitions: None,
        };
        if is_jp2 {
            let mut offset = 0;
            result.read_headers(source, &mut offset)?;
            if !result.header || result.image.is_none() {
                return None;
            }
            // JP2C's declared length does not bound the underlying J2K reader.
            result.codestream = &source[offset..];
        }
        Some(result)
    }

    pub fn dimensions_match(&self, dimensions: (u32, u32)) -> bool {
        self.image
            .as_ref()
            .is_none_or(|image| (image.width, image.height) == dimensions)
    }

    pub fn finish(&mut self, consumed: usize) -> Option<()> {
        if self.is_jp2 {
            let tail = self.codestream.get(consumed..)?;
            self.read_headers(tail, &mut 0)?;
        }
        Some(())
    }

    fn read_headers(&mut self, source: &[u8], offset: &mut usize) -> Option<()> {
        while let Some(box_header) = stream_box(source, offset) {
            if &box_header.kind == b"jp2c" {
                return self.header.then_some(());
            }
            let count = (box_header.length as usize).checked_sub(box_header.header_size)?;
            let known = matches!(&box_header.kind, b"jP  " | b"ftyp" | b"jp2h");
            let misplaced = image_box(&box_header.kind);
            if known || (misplaced && self.header) {
                let payload = take(source, offset, count)?;
                match &box_header.kind {
                    b"jP  " => {
                        if self.signature
                            || self.file_type
                            || self.header
                            || payload != [13, 10, 135, 10]
                        {
                            return None;
                        }
                        self.signature = true;
                    }
                    b"ftyp" => {
                        if !self.signature
                            || self.file_type
                            || self.header
                            || payload.len() < 8
                            || (payload.len() - 8) % 4 != 0
                        {
                            return None;
                        }
                        self.file_type = true;
                    }
                    b"jp2h" => {
                        if !self.file_type {
                            return None;
                        }
                        self.read_image_header(payload)?;
                        self.header = true;
                    }
                    _ => self.read_image_box(&box_header.kind, payload)?,
                }
            } else {
                if !self.signature || !self.file_type {
                    return None;
                }
                // OpenJPEG's memory stream permits skips past EOF, clamped there.
                *offset = offset.saturating_add(count).min(source.len());
            }
        }
        Some(())
    }

    fn read_image_header(&mut self, payload: &[u8]) -> Option<()> {
        let mut offset = 0;
        let mut has_ihdr = false;
        while offset < payload.len() {
            // Unlike stream boxes, a child box may not have an undefined size.
            let original = u32_at(payload, offset)?;
            if original == 0 {
                return None;
            }
            let box_header = stream_box(payload, &mut offset)?;
            if box_header.length == 0 {
                return None;
            }
            let count = (box_header.length as usize).checked_sub(box_header.header_size)?;
            let data = take(payload, &mut offset, count)?;
            if image_box(&box_header.kind) {
                self.read_image_box(&box_header.kind, data)?;
            }
            has_ihdr |= &box_header.kind == b"ihdr";
        }
        has_ihdr.then_some(())
    }

    fn read_image_box(&mut self, kind: &[u8; 4], payload: &[u8]) -> Option<()> {
        match kind {
            b"ihdr" => {
                if self.image.is_some() {
                    return Some(());
                }
                if payload.len() != 14 {
                    return None;
                }
                let height = u32_at(payload, 0)?;
                let width = u32_at(payload, 4)?;
                let channels = usize::from(u16_at(payload, 8)?);
                if width == 0 || height == 0 || !(1..=16384).contains(&channels) {
                    return None;
                }
                self.image = Some(ImageHeader {
                    width,
                    height,
                    channels,
                });
            }
            b"bpcc" => {
                if payload.len() != self.image.as_ref().map_or(0, |image| image.channels) {
                    return None;
                }
            }
            b"colr" => {
                if payload.len() < 3 {
                    return None;
                }
                if self.color_seen {
                    return Some(());
                }
                match payload[0] {
                    1 => {
                        self.colorspace = match u32_at(payload, 3)? {
                            16 => JpxColorSpace::SRgb,
                            17 => JpxColorSpace::Gray,
                            18 => JpxColorSpace::Sycc,
                            24 => JpxColorSpace::Eycc,
                            12 => JpxColorSpace::Cmyk,
                            _ => JpxColorSpace::Unknown,
                        };
                        self.color_seen = true;
                    }
                    2 => {
                        // CJPX_Decoder discards the opaque ICC/CIELab payload.
                        self.color_seen = true;
                    }
                    _ => {} // Unsupported METH boxes do not consume first-COLR status.
                }
            }
            b"pclr" => {
                if self.palette.is_some() {
                    return None;
                }
                let entry_count = usize::from(u16_at(payload, 0)?);
                let channel_count = usize::from(*payload.get(2)?);
                if !(1..=1024).contains(&entry_count) || channel_count == 0 {
                    return None;
                }
                let descriptors = payload.get(3..3 + channel_count)?;
                let columns: Vec<_> = descriptors
                    .iter()
                    .map(|&descriptor| Column {
                        precision: (descriptor & 127) + 1,
                        signed: descriptor & 128 != 0,
                    })
                    .collect();
                let mut offset = 3 + channel_count;
                let mut entries = Vec::with_capacity(entry_count * channel_count);
                for _ in 0..entry_count {
                    for column in &columns {
                        let bytes = take(
                            payload,
                            &mut offset,
                            usize::from(column.precision).div_ceil(8).min(4),
                        )?;
                        let value = bytes
                            .iter()
                            .fold(0u32, |value, &byte| (value << 8) | u32::from(byte));
                        entries.push(value);
                    }
                }
                self.palette = Some(Palette {
                    columns,
                    entries,
                    entry_count,
                    mapping: None,
                });
            }
            b"cmap" => {
                let palette = self.palette.as_mut()?;
                if palette.mapping.is_some() {
                    return None;
                }
                let channel_count = palette.columns.len();
                let data = payload.get(..channel_count * 4)?;
                let mut mapping = Vec::with_capacity(channel_count);
                for data in data.chunks_exact(4) {
                    mapping.push(Mapping {
                        component: usize::from(u16::from_be_bytes([data[0], data[1]])),
                        kind: data[2],
                        column: usize::from(data[3]),
                    });
                }
                palette.mapping = Some(mapping);
            }
            b"cdef" => {
                if self.definitions.is_some() {
                    return None;
                }
                let count = usize::from(u16_at(payload, 0)?);
                if count == 0 {
                    return None;
                }
                let data = payload.get(2..2 + count * 6)?;
                let mut definitions = Vec::with_capacity(count);
                for data in data.chunks_exact(6) {
                    definitions.push(Definition {
                        channel: usize::from(u16_at(data, 0)?),
                        kind: u16_at(data, 2)?,
                        association: u16_at(data, 4)?,
                    });
                }
                self.definitions = Some(definitions);
            }
            _ => {}
        }
        Some(())
    }

    pub fn apply_components(
        &mut self,
        mut components: Vec<PdfiumComponent>,
        indexed: bool,
    ) -> Option<Vec<PdfiumComponent>> {
        if indexed {
            return Some(components);
        }
        let channel_count = self
            .palette
            .as_ref()
            .filter(|palette| palette.mapping.is_some())
            .map_or(components.len(), |palette| palette.columns.len());
        if let Some(definitions) = &self.definitions {
            for definition in definitions {
                if definition.channel >= channel_count
                    || (definition.association != 65535
                        && usize::from(definition.association) > channel_count)
                {
                    return None;
                }
            }
            for channel in 0..channel_count {
                if !definitions
                    .iter()
                    .any(|definition| definition.channel == channel)
                {
                    return None;
                }
            }
        }
        if let Some(mut palette) = self.palette.take() {
            if let Some(mut mapping) = palette.mapping.take() {
                let mut used = vec![false; palette.columns.len()];
                for (index, entry) in mapping.iter().enumerate() {
                    if entry.component >= components.len()
                        || entry.kind > 1
                        || entry.column >= used.len()
                        || (entry.kind == 1 && (used[entry.column] || entry.column != index))
                        || (entry.kind == 0 && entry.column != 0)
                    {
                        return None;
                    }
                    used[entry.column] = true;
                }
                for (index, &used) in used.iter().enumerate() {
                    if !used && mapping[index].kind != 0 {
                        return None;
                    }
                }
                if components.len() == 1 && used.iter().any(|&used| !used) {
                    for (index, entry) in mapping.iter_mut().enumerate() {
                        entry.kind = 1;
                        entry.column = index;
                    }
                }
                let mut mapped = Vec::with_capacity(mapping.len());
                for (index, entry) in mapping.iter().enumerate() {
                    let source = &components[entry.component];
                    if source.height == 0 || source.width > (u32::MAX / 4) / source.height {
                        return None;
                    }
                    let count = source.width as usize * source.height as usize;
                    let input = source.samples.get(..count)?;
                    if input.is_empty() {
                        return None;
                    }
                    let mut samples = Vec::new();
                    samples.try_reserve_exact(count).ok()?;
                    for &sample in input {
                        let value = if entry.kind == 0 {
                            sample
                        } else {
                            let palette_index =
                                (sample as i32).clamp(0, palette.entry_count as i32 - 1) as usize;
                            palette.entries[palette_index * palette.columns.len() + entry.column]
                                as i32 as f32
                        };
                        samples.push(value);
                    }
                    mapped.push(PdfiumComponent {
                        width: source.width,
                        height: source.height,
                        dx: source.dx,
                        dy: source.dy,
                        precision: palette.columns[index].precision,
                        signed: palette.columns[index].signed,
                        samples,
                    });
                }
                components = mapped;
            }
        }
        if let Some(mut definitions) = self.definitions.take() {
            for index in 0..definitions.len() {
                let channel = definitions[index].channel;
                let association = definitions[index].association;
                if association == 0 || association == 65535 {
                    continue;
                }
                let target = usize::from(association - 1);
                if channel != target && definitions[index].kind == 0 {
                    components.swap(channel, target);
                    for next in &mut definitions[index + 1..] {
                        if next.channel == channel {
                            next.channel = target;
                        } else if next.channel == target {
                            next.channel = channel;
                        }
                    }
                }
                // OpenJPEG's alpha tag is not exposed by CJPX_Decoder::GetInfo;
                // selected rendering uses component count/SMaskInData instead.
            }
        }
        Some(components)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn component(samples: Vec<f32>) -> PdfiumComponent {
        PdfiumComponent {
            width: samples.len() as u32,
            height: 1,
            dx: 1,
            dy: 1,
            precision: 8,
            signed: true,
            samples,
        }
    }

    #[test]
    fn jpx_palette_mapping_keeps_native_clamps_signed_flags_and_indexed_bypass() {
        let mut header = Header::parse(&[]).unwrap();
        let palette = [0, 2, 3, 7, 7, 135, 10, 20, 30, 40, 50, 60];
        header.read_image_box(b"pclr", &palette).unwrap();
        let source = component(vec![-3.0, 0.0, 1.0, 9.0]);
        let pointer = source.samples.as_ptr();
        let unchanged = header.apply_components(vec![source], false).unwrap();
        assert_eq!(unchanged.len(), 1);
        assert_eq!(unchanged[0].samples.as_ptr(), pointer); // No CMAP means no palette.

        header.read_image_box(b"pclr", &palette).unwrap();
        header
            .read_image_box(b"cmap", &[0, 0, 1, 0, 0, 0, 1, 1, 0, 0, 1, 2])
            .unwrap();
        let mapped = header.apply_components(unchanged, false).unwrap();
        assert_eq!(mapped[0].samples, [10.0, 10.0, 40.0, 40.0]);
        assert_eq!(mapped[1].samples, [20.0, 20.0, 50.0, 50.0]);
        assert_eq!(mapped[2].samples, [30.0, 30.0, 60.0, 60.0]);
        assert!(!mapped[0].signed);
        assert!(mapped[2].signed); // Palette entries stay raw, without sign extension.

        header.read_image_box(b"pclr", &palette).unwrap();
        header
            .read_image_box(b"cmap", &[0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255])
            .unwrap();
        let indexed = header
            .apply_components(vec![component(vec![1.0])], true)
            .unwrap();
        assert_eq!(indexed[0].samples, [1.0]); // Indexed skips check_color too.
    }

    #[test]
    fn jpx_cdef_swaps_named_channels_and_updates_following_definitions() {
        let mut header = Header::parse(&[]).unwrap();
        header
            .read_image_box(
                b"cdef",
                &[
                    0, 4, 0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0, 2, 0, 1, 0, 0, 0, 3, 0, 3, 0, 1, 0, 0,
                ],
            )
            .unwrap();
        let components = vec![
            component(vec![10.0]),
            component(vec![20.0]),
            component(vec![30.0]),
            component(vec![40.0]),
        ];
        let original_third = components[2].samples.as_ptr();
        let mapped = header.apply_components(components, false).unwrap();
        assert_eq!(
            mapped
                .iter()
                .map(|component| component.samples[0])
                .collect::<Vec<_>>(),
            [30.0, 10.0, 20.0, 40.0]
        );
        assert_eq!(mapped[0].samples.as_ptr(), original_third);
        header
            .read_image_box(b"cdef", &[0, 1, 0, 0, 0, 0, 0, 1])
            .unwrap();
        assert!(header.apply_components(mapped, false).is_none()); // Incomplete list.
    }

    #[test]
    fn jpx_codestream_box_lengths_do_not_bound_decode_and_trailing_boxes_are_checked() {
        let fixture = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/rgb.jp2"
        ));
        let header = Header::parse(fixture).unwrap();
        let codestream_start = fixture.len() - header.codestream.len();
        for length in [0u32, 4, u32::MAX] {
            let mut source = fixture.to_vec();
            source[codestream_start - 8..codestream_start - 4]
                .copy_from_slice(&length.to_be_bytes());
            assert!(
                super::super::CjpxDecoder::create(&source, false, 0).is_some(),
                "length {length}"
            );
        }
        for (suffix, succeeds) in [
            (&[0, 0, 0, 10, b'c', b'o', b'l', b'r', 0, 0][..], false),
            (&[255, 255, 255, 255, b'u', b'u', b'i', b'd'][..], true),
            (&[1, 2][..], true),
        ] {
            let mut source = fixture.to_vec();
            source.extend_from_slice(suffix);
            assert_eq!(
                super::super::CjpxDecoder::create(&source, false, 0).is_some(),
                succeeds
            );
        }
    }
}
