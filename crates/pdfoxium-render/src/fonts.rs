// Interfaces copied from pdfium-render 0.9.4; MIT. See LICENSE.pdfium-render.
use crate::backend::document::DocumentState;
use crate::prelude::*;
use pdfoxium_core::{fpdfapi::font::cpdf_font::CpdfFont, fpdfsdk::fpdf_edittext as sdk};
use std::{collections::HashMap, io::Read, path::Path, rc::Rc};
#[derive(Copy, Clone, Debug, Eq, Hash, PartialEq)]
pub struct PdfFontToken(pub(crate) crate::backend::PdfFontToken);
pub trait ToPdfFontToken {
    fn token(&self) -> PdfFontToken;
}
impl ToPdfFontToken for PdfFontToken {
    fn token(&self) -> PdfFontToken {
        *self
    }
}
impl<'a> ToPdfFontToken for PdfFont<'a> {
    fn token(&self) -> PdfFontToken {
        self.token
    }
}
impl<'a> ToPdfFontToken for &'a PdfFont<'a> {
    fn token(&self) -> PdfFontToken {
        self.token
    }
}
pub struct PdfFonts<'a> {
    pub(crate) document: Rc<DocumentState>,
    pub(crate) fonts: HashMap<PdfFontToken, PdfFont<'a>>,
}
impl<'a> PdfFonts<'a> {
    pub fn new_built_in(&mut self, font: PdfFontBuiltin) -> PdfFontToken {
        let core = CpdfFont::get_stock_font(
            &self.document.core.borrow(),
            font.to_pdf_font_name().as_bytes(),
        )
        .expect("stock PDF font");
        self.register(core, Some(font))
    }
    pub fn times_roman(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::TimesRoman)
    }
    pub fn times_bold(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::TimesBold)
    }
    pub fn times_italic(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::TimesItalic)
    }
    pub fn times_bold_italic(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::TimesBoldItalic)
    }
    pub fn helvetica(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::Helvetica)
    }
    pub fn helvetica_bold(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::HelveticaBold)
    }
    pub fn helvetica_oblique(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::HelveticaOblique)
    }
    pub fn helvetica_bold_oblique(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::HelveticaBoldOblique)
    }
    pub fn courier(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::Courier)
    }
    pub fn courier_bold(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::CourierBold)
    }
    pub fn courier_oblique(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::CourierOblique)
    }
    pub fn courier_bold_oblique(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::CourierBoldOblique)
    }
    pub fn symbol(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::Symbol)
    }
    pub fn zapf_dingbats(&mut self) -> PdfFontToken {
        self.new_built_in(PdfFontBuiltin::ZapfDingbats)
    }
    pub fn load_type1_from_file(
        &mut self,
        path: &(impl AsRef<Path> + ?Sized),
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        self.load_type1_from_reader(
            std::fs::File::open(path).map_err(PdfiumError::IoError)?,
            is_cid_font,
        )
    }
    pub fn load_type1_from_reader(
        &mut self,
        mut reader: impl Read,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(PdfiumError::IoError)?;
        self.load_type1_from_bytes(&bytes, is_cid_font)
    }
    pub fn load_type1_from_bytes(
        &mut self,
        font_data: &[u8],
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let core = sdk::load_font(
            &self.document.core.borrow(),
            font_data,
            sdk::FONT_TYPE1,
            is_cid_font,
        )
        .ok_or(PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::Unknown,
        ))?;
        Ok(self.register(core, None))
    }
    pub fn load_true_type_from_file(
        &mut self,
        path: &(impl AsRef<Path> + ?Sized),
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        self.load_true_type_from_reader(
            std::fs::File::open(path).map_err(PdfiumError::IoError)?,
            is_cid_font,
        )
    }
    pub fn load_true_type_from_reader(
        &mut self,
        mut reader: impl Read,
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let mut bytes = Vec::new();
        reader
            .read_to_end(&mut bytes)
            .map_err(PdfiumError::IoError)?;
        self.load_true_type_from_bytes(&bytes, is_cid_font)
    }
    pub fn load_true_type_from_bytes(
        &mut self,
        font_data: &[u8],
        is_cid_font: bool,
    ) -> Result<PdfFontToken, PdfiumError> {
        let core = sdk::load_font(
            &self.document.core.borrow(),
            font_data,
            sdk::FONT_TRUETYPE,
            is_cid_font,
        )
        .ok_or(PdfiumError::PdfiumLibraryInternalError(
            PdfiumInternalError::Unknown,
        ))?;
        Ok(self.register(core, None))
    }
    pub fn get(&self, token: PdfFontToken) -> Option<&PdfFont<'_>> {
        self.fonts.get(&token)
    }
}
impl<'a> PdfFonts<'a> {
    fn register(&mut self, core: CpdfFont, built_in: Option<PdfFontBuiltin>) -> PdfFontToken {
        let index = self.document.register_font(&core);
        let token = PdfFontToken(crate::backend::PdfFontToken {
            owner: self.document.identity,
            index,
        });
        self.fonts.insert(
            token,
            PdfFont::from_core(self.document.clone(), core, token, built_in),
        );
        token
    }
}
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum PdfFontWeight {
    Weight100,
    Weight200,
    Weight300,
    Weight400Normal,
    Weight500,
    Weight600,
    Weight700Bold,
    Weight800,
    Weight900,

    /// Any font weight value that falls outside the typical 100 - 900 value range.
    Custom(u32),
}
pub enum PdfFontCharacterSet {
    Ansi,
    Default,
    Symbol,
    JapaneseShiftJis,
    KoreanHangul,
    ChineseGb2312,
    ChineseBig5,
    Greek,
    Vietnamese,
    Hebrew,
    Arabic,
    Cyrillic,
    Thai,
    EasternEuropean,
}
pub struct PdfFont<'a> {
    pub(crate) _document: Rc<DocumentState>,
    pub(crate) core: CpdfFont,
    pub(crate) token: PdfFontToken,
    built_in: Option<PdfFontBuiltin>,
    glyphs: PdfFontGlyphs<'a>,
}
impl<'a> PdfFont<'a> {
    pub(crate) fn from_core(
        document: Rc<DocumentState>,
        core: CpdfFont,
        token: PdfFontToken,
        built_in: Option<PdfFontBuiltin>,
    ) -> Self {
        let glyphs = PdfFontGlyphs::from_core(document.clone(), core.clone());
        Self {
            _document: document,
            core,
            token,
            built_in,
            glyphs,
        }
    }
}
impl<'a> PdfFont<'a> {
    pub fn name(&self) -> String {
        String::from_utf8_lossy(self.core.base_font_name()).into_owned()
    }
    pub fn family(&self) -> String {
        self.core
            .font()
            .map(|f| f.family_name())
            .unwrap_or_default()
    }
    pub fn weight(&self) -> Result<PdfFontWeight, PdfiumError> {
        match self
            .core
            .font_weight()
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?
        {
            100 => Ok(PdfFontWeight::Weight100),
            200 => Ok(PdfFontWeight::Weight200),
            300 => Ok(PdfFontWeight::Weight300),
            400 => Ok(PdfFontWeight::Weight400Normal),
            500 => Ok(PdfFontWeight::Weight500),
            600 => Ok(PdfFontWeight::Weight600),
            700 => Ok(PdfFontWeight::Weight700Bold),
            800 => Ok(PdfFontWeight::Weight800),
            900 => Ok(PdfFontWeight::Weight900),
            v => Ok(PdfFontWeight::Custom(v as u32)),
        }
    }
    pub fn italic_angle(&self) -> Result<i32, PdfiumError> {
        Ok(self.core.italic_angle())
    }
    pub fn ascent(&self, font_size: PdfPoints) -> Result<PdfPoints, PdfiumError> {
        Ok(PdfPoints::new(
            self.core.type_ascent() as f32 * font_size.value / 1000.,
        ))
    }
    pub fn descent(&self, font_size: PdfPoints) -> Result<PdfPoints, PdfiumError> {
        Ok(PdfPoints::new(
            self.core.type_descent() as f32 * font_size.value / 1000.,
        ))
    }
    pub fn is_fixed_pitch(&self) -> bool {
        self.core.font_flags() & (1 << 0) != 0
    }
    pub fn is_proportional_pitch(&self) -> bool {
        !self.is_fixed_pitch()
    }
    pub fn is_serif(&self) -> bool {
        self.core.font_flags() & (1 << 1) != 0
    }
    pub fn is_sans_serif(&self) -> bool {
        !self.is_serif()
    }
    pub fn is_symbolic(&self) -> bool {
        self.core.font_flags() & (1 << 2) != 0
    }
    pub fn is_non_symbolic(&self) -> bool {
        self.core.font_flags() & (1 << 5) != 0
    }
    pub fn is_cursive(&self) -> bool {
        self.core.font_flags() & (1 << 3) != 0
    }
    pub fn is_italic(&self) -> bool {
        self.core.font_flags() & (1 << 6) != 0
    }
    pub fn is_all_caps(&self) -> bool {
        self.core.font_flags() & (1 << 16) != 0
    }
    pub fn is_small_caps(&self) -> bool {
        self.core.font_flags() & (1 << 17) != 0
    }
    pub fn is_bold_reenforced(&self) -> bool {
        self.core.font_flags() & (1 << 18) != 0
    }
    pub fn is_built_in(&self) -> bool {
        self.built_in.is_some()
    }
    pub fn built_in(&self) -> Option<PdfFontBuiltin> {
        self.built_in
    }
    pub fn is_embedded(&self) -> Result<bool, PdfiumError> {
        Ok(self.core.is_embedded())
    }
    pub fn data(&self) -> Result<Vec<u8>, PdfiumError> {
        self.core.font().map(|f| f.font_span().to_vec()).ok_or(
            PdfiumError::PdfiumLibraryInternalError(PdfiumInternalError::Unknown),
        )
    }
    pub fn glyphs(&self) -> &PdfFontGlyphs<'_> {
        &self.glyphs
    }
}

impl<'a> PdfFont<'a> {
    pub(crate) fn from_existing(
        document: std::rc::Rc<crate::backend::document::DocumentState>,
        core: pdfoxium_core::fpdfapi::font::cpdf_font::CpdfFont,
    ) -> Self {
        let index = document.register_font(&core);
        let token = PdfFontToken(crate::backend::PdfFontToken {
            owner: document.identity,
            index,
        });
        Self::from_core(document, core, token, None)
    }
}
