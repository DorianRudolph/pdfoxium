// Interfaces copied from pdfium-render 0.9.4; MIT.
use crate::object_store::ObjectHandle;
use crate::prelude::*;
use pdfoxium_core::fpdfapi::page::cpdf_pageobject::PageObject;
use pdfoxium_core::fpdfsdk::fpdf_edittext as sdk;
use std::marker::PhantomData;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub enum PdfPageTextRenderMode {
    /// The text render mode is not recognized by Pdfium.
    Unknown = -1 as isize,

    /// The text will be filled, but not stroked.
    FilledUnstroked = 0 as isize,

    /// The text will be stroked, but not filled.
    StrokedUnfilled = 1 as isize,

    /// The text will be filled, then stroked.
    FilledThenStroked = 2 as isize,

    /// The text will be neither filled nor stroked. It will still take up size in the layout, however.
    Invisible = 3 as isize,

    /// The text will be filled and added to the path for clipping.
    FilledUnstrokedClipping = 4 as isize,

    /// The text will be stroked and added to the path for clipping.
    StrokedUnfilledClipping = 5 as isize,

    /// The text will be filled, then stroked, and added to the path for clipping.
    FilledThenStrokedClipping = 6 as isize,

    /// The text will be neither filled nor stroked, only added to the path for clipping.
    InvisibleClipping = 7 as isize,
}
impl<'a> PdfPageTextObject<'a> {
    pub fn new(
        document: &PdfDocument<'a>,
        text: impl ToString,
        font: impl ToPdfFontToken,
        font_size: PdfPoints,
    ) -> Result<Self, PdfiumError> {
        let token = font.token().0;
        if token.owner != document.inner.inner.identity {
            return Err(PdfiumError::PageObjectNotCopyable);
        }
        let font = document
            .inner
            .inner
            .fonts
            .borrow()
            .get(token.index)
            .cloned()
            .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)?;
        let core = sdk::new_text_object(font, font_size.value);
        let mut result = Self {
            handle: ObjectHandle::detached(document.inner.inner.clone(), Box::new(core)),
            lifetime: PhantomData,
        };
        result.set_text(text)?;
        Ok(result)
    }
    pub fn render_mode(&self) -> PdfPageTextRenderMode {
        self.handle
            .read(|o| {
                let mode = o
                    .as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject>()
                    .ok_or(PdfiumError::UnknownPdfPageTextRenderMode)?
                    .base()
                    .text_state()
                    .get_text_mode();
                Ok(match mode as i32 {
                    0 => PdfPageTextRenderMode::FilledUnstroked,
                    1 => PdfPageTextRenderMode::StrokedUnfilled,
                    2 => PdfPageTextRenderMode::FilledThenStroked,
                    3 => PdfPageTextRenderMode::Invisible,
                    4 => PdfPageTextRenderMode::FilledUnstrokedClipping,
                    5 => PdfPageTextRenderMode::StrokedUnfilledClipping,
                    6 => PdfPageTextRenderMode::FilledThenStrokedClipping,
                    7 => PdfPageTextRenderMode::InvisibleClipping,
                    _ => PdfPageTextRenderMode::Unknown,
                })
            })
            .unwrap_or(PdfPageTextRenderMode::Unknown)
    }
    pub fn is_visible(&self) -> bool {
        match self.render_mode() {
            PdfPageTextRenderMode::Invisible | PdfPageTextRenderMode::InvisibleClipping => false,
            _ => true,
        }
    }
    pub fn scaled_font_size(&self) -> PdfPoints {
        PdfPoints::new(self.unscaled_font_size().value * self.get_vertical_scale())
    }
    pub fn unscaled_font_size(&self) -> PdfPoints {
        self.handle.read(|o|Ok(PdfPoints::new(o.as_any().downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject>().ok_or(PdfiumError::InvalidFontSize)?.font_size()))).unwrap_or(PdfPoints::ZERO)
    }
    pub fn set_unscaled_font_size(&self, points: PdfPoints) -> Result<(), PdfiumError> {
        if !points.value.is_finite() || points.value < 0. {
            return Err(PdfiumError::InvalidFontSize);
        }
        self.handle.write(|o| {
            o.base_mut()
                .mutable_text_state()
                .set_font_size(points.value);
            Ok(())
        })
    }
    pub fn font(&self) -> PdfFont<'_> {
        let font = self
            .handle
            .read(|o| {
                o.as_any()
                    .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject>()
                    .and_then(|t| t.font())
                    .ok_or(PdfiumError::PdfiumFunctionReturnValueIndicatedFailure)
            })
            .expect("live text object with a font");
        PdfFont::from_existing(self.handle.document(), font)
    }
    pub fn text(&self) -> String {
        self.handle
            .owner_page()
            .and_then(|page| crate::text::PdfPageText::from_page(page).ok())
            .map(|text| text.for_object(self))
            .unwrap_or_default()
    }
    pub fn set_text(&mut self, text: impl ToString) -> Result<(), PdfiumError> {
        let text = text.to_string();
        let text = if text.is_empty() { " " } else { &text };
        let utf16 = text.encode_utf16().collect::<Vec<_>>();
        self.handle.write(|o| {
            crate::objects::boolean(sdk::set_text(
                o.as_any_mut()
                    .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_textobject::CpdfTextObject>()
                    .ok_or(PdfiumError::UnknownPdfPageTextRenderMode)?,
                &utf16,
            ))
        })
    }
    pub fn set_render_mode(
        &mut self,
        render_mode: PdfPageTextRenderMode,
    ) -> Result<(), PdfiumError> {
        let mode = pdfoxium_core::fpdfapi::page::cpdf_textstate::set_text_rendering_mode_from_int(
            render_mode as i32,
        )
        .ok_or(PdfiumError::UnknownPdfPageTextRenderMode)?;
        self.handle.write(|o| {
            o.base_mut().mutable_text_state().set_text_mode(mode);
            Ok(())
        })
    }
    pub fn chars(&self, text: &'a PdfPageText<'a>) -> Result<PdfPageTextChars<'a>, PdfiumError> {
        text.chars_for_object(self)
    }
    pub fn has_descenders(&self, text: &PdfPageText) -> Result<bool, PdfiumError> {
        self.chars(text)
            .map(|chars| chars.iter().any(|char| char.has_descender()))
    }
    pub fn descent(&self, text: &PdfPageText) -> Result<PdfPoints, PdfiumError> {
        let object_bottom = self.get_vertical_translation();

        let mut maximum_descent = object_bottom;

        for char in self.chars(text)?.iter() {
            let char_bottom = char.tight_bounds()?.bottom();

            if char_bottom < maximum_descent {
                maximum_descent = char_bottom;
            }
        }

        Ok(maximum_descent - object_bottom)
    }
}
