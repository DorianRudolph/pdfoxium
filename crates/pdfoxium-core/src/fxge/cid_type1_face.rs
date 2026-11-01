// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Adapter for the CID-keyed Type 1 container (Adobe Technical Note #5014).
//!
//! Fontations handles Type 1 charstrings, but its Type1Font loader only accepts
//! name-keyed PFA/PFB containers. Read the CID dictionary/maps here, then use its
//! public evaluator. The native reference is FreeType's cidparse/cidload/cidgload
//! pipeline, reached through CFX_Face::New(). No PostScript program is executed.

use std::borrow::Cow;

use read_fonts::{
    model::Error,
    ps::{
        charstring::{self, CharstringContext, CharstringKind, NopFilterSink, TransformSink},
        FontMatrix, Transform,
    },
    types::Fixed,
};
use skrifa::outline::OutlinePen;

#[derive(yoke::Yokeable)]
pub(super) struct CidType1Face<'a> {
    data: Cow<'a, [u8]>,
    map_offset: usize,
    fd_bytes: usize,
    gd_bytes: usize,
    count: u32,
    dicts: Vec<FontDict>,
    name: String,
    family: String,
    weight: String,
    italic: bool,
    fixed_pitch: bool,
    bbox: [f32; 4],
}

struct FontDict {
    len_iv: i32,
    subr_offset: usize,
    sd_bytes: usize,
    subr_count: usize,
    subrs: Vec<Vec<u8>>,
    transform: Transform,
}

impl Default for FontDict {
    fn default() -> Self {
        Self {
            len_iv: 4,
            subr_offset: 0,
            sd_bytes: 0,
            subr_count: 0,
            subrs: Vec::new(),
            transform: Transform::IDENTITY,
        }
    }
}

impl CharstringContext for FontDict {
    fn kind(&self) -> CharstringKind {
        CharstringKind::Type1
    }
    // CID fonts address glyphs by CID, without StandardEncoding glyph names.
    fn seac_components(&self, _: i32, _: i32) -> Result<[&[u8]; 2], Error> {
        Err(Error::InvalidOperation)
    }
    fn global_subr(&self, _: i32) -> Result<&[u8], Error> {
        Err(Error::InvalidOperation)
    }
    fn subr(&self, index: i32) -> Result<&[u8], Error> {
        self.subrs
            .get(usize::try_from(index).map_err(|_| Error::InvalidOperation)?)
            .map(Vec::as_slice)
            .ok_or(Error::MissingResource)
    }
}

impl<'a> CidType1Face<'a> {
    pub fn new(input: &'a [u8]) -> Option<Self> {
        if !input.starts_with(b"%!PS-Adobe-3.0 Resource-CIDFont") {
            return None;
        }
        let mut face = Self {
            data: Cow::Borrowed(&[]),
            map_offset: 0,
            fd_bytes: 0,
            gd_bytes: 0,
            count: 0,
            dicts: Vec::new(),
            name: String::new(),
            family: String::new(),
            weight: String::new(),
            italic: false,
            fixed_pitch: false,
            bbox: [0.0; 4],
        };
        let mut tokens = Tokens { input, pos: 0 };
        let mut dict = None;
        let mut declared_dicts = 0;
        let mut previous = [b"".as_slice(); 2];
        loop {
            let token = tokens.next()?;
            match token {
                b"StartData" => {
                    // StartData consumes one separator, not all whitespace:
                    // binary CID maps often begin with zero/whitespace bytes.
                    if !input.get(tokens.pos)?.is_ascii_whitespace() {
                        return None;
                    }
                    let data = input.get(tokens.pos + 1..)?;
                    face.data = if previous[0] == b"(Hex)" {
                        let length = integer(previous[1])?;
                        let mut decoded = Vec::new();
                        let mut high = None;
                        for &byte in data {
                            if decoded.len() == length {
                                break;
                            }
                            if byte.is_ascii_whitespace() {
                                continue;
                            }
                            let digit = (byte as char).to_digit(16)? as u8;
                            if let Some(value) = high.take() {
                                decoded.push(value * 16 + digit);
                            } else {
                                high = Some(digit);
                            }
                        }
                        if decoded.len() != length || high.is_some() {
                            return None;
                        }
                        Cow::Owned(decoded)
                    } else if previous[0] == b"(Binary)" {
                        // Some producers overstate this length. Binary offsets
                        // are validated against the actual remaining stream.
                        Cow::Borrowed(data)
                    } else {
                        return None;
                    };
                    break;
                }
                b"%ADOBeginFontDict" => {
                    if face.dicts.len() >= declared_dicts {
                        return None;
                    }
                    dict = Some(face.dicts.len());
                    face.dicts.push(FontDict::default());
                }
                b"/CIDFontType" => {
                    if integer(tokens.next()?)? != 0 {
                        return None;
                    }
                }
                b"/CIDFontName" => face.name = text(tokens.next()?)?,
                b"/FamilyName" => face.family = text(tokens.next()?)?,
                b"/Weight" => face.weight = text(tokens.next()?)?,
                b"/ItalicAngle" => face.italic = number(tokens.next()?)? != 0.0,
                b"/isFixedPitch" => face.fixed_pitch = tokens.next()? == b"true",
                b"/FontBBox" => face.bbox = array(&mut tokens)?,
                b"/CIDMapOffset" => face.map_offset = integer(tokens.next()?)?,
                b"/FDBytes" => face.fd_bytes = integer(tokens.next()?)?,
                b"/GDBytes" => face.gd_bytes = integer(tokens.next()?)?,
                b"/CIDCount" => face.count = integer(tokens.next()?)?.try_into().ok()?,
                b"/FDArray" => {
                    declared_dicts = integer(tokens.next()?)?;
                    // Each dictionary needs at least a marker in this input.
                    // Do not reserve memory using an untrusted declared count.
                    if declared_dicts == 0 || declared_dicts > input.len() / 17 {
                        return None;
                    }
                }
                b"/lenIV" => {
                    face.dicts.get_mut(dict?)?.len_iv =
                        std::str::from_utf8(tokens.next()?).ok()?.parse().ok()?;
                }
                b"/SubrMapOffset" => {
                    face.dicts.get_mut(dict?)?.subr_offset = integer(tokens.next()?)?;
                }
                b"/SDBytes" => face.dicts.get_mut(dict?)?.sd_bytes = integer(tokens.next()?)?,
                b"/SubrCount" => face.dicts.get_mut(dict?)?.subr_count = integer(tokens.next()?)?,
                b"/FontMatrix" => {
                    let values: [f32; 6] = array(&mut tokens)?;
                    let [xx, yx, xy, yy, dx, dy] = values.map(|v| v * 1000.0);
                    if (xx * yy - xy * yx) == 0.0
                        || [xx, yx, xy, yy, dx, dy]
                            .iter()
                            .any(|v| !v.is_finite() || v.abs() >= 32768.0)
                    {
                        return None;
                    }
                    // Canonical 1000-unit em across all FDs. TransformSink
                    // expects translations in integer font units (Fixed bits).
                    face.dicts.get_mut(dict?)?.transform.matrix = FontMatrix::from_elements([
                        Fixed::from_f64(xx as f64),
                        Fixed::from_f64(yx as f64),
                        Fixed::from_f64(xy as f64),
                        Fixed::from_f64(yy as f64),
                        Fixed::from_bits(dx as i32),
                        Fixed::from_bits(dy as i32),
                    ]);
                }
                _ => {}
            }
            previous = [previous[1], token];
        }
        if face.dicts.len() != declared_dicts
            || face.gd_bytes == 0
            || face.gd_bytes > 4
            || face.fd_bytes > 4
            || face.count == 0
        {
            return None;
        }
        let entry = face.fd_bytes + face.gd_bytes;
        let map_len = (face.count as usize).checked_add(1)?.checked_mul(entry)?;
        face.data
            .get(face.map_offset..face.map_offset.checked_add(map_len)?)?;
        let mut retained_subr_bytes = 0usize;
        for dict in &mut face.dicts {
            if dict.sd_bytes > 4 || dict.subr_offset > face.data.len() {
                return None;
            }
            if dict.subr_count == 0 {
                continue;
            }
            if dict.sd_bytes == 0 {
                return None;
            }
            let map_len = dict.subr_count.checked_add(1)?.checked_mul(dict.sd_bytes)?;
            let map = face
                .data
                .get(dict.subr_offset..dict.subr_offset.checked_add(map_len)?)?;
            let start = offset(map.get(..dict.sd_bytes)?)?;
            let end = offset(map.get(map.len() - dict.sd_bytes..)?)?;
            retained_subr_bytes = retained_subr_bytes.checked_add(end.checked_sub(start)?)?;
            // Match native's overlap guard, bounding copies across FDs.
            if retained_subr_bytes / 4 > face.data.len() {
                return None;
            }
            for pair in map.windows(2 * dict.sd_bytes).step_by(dict.sd_bytes) {
                let start = offset(&pair[..dict.sd_bytes])?;
                let end = offset(&pair[dict.sd_bytes..])?;
                dict.subrs
                    .push(decrypt(face.data.get(start..end)?, dict.len_iv)?.into_owned());
            }
        }
        Some(face)
    }

    fn glyph(&self, cid: u32) -> Option<(&FontDict, &[u8])> {
        if cid >= self.count {
            return None;
        }
        let entry = self.fd_bytes + self.gd_bytes;
        let pos = self
            .map_offset
            .checked_add((cid as usize).checked_mul(entry)?)?;
        let entries = self.data.get(pos..pos.checked_add(2 * entry)?)?;
        let dict = self.dicts.get(offset(&entries[..self.fd_bytes])?)?;
        let start = offset(&entries[self.fd_bytes..entry])?;
        let end = offset(&entries[entry + self.fd_bytes..])?;
        Some((dict, self.data.get(start..end)?))
    }

    pub fn has_outline(&self, cid: u32) -> bool {
        self.glyph(cid).is_some()
    }

    pub fn draw(&self, cid: u32, pen: &mut impl OutlinePen) -> Option<Option<f32>> {
        let (dict, bytes) = self.glyph(cid)?;
        if bytes.is_empty() {
            return Some(None);
        }
        // Decrypt this glyph only for the active draw. It is not a retained
        // outline/charstring cache; unencrypted data stays borrowed.
        let bytes = decrypt(bytes, dict.len_iv)?;
        let mut filtered = NopFilterSink::new(pen);
        let mut sink = TransformSink::new(&mut filtered, dict.transform);
        let width = charstring::evaluate(dict, None, &bytes, &mut sink).ok()?;
        Some(width.map(|w| dict.transform.transform_h_metric(w).to_f32().max(0.0)))
    }

    pub fn glyph_count(&self) -> u32 {
        self.count
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn family(&self) -> &str {
        if self.family.is_empty() {
            &self.name
        } else {
            &self.family
        }
    }
    pub fn weight(&self) -> &str {
        &self.weight
    }
    pub fn is_italic(&self) -> bool {
        self.italic
    }
    pub fn is_fixed_pitch(&self) -> bool {
        self.fixed_pitch
    }
    pub fn bbox(&self) -> (f32, f32, f32, f32) {
        let [a, b, c, d] = self.bbox;
        (a, b, c, d)
    }
}

fn offset(bytes: &[u8]) -> Option<usize> {
    bytes
        .iter()
        .try_fold(0usize, |n, &b| n.checked_mul(256)?.checked_add(b as usize))
}

fn decrypt(input: &[u8], len_iv: i32) -> Option<Cow<'_, [u8]>> {
    if len_iv < 0 {
        return Some(Cow::Borrowed(input));
    }
    let skip = usize::try_from(len_iv).ok()?;
    input.get(..skip)?;
    let mut key = 4330u16;
    let mut output = Vec::with_capacity(input.len() - skip);
    for (index, &byte) in input.iter().enumerate() {
        let decoded = byte ^ (key >> 8) as u8;
        key = u16::from(byte)
            .wrapping_add(key)
            .wrapping_mul(52845)
            .wrapping_add(22719);
        if index >= skip {
            output.push(decoded);
        }
    }
    Some(Cow::Owned(output))
}

fn integer(token: &[u8]) -> Option<usize> {
    std::str::from_utf8(token).ok()?.parse().ok()
}

fn number(token: &[u8]) -> Option<f32> {
    let n: f32 = std::str::from_utf8(token).ok()?.parse().ok()?;
    n.is_finite().then_some(n)
}

fn text(token: &[u8]) -> Option<String> {
    let bytes = if token.first() == Some(&b'/') {
        &token[1..]
    } else {
        token.strip_prefix(b"(")?.strip_suffix(b")")?
    };
    Some(String::from_utf8_lossy(bytes).into_owned())
}

fn array<const N: usize>(tokens: &mut Tokens<'_>) -> Option<[f32; N]> {
    if tokens.next()? != b"[" {
        return None;
    }
    let mut values = [0.0; N];
    for value in &mut values {
        *value = number(tokens.next()?)?;
    }
    (tokens.next()? == b"]").then_some(values)
}

/// Lexical tokens only. Skip comments, strings and procedures as units so
/// keywords inside them cannot become dictionary entries or StartData.
struct Tokens<'a> {
    input: &'a [u8],
    pos: usize,
}

impl<'a> Tokens<'a> {
    fn next(&mut self) -> Option<&'a [u8]> {
        let token = self.word()?;
        if token != b"{" {
            return Some(token);
        }
        let start = self.pos - 1;
        let mut depth = 1usize;
        while depth != 0 {
            match self.word()? {
                b"{" => depth += 1,
                b"}" => depth -= 1,
                _ => {}
            }
        }
        Some(&self.input[start..self.pos])
    }

    fn word(&mut self) -> Option<&'a [u8]> {
        loop {
            let byte = *self.input.get(self.pos)?;
            if byte.is_ascii_whitespace() {
                self.pos += 1;
                continue;
            }
            let start = self.pos;
            self.pos += 1;
            match byte {
                b'%' => {
                    while self
                        .input
                        .get(self.pos)
                        .is_some_and(|b| !matches!(b, b'\r' | b'\n'))
                    {
                        self.pos += 1;
                    }
                    if self.input[start..self.pos].starts_with(b"%ADOBeginFontDict") {
                        return Some(b"%ADOBeginFontDict");
                    }
                    continue;
                }
                b'(' => {
                    let mut depth = 1usize;
                    while depth != 0 {
                        match *self.input.get(self.pos)? {
                            b'\\' => {
                                self.pos += 1;
                                self.input.get(self.pos)?;
                            }
                            b'(' => depth += 1,
                            b')' => depth -= 1,
                            _ => {}
                        }
                        self.pos += 1;
                    }
                }
                b'[' | b']' | b'{' | b'}' => {}
                b'<' if self.input.get(self.pos) == Some(&b'<') => self.pos += 1,
                b'>' if self.input.get(self.pos) == Some(&b'>') => self.pos += 1,
                b'<' => {
                    while *self.input.get(self.pos)? != b'>' {
                        self.pos += 1;
                    }
                    self.pos += 1;
                }
                _ => {
                    while self
                        .input
                        .get(self.pos)
                        .is_some_and(|b| !b.is_ascii_whitespace() && !b"/()<>[]{}%".contains(b))
                    {
                        self.pos += 1;
                    }
                }
            }
            return Some(&self.input[start..self.pos]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::fx_coordinates::{CfxMatrix, FxRect},
        fxge::{cfx_face::CfxFace, fx_font::FontAntiAliasingMode},
    };

    fn encrypt(bytes: &[u8], len_iv: i32) -> Vec<u8> {
        if len_iv < 0 {
            return bytes.to_vec();
        }
        let mut key = 4330u16;
        std::iter::repeat_n(0u8, len_iv as usize)
            .chain(bytes.iter().copied())
            .map(|byte| {
                let encoded = byte ^ (key >> 8) as u8;
                key = u16::from(encoded)
                    .wrapping_add(key)
                    .wrapping_mul(52845)
                    .wrapping_add(22719);
                encoded
            })
            .collect()
    }

    // Original synthetic font: empty CID 0, triangular CIDs 1/2 in separate
    // FDs, with a callsubr and independent affine matrices. No external font.
    fn fixture(len_iv: i32, hex: bool) -> Vec<u8> {
        let glyph = encrypt(&[139, 248, 136, 13, 139, 10, 14], len_iv);
        let subr = encrypt(
            &[139, 139, 21, 248, 136, 139, 5, 252, 136, 249, 80, 5, 9, 11],
            len_iv,
        );
        let first = 12;
        let second = first + glyph.len();
        let subr_map = second + glyph.len();
        let second_subr_map = subr_map + 4 + subr.len();
        let mut binary = Vec::new();
        for (fd, offset) in [(0, first), (0, first), (1, second), (0, subr_map)] {
            binary.push(fd);
            binary.extend_from_slice(&(offset as u16).to_be_bytes());
        }
        binary.extend_from_slice(&glyph);
        binary.extend_from_slice(&glyph);
        for map in [subr_map, second_subr_map] {
            binary.extend_from_slice(&((map + 4) as u16).to_be_bytes());
            binary.extend_from_slice(&((map + 4 + subr.len()) as u16).to_be_bytes());
            binary.extend_from_slice(&subr);
        }
        let mut output = format!(
            "%!PS-Adobe-3.0 Resource-CIDFont\n\
             % StartData and /CIDCount 999 in a comment are inert\n\
             /Notice (escaped \\) StartData /CIDCount 888) def\n\
             /Unused {{ {{ /CIDCount 777 }} (StartData) }} def\n\
             /UnusedHex <2f434944436f756e74> def\n\
             /CIDFontName /SyntheticCID def /FamilyName (Synthetic CID) def\n\
             /Weight (Regular) def /CIDFontType 0 def\n\
             /FontBBox [0 0 1010 700] def\n\
             /CIDMapOffset 0 def /FDBytes 1 def /GDBytes 2 def /CIDCount 3 def\n\
             /FDArray 2 array\n%ADOBeginFontDict\n\
             /FontMatrix [0.001 0 0 0.001 0 0] def /lenIV {len_iv} def\n\
             /SubrMapOffset {subr_map} def /SDBytes 2 def /SubrCount 1 def\n\
             %ADOBeginFontDict trailing producer comment\n\
             /FontMatrix [0.002 0 0 0.001 0.010 0] def /lenIV {len_iv} def\n\
             /SubrMapOffset {second_subr_map} def /SDBytes 2 def /SubrCount 1 def\n\
             ({}) {} StartData ",
            if hex { "Hex" } else { "Binary" },
            binary.len()
        )
        .into_bytes();
        if hex {
            for byte in binary {
                output.extend_from_slice(format!("{byte:02X} ").as_bytes());
            }
        } else {
            output.extend_from_slice(&binary);
        }
        output
    }

    #[test]
    fn cid_type1_embedded_outlines_subrs_and_fd_transforms() {
        for len_iv in [-1, 0, 1, 4] {
            for hex in [false, true] {
                let data = fixture(len_iv, hex);
                let face = CfxFace::new(&data, 0).unwrap();
                assert_eq!(face.family_name(), "Synthetic CID");
                assert_eq!(face.postscript_name(), "SyntheticCID");
                assert_eq!(face.glyph_count(), 3);
                assert_eq!(face.units_per_em(), 1000);
                assert!(!face.is_tricky());
                assert_eq!(face.charmap_count(), 0);
                assert!(!face.select_charmap(super::super::fx_fontencoding::FontEncoding::Unicode));
                assert!(!face.has_glyph_names());
                assert_eq!(face.glyph_width(1, 0, None), 500);
                assert_eq!(face.glyph_width(2, 0, None), 1010);
                assert_eq!(face.font_glyph_bbox(1), Some(FxRect::new(0, 0, 500, 700)));
                assert_eq!(face.font_glyph_bbox(2), Some(FxRect::new(10, 0, 1010, 700)));
                for cid in [1, 2] {
                    assert!(face.load_glyph_path(cid, 0, false, None).is_some());
                    let bitmap = face
                        .render_glyph(
                            cid,
                            false,
                            false,
                            CfxMatrix::new(24.0, 0.0, 0.0, 24.0, 0.0, 0.0),
                            0,
                            FontAntiAliasingMode::Normal,
                            None,
                        )
                        .unwrap();
                    assert!(bitmap.bitmap().buffer().iter().any(|&b| b != 0));
                }
                assert_eq!(face.glyph_width(0, 0, None), 0);
                assert!(face.load_glyph_path(3, 0, false, None).is_none());
            }
        }
    }

    #[test]
    fn cid_type1_bad_maps_and_truncation_fail_without_losing_other_glyphs() {
        let data = fixture(1, false);
        // Every truncated prefix, including partial binary maps/subrs, fails
        // cleanly without reserving memory from an untrusted declared count.
        for end in 0..data.len() {
            assert!(CidType1Face::new(&data[..end]).is_none(), "prefix {end}");
        }
        for (from, to) in [
            ("/GDBytes 2", "/GDBytes 0"),
            ("/FDBytes 1", "/FDBytes 5"),
            ("/CIDCount 3", "/CIDCount 4294967295"),
            ("/FDArray 2", "/FDArray 4294967295"),
            ("/SDBytes 2", "/SDBytes 0"),
            ("/SubrCount 1", "/SubrCount 4294967295"),
            ("/CIDFontType 0", "/CIDFontType 2"),
            ("0.001 0 0 0.001", "0 0 0 0"),
        ] {
            let original = CidType1Face::new(&data).unwrap();
            let header_len = data.len() - original.data.len();
            let header = std::str::from_utf8(&data[..header_len]).unwrap();
            let mut modified = header.replace(from, to).into_bytes();
            modified.extend_from_slice(&data[header_len..]);
            assert!(CidType1Face::new(&modified).is_none(), "{from}");
        }
        let mut face = CidType1Face::new(&data).unwrap();
        assert!(matches!(face.data, Cow::Borrowed(_)));
        assert!(matches!(decrypt(&[1, 2, 3], -1), Some(Cow::Borrowed(_))));
        assert!(decrypt(&[1, 2, 3], 4).is_none());
        // A bad FD selection is local to that CID, including native sentinels.
        face.data.to_mut()[6] = 0xff;
        let mut pen = TestPen;
        assert!(face.draw(1, &mut pen).is_some());
        assert!(face.draw(2, &mut pen).is_none());
        assert!(!face.has_outline(2));
        // Descending glyph offsets also fail locally.
        face.data.to_mut()[7..9].copy_from_slice(&0xffffu16.to_be_bytes());
        assert!(face.draw(1, &mut pen).is_none());
        assert!(face.draw(u32::MAX, &mut pen).is_none());
    }

    struct TestPen;
    impl OutlinePen for TestPen {
        fn move_to(&mut self, _: f32, _: f32) {}
        fn line_to(&mut self, _: f32, _: f32) {}
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
        fn close(&mut self) {}
    }
}
