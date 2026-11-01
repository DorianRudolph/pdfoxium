// Public interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::prelude::*;
use pdfoxium_core::{
    fxcrt::fx_codepage::FxCharset,
    fxge::{
        cfx_gemodule::{initialize_ge_module, with_ge_module},
        systemfontinfo_iface::SystemFontInfo,
    },
};
use std::{
    collections::HashMap,
    io::{Read, Seek},
    path::Path,
};
pub struct PdfiumCustomFontProviderRequest {
    /// The font face of the requested custom font.
    pub font_face: String,

    /// The character set of the requested custom font.
    pub character_set: PdfFontCharacterSet,

    /// The weight of the requested custom font.
    pub weight: PdfFontWeight,

    /// `true` if the glyphs in the requested custom font should include dominant vertical
    /// strokes that are slanted.
    pub is_italic: bool,

    /// `true` if all the glyphs in the requested custom font should have the same width.
    pub is_fixed_pitch: bool,

    /// `true` if the glyphs in the requested custom font should have serifs - short strokes
    /// drawn at an angle on the top or bottom of glyph stems to decorate the glyphs.
    /// For example, Times New Roman is a serif font.
    pub is_serif: bool,

    /// `true` if the glyphs in the requested custom font should be designed to resemble
    /// cursive handwriting.
    pub is_cursive: bool,
}
pub struct PdfiumCustomFontProviderResponse {
    /// A unique ID for the custom font provided in this response. Pdfium will use this
    /// value as a font handle in all subsequent calls related to this font.
    pub id: PdfiumCustomFontHandle,

    /// The font face of the custom font provided in this response.
    pub font_face: String,

    /// The character set of the custom font provided in this response.
    pub character_set: PdfFontCharacterSet,

    /// The raw font byte data for the custom font provided in this response, in either
    /// OpenType or TrueType format.
    pub data: Vec<u8>,
}
pub trait PdfiumCustomFontProvider: Send + Sync {
    /// Responds to a single custom font lookup request from Pdfium, returning either a valid
    /// response if the font is available or `None` if the font is not available to this
    /// provider implementation.
    fn provide(
        &mut self,
        request: PdfiumCustomFontProviderRequest,
    ) -> Option<PdfiumCustomFontProviderResponse>;
}
pub type PdfiumCustomFontHandle = u64;
struct FontProvider {
    provider: Box<dyn PdfiumCustomFontProvider>,
    cache: HashMap<PdfiumCustomFontHandle, PdfiumCustomFontProviderResponse>,
}
fn charset(value: FxCharset) -> Option<PdfFontCharacterSet> {
    Some(match value {
        FxCharset::Ansi => PdfFontCharacterSet::Ansi,
        FxCharset::Default => PdfFontCharacterSet::Default,
        FxCharset::Symbol => PdfFontCharacterSet::Symbol,
        FxCharset::ShiftJis => PdfFontCharacterSet::JapaneseShiftJis,
        FxCharset::Hangul => PdfFontCharacterSet::KoreanHangul,
        FxCharset::ChineseSimplified => PdfFontCharacterSet::ChineseGb2312,
        FxCharset::ChineseTraditional => PdfFontCharacterSet::ChineseBig5,
        FxCharset::MswinGreek => PdfFontCharacterSet::Greek,
        FxCharset::MswinVietnamese => PdfFontCharacterSet::Vietnamese,
        FxCharset::MswinHebrew => PdfFontCharacterSet::Hebrew,
        FxCharset::MswinArabic => PdfFontCharacterSet::Arabic,
        FxCharset::MswinCyrillic => PdfFontCharacterSet::Cyrillic,
        FxCharset::Thai => PdfFontCharacterSet::Thai,
        FxCharset::MswinEasternEuropean => PdfFontCharacterSet::EasternEuropean,
        _ => return None,
    })
}
fn core_charset(value: &PdfFontCharacterSet) -> FxCharset {
    match value {
        PdfFontCharacterSet::Ansi => FxCharset::Ansi,
        PdfFontCharacterSet::Default => FxCharset::Default,
        PdfFontCharacterSet::Symbol => FxCharset::Symbol,
        PdfFontCharacterSet::JapaneseShiftJis => FxCharset::ShiftJis,
        PdfFontCharacterSet::KoreanHangul => FxCharset::Hangul,
        PdfFontCharacterSet::ChineseGb2312 => FxCharset::ChineseSimplified,
        PdfFontCharacterSet::ChineseBig5 => FxCharset::ChineseTraditional,
        PdfFontCharacterSet::Greek => FxCharset::MswinGreek,
        PdfFontCharacterSet::Vietnamese => FxCharset::MswinVietnamese,
        PdfFontCharacterSet::Hebrew => FxCharset::MswinHebrew,
        PdfFontCharacterSet::Arabic => FxCharset::MswinArabic,
        PdfFontCharacterSet::Cyrillic => FxCharset::MswinCyrillic,
        PdfFontCharacterSet::Thai => FxCharset::Thai,
        PdfFontCharacterSet::EasternEuropean => FxCharset::MswinEasternEuropean,
    }
}
impl SystemFontInfo for FontProvider {
    fn enum_font_list(&mut self) -> Vec<(Vec<u8>, FxCharset)> {
        Vec::new()
    } // Upstream supplies no EnumFonts callback.
    fn get_font(&mut self, _face: &[u8]) -> Option<u64> {
        None
    } // Upstream supplies no GetFont callback.
    fn map_font(
        &mut self,
        weight: i32,
        italic: bool,
        char_set: FxCharset,
        pitch_family: u32,
        face: &[u8],
    ) -> Option<u64> {
        let weight = match weight {
            100 => PdfFontWeight::Weight100,
            200 => PdfFontWeight::Weight200,
            300 => PdfFontWeight::Weight300,
            400 => PdfFontWeight::Weight400Normal,
            500 => PdfFontWeight::Weight500,
            600 => PdfFontWeight::Weight600,
            700 => PdfFontWeight::Weight700Bold,
            800 => PdfFontWeight::Weight800,
            900 => PdfFontWeight::Weight900,
            -1 => return None,
            v => PdfFontWeight::Custom(v as u32),
        };
        let response = self.provider.provide(PdfiumCustomFontProviderRequest {
            font_face: std::str::from_utf8(face).ok()?.into(),
            character_set: charset(char_set)?,
            weight,
            is_italic: italic,
            is_fixed_pitch: pitch_family & 1 == 1,
            is_serif: pitch_family & 16 != 0,
            is_cursive: pitch_family & 64 != 0,
        })?;
        let id = response.id;
        if id == 0 {
            return None;
        }
        self.cache.insert(id, response);
        Some(id)
    }
    fn get_font_data(&mut self, font: u64, table: u32, buffer: &mut [u8]) -> usize {
        if table != 0 {
            return 0;
        } // Same full-font-only contract as the copied provider interface.
        let Some(response) = self.cache.get(&font) else {
            return 0;
        };
        if buffer.len() >= response.data.len() {
            buffer[..response.data.len()].copy_from_slice(&response.data);
        }
        response.data.len()
    }
    fn get_face_name(&mut self, font: u64) -> Option<Vec<u8>> {
        self.cache
            .get(&font)
            .map(|r| r.font_face.as_bytes().to_vec())
    }
    fn get_font_charset(&mut self, font: u64) -> Option<FxCharset> {
        self.cache
            .get(&font)
            .map(|r| core_charset(&r.character_set))
    }
    fn delete_font(&mut self, font: u64) {
        self.cache.remove(&font);
    }
}
pub trait PdfiumReader: Read + Seek {}
impl<R: Read + Seek> PdfiumReader for R {}
/// Entry point to the safe Rust PDF engine.
///
/// Construct with [`Pdfium::default`]; no native PDFium library or bindings are
/// needed. Loaded documents borrow this engine in their public lifetime and
/// remain single-threaded. See the crate guide for ownership and feature support.
pub struct Pdfium {
    inner: crate::backend::Pdfium,
}
impl Default for Pdfium {
    fn default() -> Self {
        initialize_ge_module(None);
        Self {
            inner: crate::backend::Pdfium::default(),
        }
    }
}
impl std::fmt::Debug for Pdfium {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pdfium").finish()
    }
}
impl Pdfium {
    pub fn set_custom_font_provider(&mut self, provider: Box<dyn PdfiumCustomFontProvider>) {
        let mut mapper = pdfoxium_core::fxge::cfx_fontmapper::CfxFontMapper::new();
        mapper.set_system_font_info(Some(Box::new(FontProvider {
            provider,
            cache: HashMap::new(),
        })));
        let previous =
            with_ge_module(|module| module.font_mgr().replace_builtin_mapper(mapper, None));
        drop(previous);
    }
    pub fn clear_custom_font_provider(&mut self) {
        let previous = with_ge_module(|module| {
            module.font_mgr().replace_builtin_mapper(
                pdfoxium_core::fxge::cfx_fontmapper::CfxFontMapper::new(),
                None,
            )
        });
        drop(previous);
    }
    pub fn use_platform_default_font_provider(&mut self) -> Result<(), PdfiumError> {
        let previous = with_ge_module(|module| {
            let mut mapper = pdfoxium_core::fxge::cfx_fontmapper::CfxFontMapper::new();
            mapper.set_system_font_info(Some(module.create_default_system_font_info()));
            module.font_mgr().replace_builtin_mapper(mapper, None)
        });
        drop(previous);
        Ok(())
    }
    /// Load a PDF from bytes with an optional password.
    ///
    /// The engine copies the input into owned storage. The public lifetime still
    /// follows the upstream API and is tied to both `self` and `bytes`.
    /// Invalid documents and incorrect passwords return [`PdfiumError`].
    pub fn load_pdf_from_byte_slice<'a>(
        &'a self,
        bytes: &'a [u8],
        password: Option<&str>,
    ) -> Result<PdfDocument<'a>, PdfiumError> {
        self.inner
            .load_pdf_from_byte_slice(bytes, password)
            .map(PdfDocument::from_inner)
    }
    /// Load a PDF, taking ownership of its source bytes.
    pub fn load_pdf_from_byte_vec(
        &self,
        bytes: Vec<u8>,
        password: Option<&str>,
    ) -> Result<PdfDocument<'_>, PdfiumError> {
        self.inner
            .load_pdf_from_byte_vec(bytes, password)
            .map(PdfDocument::from_inner)
    }
    /// Read a PDF file into owned storage and load it with an optional password.
    ///
    /// Returns an error if the file cannot be read or the PDF cannot be loaded.
    pub fn load_pdf_from_file<'a>(
        &'a self,
        path: &(impl AsRef<Path> + ?Sized),
        password: Option<&str>,
    ) -> Result<PdfDocument<'a>, PdfiumError> {
        self.inner
            .load_pdf_from_file(path, password)
            .map(PdfDocument::from_inner)
    }
    /// Read a seekable source and load the PDF with an optional password.
    pub fn load_pdf_from_reader<'a, R: PdfiumReader + 'a>(
        &'a self,
        reader: R,
        password: Option<&str>,
    ) -> Result<PdfDocument<'a>, PdfiumError> {
        self.inner
            .load_pdf_from_reader(reader, password)
            .map(PdfDocument::from_inner)
    }
    /// Create an empty document. Add pages through [`PdfDocument::pages_mut`]
    /// and save explicitly through [`PdfDocument::save_to_file`].
    pub fn create_new_pdf<'a>(&'a self) -> Result<PdfDocument<'a>, PdfiumError> {
        let mut document = PdfDocument::from_inner(self.inner.create_new_pdf()?);
        document.set_version(PdfDocumentVersion::DEFAULT_VERSION);
        Ok(document)
    }
}
