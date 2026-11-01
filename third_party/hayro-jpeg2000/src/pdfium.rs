// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2019 The PDFium Authors
// Use of this source code is governed by the BSD-style license in
// ../LICENSE-PDFIUM.

//! Raw codestream entry for PDFium's JP2 wrapper and exact reduction setting.

use crate::j2c::{self, Header};
use crate::{DecodeSettings, PdfiumComponent, Result, ValidationError};
use alloc::vec::Vec;

/// A borrowed codestream with native component metadata and reduction levels.
/// JP2 palette, channel and color boxes are handled by the PDFium adapter.
pub struct Codestream<'a> {
    data: &'a [u8],
    header: Header<'a>,
    header_bytes: usize,
}

impl<'a> Codestream<'a> {
    /// Read a strict codestream header with an exact reduction count. Excessive
    /// counts fail instead of being clamped to a resolution hint.
    pub fn new(data: &'a [u8], levels_to_skip: u8) -> Result<Self> {
        if levels_to_skip > 32 {
            return Err(ValidationError::InvalidComponentMetadata.into());
        }
        let settings = DecodeSettings {
            strict: true,
            target_resolution: None,
            resolve_palette_indices: false,
        };
        let mut parsed = j2c::parse_raw_with_reduce(data, &settings, true, levels_to_skip)?;
        if levels_to_skip > 32 || parsed.header.component_infos.len() > 16384 {
            return Err(ValidationError::InvalidComponentMetadata.into());
        }
        if parsed
            .header
            .component_infos
            .iter()
            .any(|component| levels_to_skip >= component.num_resolution_levels())
        {
            return Err(ValidationError::TooManySkippedResolutionLevels.into());
        }
        parsed.header.skipped_resolution_levels = levels_to_skip;
        // The native path computes component bounds directly from the original
        // grid and reduction count. Backend overall-image shrink hints are unused.
        Ok(Self {
            data: parsed.data,
            header: parsed.header,
            header_bytes: data.len() - parsed.data.len(),
        })
    }

    /// Decode component planes without JP2 color, palette or channel changes.
    pub fn decode_components(&self) -> Result<Vec<PdfiumComponent>> {
        decode_components(self.data, &self.header)
    }

    /// Original reference-grid image dimensions, for the JP2 IHDR/SIZ check.
    pub fn reference_dimensions(&self) -> (u32, u32) {
        let size = &self.header.size_data;
        (
            size.reference_grid_width - size.image_area_x_offset,
            size.reference_grid_height - size.image_area_y_offset,
        )
    }

    /// Decode planes and report the actual position after EOC. JP2's declared
    /// codestream box length does not bound the native codec's reads.
    pub fn decode_components_with_offset(&self) -> Result<(Vec<PdfiumComponent>, usize)> {
        decode_components_with_offset(self.data, &self.header)
            .map(|(components, offset)| (components, self.header_bytes + offset))
    }
}

pub(crate) fn decode_components(data: &[u8], header: &Header<'_>) -> Result<Vec<PdfiumComponent>> {
    decode_components_with_offset(data, header).map(|(components, _)| components)
}

fn decode_components_with_offset(
    data: &[u8],
    header: &Header<'_>,
) -> Result<(Vec<PdfiumComponent>, usize)> {
    let (planes, offset) = j2c::decode_pdfium(data, header)?;
    let components = planes
        .into_iter()
        .zip(&header.component_infos)
        .map(|(plane, info)| {
            let rect = j2c::component_rect(header, &info.size_info)?;
            Ok(PdfiumComponent {
                width: rect.width(),
                height: rect.height(),
                dx: u32::from(info.size_info.horizontal_resolution),
                dy: u32::from(info.size_info.vertical_resolution),
                precision: info.size_info.precision,
                signed: info.size_info.is_signed,
                samples: plane.container.into_truncated(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((components, offset))
}
