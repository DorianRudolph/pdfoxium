// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Phase 1 Unicode resolver from `core/fxge/cfx_charmap_resolver.{h,cpp}`.
//! The alternate resolver is XFA only.

use crate::fxge::cfx_face::CfxFace;
use crate::fxge::fx_fontencoding::FontEncoding;

pub struct CfxCharmapResolver<'face, 'data> {
    face: Option<&'face mut CfxFace<'data>>,
    symbolic_substitution: bool,
}

impl<'face, 'data> CfxCharmapResolver<'face, 'data> {
    pub fn create_unicode(
        face: Option<&'face mut CfxFace<'data>>,
        symbolic_substitution: bool,
    ) -> Self {
        Self {
            face,
            symbolic_substitution,
        }
    }

    pub fn glyph_from_char_code(&mut self, charcode: u32) -> u32 {
        let Some(face) = self.face.as_deref_mut() else {
            return charcode;
        };
        if face.select_charmap(FontEncoding::Unicode) {
            return face.get_char_index(charcode);
        }
        if self.symbolic_substitution {
            let mut index = 0;
            if face.select_charmap(FontEncoding::Symbol) {
                index = face.get_char_index(charcode);
            }
            if index == 0 && face.select_charmap(FontEncoding::AppleRoman) {
                return face.get_char_index(charcode);
            }
        }
        charcode
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_resolver_selects_cmap_and_preserves_no_face_fallback() {
        let data =
            include_bytes!("../../../../upstream/pdfium/testing/resources/fonts/ahem/Ahem.ttf");
        let mut face = CfxFace::new(data, 0).unwrap();
        let expected = face.unicode_char_index(u32::from('A'));
        let mut resolver = CfxCharmapResolver::create_unicode(Some(&mut face), false);
        assert_eq!(resolver.glyph_from_char_code(u32::from('A')), expected);
        assert_eq!(face.current_charmap_encoding(), FontEncoding::Unicode);

        let mut no_face = CfxCharmapResolver::create_unicode(None, false);
        assert_eq!(no_face.glyph_from_char_code(0x1234), 0x1234);
    }
}
