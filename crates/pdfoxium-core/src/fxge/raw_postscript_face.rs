// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Raw CFF and Type 1 operations from `core/fxge/skrifa/src/main.rs` for
//! the Fontations-backed `CFX_Face`.

use read_fonts::{
    ps::{
        cff::{
            charset::{Charset, CharsetKind, CustomCharset},
            CffFontRef, Subfont,
        },
        type1::{Type1Font, Type1Instance},
        Sid, UnicodeCharmap,
    },
    types::{Fixed, GlyphId},
};
use skrifa::outline::OutlinePen;

use super::cid_type1_face::CidType1Face;

#[derive(yoke::Yokeable)]
enum RawFont<'a> {
    Cff(CffFontRef<'a>),
    Type1(Type1Font),
    CidType1(CidType1Face<'a>),
}

#[derive(yoke::Yokeable)]
pub struct RawPostScriptFace<'a> {
    font: RawFont<'a>,
    unicode_charmap: Option<UnicodeCharmap>,
    subfonts: Vec<Option<Subfont>>,
    // A CID is u16, so the count is at most 65536. Reuse the high bit for
    // the sorted-prefix proof without growing this retained font object.
    cid_count: Option<u32>,
}

const SORTED_CID_FLAG: u32 = 1 << 31;

// Fontations' format-0 reverse lookup scans the complete stored SID array.
// Large CID CFF subsets can order their used CIDs strictly by value. Prove
// that for the declared glyph prefix before using its borrowed array directly.
fn has_sorted_cids(charset: &Charset<'_>, glyph_count: u32) -> bool {
    let CharsetKind::Custom(CustomCharset::Format0(format)) = charset.kind() else {
        return false;
    };
    glyph_count != 0
        && format
            .glyph()
            .get(..glyph_count.saturating_sub(1) as usize)
            .is_some_and(|ids| {
                ids.first().is_none_or(|id| id.get() != 0)
                    && ids.windows(2).all(|pair| pair[0].get() < pair[1].get())
            })
}

fn map_cff_cid(charset: &Charset<'_>, glyph_count: u32, sorted: bool, cid: u16) -> Option<u32> {
    if sorted {
        if cid == 0 {
            return Some(0);
        }
        if let CharsetKind::Custom(CustomCharset::Format0(format)) = charset.kind() {
            let ids = format
                .glyph()
                .get(..glyph_count.saturating_sub(1) as usize)?;
            if let Ok(index) = ids.binary_search_by_key(&cid, |id| id.get()) {
                return Some(index as u32 + 1);
            }
        }
    }
    // Keep Fontations' first-match, truncation and malformed trailing-data
    // behavior. In particular, do not turn a binary-search miss into failure.
    charset.glyph_id(Sid::new(cid)).map(GlyphId::to_u32)
}

impl<'a> RawPostScriptFace<'a> {
    pub fn from_cff(data: &'a [u8], face_index: u32) -> Option<Self> {
        let font = CffFontRef::new(data, face_index, None).ok()?;
        let charset = font.charset();
        let unicode_charmap =
            charset.as_ref().map(|charset| {
                UnicodeCharmap::from_glyph_names(charset.iter().filter_map(|(gid, sid)| {
                    Some((gid, std::str::from_utf8(font.string(sid)?).ok()?))
                }))
            });
        let cid_count = if font.is_cid() {
            charset.as_ref().and_then(|charset| {
                charset
                    .iter()
                    .filter(|(gid, _)| gid.to_u32() < font.num_glyphs())
                    .map(|(_, cid)| u32::from(cid.to_u16()))
                    .max()
                    .map(|max_cid| max_cid + 1)
            })
        } else {
            None
        };
        let sorted_cids = font.is_cid()
            && charset
                .as_ref()
                .is_some_and(|charset| has_sorted_cids(charset, font.num_glyphs()));
        let cid_count =
            cid_count.map(|count| count | if sorted_cids { SORTED_CID_FLAG } else { 0 });
        let subfonts = (0..font.num_subfonts())
            .map(|index| font.subfont(index, &[]).ok())
            .collect();
        Some(Self {
            font: RawFont::Cff(font),
            unicode_charmap,
            subfonts,
            cid_count,
        })
    }

    pub fn from_cid_type1(data: &'a [u8]) -> Option<Self> {
        Some(Self {
            font: RawFont::CidType1(CidType1Face::new(data)?),
            unicode_charmap: None,
            subfonts: Vec::new(),
            cid_count: None,
        })
    }

    pub fn from_type1(data: &[u8]) -> Option<Self> {
        let font = Type1Font::new(data).ok()?;
        Some(Self {
            font: RawFont::Type1(font),
            unicode_charmap: None,
            subfonts: Vec::new(),
            cid_count: None,
        })
    }

    pub fn is_cff(&self) -> bool {
        matches!(self.font, RawFont::Cff(_))
    }
    pub fn is_cid_type1(&self) -> bool {
        matches!(self.font, RawFont::CidType1(_))
    }
    pub fn upem(&self) -> u32 {
        let value = match &self.font {
            RawFont::Cff(font) => font.upem(),
            RawFont::Type1(font) => font.upem(),
            RawFont::CidType1(_) => 1000,
        };
        // Unlike SFNT's unitsPerEm, a PostScript FontMatrix can describe more
        // than 65535 units per em. Keep Fontations' full scale so normalization
        // preserves the valid transform instead of panicking or wrapping it.
        u32::try_from(value).unwrap_or_default()
    }
    pub fn glyph_count(&self) -> u32 {
        match &self.font {
            RawFont::Cff(font) => self
                .cid_count
                .map(|count| count & !SORTED_CID_FLAG)
                .unwrap_or_else(|| font.num_glyphs()),
            RawFont::Type1(font) => font.num_glyphs(),
            RawFont::CidType1(font) => font.glyph_count(),
        }
    }
    pub fn unicode_char_index(&self, code: u32) -> u32 {
        let mapped = match &self.font {
            RawFont::Type1(font) => font.unicode_charmap().map(code),
            RawFont::CidType1(_) => None,
            RawFont::Cff(_) => self
                .unicode_charmap
                .as_ref()
                .and_then(|charmap| charmap.map(code)),
        };
        mapped.map_or(0, GlyphId::to_u32)
    }
    pub fn synthesized_unicode(&self) -> Option<&UnicodeCharmap> {
        let map = match &self.font {
            RawFont::Type1(font) => Some(font.unicode_charmap()),
            RawFont::Cff(_) if self.has_glyph_names() => self.unicode_charmap.as_ref(),
            RawFont::Cff(_) | RawFont::CidType1(_) => None,
        };
        map.filter(|map| map.iter().next().is_some())
    }
    pub fn has_encoding(&self) -> bool {
        match &self.font {
            RawFont::Cff(font) => font.encoding().is_some(),
            RawFont::Type1(font) => font.encoding().is_some(),
            RawFont::CidType1(_) => false,
        }
    }
    pub fn has_glyph_names(&self) -> bool {
        match &self.font {
            RawFont::Cff(font) => font.charset().is_some() && !font.is_cid(),
            RawFont::Type1(_) => true,
            RawFont::CidType1(_) => false,
        }
    }
    pub fn encoded_char_index(&self, code: u32) -> u32 {
        let Ok(code) = u8::try_from(code) else {
            return 0;
        };
        let gid = match &self.font {
            RawFont::Cff(font) => font.encoding().and_then(|encoding| encoding.map(code)),
            RawFont::Type1(font) => font.encoding().and_then(|encoding| encoding.map(code)),
            RawFont::CidType1(_) => None,
        };
        let gid = gid.map_or(0, GlyphId::to_u32);
        if gid < self.glyph_count() {
            gid
        } else {
            0
        }
    }
    pub fn glyph_name(&self, gid: u32) -> Option<&str> {
        match &self.font {
            RawFont::Cff(font) => {
                if font.is_cid() {
                    return None;
                }
                let sid = font.charset()?.string_id(GlyphId::new(gid))?;
                std::str::from_utf8(font.string(sid)?).ok()
            }
            RawFont::Type1(font) => font.glyph_name(GlyphId::new(gid)),
            RawFont::CidType1(_) => None,
        }
    }
    pub fn name_index(&self, name: &str) -> u32 {
        match &self.font {
            RawFont::Cff(font) => font
                .charset()
                .and_then(|charset| {
                    charset
                        .iter()
                        .find(|(_, sid)| font.string(*sid) == Some(name.as_bytes()))
                        .map(|(gid, _)| gid.to_u32())
                })
                .unwrap_or(0),
            RawFont::Type1(font) => font
                .glyph_names()
                .find(|(_, glyph_name)| *glyph_name == name)
                .map_or(0, |(gid, _)| gid.to_u32()),
            RawFont::CidType1(_) => 0,
        }
    }
    fn map_gid(&self, gid: u32) -> Option<u32> {
        match &self.font {
            RawFont::Cff(font) if font.is_cid() => map_cff_cid(
                &font.charset()?,
                font.num_glyphs(),
                self.cid_count
                    .is_some_and(|count| count & SORTED_CID_FLAG != 0),
                gid as u16,
            ),
            _ => Some(gid),
        }
    }
    pub fn has_outline(&self, gid: u32) -> bool {
        match &self.font {
            RawFont::Cff(font) => self.map_gid(gid).is_some_and(|gid| gid < font.num_glyphs()),
            RawFont::Type1(font) => gid < font.num_glyphs(),
            RawFont::CidType1(font) => font.has_outline(gid),
        }
    }
    pub fn draw(&self, gid: u32, pen: &mut impl OutlinePen) -> Option<Option<f32>> {
        let gid = GlyphId::new(self.map_gid(gid)?);
        match &self.font {
            RawFont::Cff(font) => {
                let subfont = self
                    .subfonts
                    .get(font.subfont_index(gid)? as usize)?
                    .as_ref()?;
                font.draw(subfont, gid, &[], None, pen).ok()
            }
            RawFont::Type1(font) => font.draw(gid, None, pen).ok(),
            RawFont::CidType1(font) => font.draw(gid.to_u32(), pen),
        }
    }
    pub fn has_multiple_master(&self) -> bool {
        matches!(&self.font, RawFont::Type1(font)
            if font.multiple_master().is_some_and(|mm| mm.axes().len() == 2 && mm.num_masters() == 4))
    }

    /// PDFium CFX_Face::AdjustVariationParams(), for the two-axis generic
    /// substitutes. Instance weights live on the caller's stack; the shared
    /// face and its default instance remain unchanged.
    fn adjusted_instance<'b>(
        &'b self,
        gid: u32,
        dest_width: i32,
        weight: i32,
        weights: &'b mut [Fixed; 4],
    ) -> Option<Type1Instance<'b>> {
        let RawFont::Type1(font) = &self.font else {
            return None;
        };
        let mm = font.multiple_master()?;
        let axes = mm.axes();
        // PDFium's generic substitutes have Weight and Width, four masters.
        if axes.len() != 2 || mm.num_masters() != weights.len() {
            return None;
        }
        let mut blend = [Fixed::ZERO; 2];
        mm.default_blend(&mut blend);
        let weight = if weight == 0 {
            axes[0].blend_to_design(blend[0])
        } else {
            weight
        };
        blend[0] = axes[0].design_to_blend(weight);
        if dest_width == 0 {
            blend[1] = axes[1].design_to_blend(axes[1].blend_to_design(blend[1]));
        } else {
            let min_param = axes[1].min_design();
            let max_param = axes[1].max_design();
            let upem = i64::from(font.upem());
            if upem == 0 {
                return None;
            }
            let mut width_at = |param| {
                blend[1] = axes[1].design_to_blend(param);
                mm.blend_to_weights(&blend, weights);
                let width = font
                    .instance(weights)
                    .evaluate_width(GlyphId::new(gid))
                    .ok()??;
                // FT_LOAD_NO_SCALE exposes an integer font-unit advance.
                Some(i64::from(width.to_bits() >> 16) * 1000 / upem)
            };
            let min_width = width_at(min_param)?;
            let max_width = width_at(max_param)?;
            // FreeType is left at the maximum-width instance on this return.
            let param = if min_width == max_width {
                max_param
            } else {
                let param = i128::from(min_param)
                    + (i128::from(max_param) - i128::from(min_param))
                        * (i128::from(dest_width) - i128::from(min_width))
                        / (i128::from(max_width) - i128::from(min_width));
                param.clamp(i128::from(i32::MIN), i128::from(i32::MAX)) as i32
            };
            blend[1] = axes[1].design_to_blend(param);
        }
        mm.blend_to_weights(&blend, weights);
        Some(font.instance(weights))
    }

    pub fn draw_with_variation(
        &self,
        gid: u32,
        dest_width: i32,
        weight: i32,
        pen: &mut impl OutlinePen,
    ) -> Option<Option<f32>> {
        let mut weights = [Fixed::ZERO; 4];
        self.adjusted_instance(gid, dest_width, weight, &mut weights)?
            .draw(GlyphId::new(gid), None, pen)
            .ok()
    }

    pub fn width_with_variation(&self, gid: u32, dest_width: i32, weight: i32) -> Option<i32> {
        let mut weights = [Fixed::ZERO; 4];
        let width = self
            .adjusted_instance(gid, dest_width, weight, &mut weights)?
            .evaluate_width(GlyphId::new(gid))
            .ok()??;
        let upem = i64::from(self.upem());
        (upem != 0).then(|| (i64::from(width.to_bits() >> 16) * 1000 / upem) as i32)
    }

    pub fn family_name(&self) -> &str {
        match &self.font {
            RawFont::Cff(font) => font
                .metadata()
                .and_then(|m| m.family_name())
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| self.postscript_name()),
            RawFont::Type1(font) => font.family_name().unwrap_or(""),
            RawFont::CidType1(font) => font.family(),
        }
    }
    pub fn postscript_name(&self) -> &str {
        match &self.font {
            RawFont::Cff(font) => font.metadata().and_then(|m| m.name()).unwrap_or(""),
            RawFont::Type1(font) => font.name().unwrap_or(""),
            RawFont::CidType1(font) => font.name(),
        }
    }
    pub fn is_fixed_width(&self) -> bool {
        match &self.font {
            RawFont::Cff(font) => font.metadata().is_some_and(|m| m.is_fixed_pitch()),
            RawFont::Type1(font) => font.is_fixed_pitch(),
            RawFont::CidType1(font) => font.is_fixed_pitch(),
        }
    }
    pub fn is_italic(&self) -> bool {
        match &self.font {
            RawFont::Cff(font) => font
                .metadata()
                .is_some_and(|m| m.italic_angle().to_f32() != 0.0),
            RawFont::Type1(font) => font.italic_angle() != 0,
            RawFont::CidType1(font) => font.is_italic(),
        }
    }
    pub fn is_bold(&self) -> bool {
        let weight = match &self.font {
            RawFont::Cff(font) => font.metadata().and_then(|meta| meta.weight()),
            RawFont::Type1(font) => font.weight(),
            RawFont::CidType1(font) => Some(font.weight()),
        };
        weight.is_some_and(|weight| {
            weight.eq_ignore_ascii_case("bold") || weight.eq_ignore_ascii_case("black")
        })
    }
    pub fn bbox(&self) -> Option<(f32, f32, f32, f32)> {
        let bounds = match &self.font {
            RawFont::Cff(font) => font.metadata().map(|m| m.bbox()),
            RawFont::Type1(font) => Some(font.bbox()),
            RawFont::CidType1(font) => return Some(font.bbox()),
        };
        bounds.map(|b| {
            (
                b.x_min.to_f32(),
                b.y_min.to_f32(),
                b.x_max.to_f32(),
                b.y_max.to_f32(),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)]
    struct CountingPen(usize);
    impl OutlinePen for CountingPen {
        fn move_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn line_to(&mut self, _: f32, _: f32) {
            self.0 += 1;
        }
        fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
            self.0 += 1;
        }
        fn close(&mut self) {}
    }

    #[test]
    fn sorted_cid_search_preserves_first_matches_missing_and_truncated_charsets() {
        use read_fonts::FontData;
        for (ids, count, sorted) in [
            (vec![], 1, true),
            (vec![1, 2, 40, 500, 65535], 6, true),
            (vec![1, 3, 100], 3, true),
            (vec![1, 3], 4, false),
            (vec![10, 1, 20], 4, false),
            (vec![1, 1, 2], 4, false),
            (vec![0, 3], 3, false),
            (vec![], 0, false),
        ] {
            let mut data = vec![0; 4]; // format 0 at offset 3
            for cid in ids {
                data.extend_from_slice(&u16::to_be_bytes(cid));
            }
            let charset = Charset::new(FontData::new(&data), 3, count).unwrap();
            assert_eq!(has_sorted_cids(&charset, count), sorted);
            for cid in 0..=u16::MAX {
                assert_eq!(
                    map_cff_cid(&charset, count, sorted, cid),
                    charset.glyph_id(Sid::new(cid)).map(GlyphId::to_u32),
                    "count={count} sorted={sorted} cid={cid}"
                );
            }
        }
        // Fontations searches trailing format-0 data before narrowing the
        // position to u16. Even a match beyond the glyph prefix can wrap to
        // an accepted glyph; preserve that existing malformed-font behavior.
        let mut data = vec![0; 4];
        for i in 0..=65536 {
            let cid = match i {
                0 => 1u16,
                1 => 3,
                65536 => 42,
                _ => 65535,
            };
            data.extend_from_slice(&cid.to_be_bytes());
        }
        let charset = Charset::new(FontData::new(&data), 3, 3).unwrap();
        assert!(has_sorted_cids(&charset, 3));
        assert_eq!(map_cff_cid(&charset, 3, true, 42), Some(1));
        for cid in [0, 1, 2, 3, 42, 65535] {
            assert_eq!(
                map_cff_cid(&charset, 3, true, cid),
                charset.glyph_id(Sid::new(cid)).map(GlyphId::to_u32)
            );
        }
        // Range charsets retain Fontations' general path.
        for format in [1, 2] {
            let data = [0, 0, 0, format, 0, 1, 0, 1];
            let charset = Charset::new(FontData::new(&data), 3, 3).unwrap();
            assert!(!has_sorted_cids(&charset, 3));
            for cid in [0, 1, 2, 3, 65535] {
                assert_eq!(
                    map_cff_cid(&charset, 3, false, cid),
                    charset.glyph_id(Sid::new(cid)).map(GlyphId::to_u32)
                );
            }
        }
    }

    #[test]
    fn raw_cff_maps_and_draws_embedded_glyph() {
        let data = include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../upstream/pdfium/testing/resources/fonts/foxit_fixed.cff"
        ));
        let face = RawPostScriptFace::from_cff(data, 0).unwrap();
        let gid = face.unicode_char_index('U' as u32);
        assert!(gid > 0);
        let mut pen = CountingPen::default();
        assert!(face.draw(gid, &mut pen).is_some());
        assert!(pen.0 > 0);
        assert!(
            face.draw(gid, &mut CountingPen::default())
                .unwrap()
                .unwrap()
                > 0.0
        );
    }
}
