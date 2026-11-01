// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Text, font and image inspection/editing through owned data and document-bound fonts.
use crate::edit::{append, matrix, path_segments, PathSegment};
use crate::{Bitmap, DetachedObject, Error, Font, Matrix, ObjectId, Page, PixelFormat, Result};
use pdfoxium_core::{
    fpdfapi::page::{
        cpdf_imageobject::CpdfImageObject, cpdf_pageobject::PageObject,
        cpdf_textobject::CpdfTextObject, cpdf_textstate::TextRenderingMode,
    },
    fpdfsdk::{fpdf_editimg as image_sdk, fpdf_edittext as text_sdk},
    fxcrt::cfx_read_only_container_stream::CfxReadOnlyContainerStream,
    fxge::dib::{
        cfx_dibitmap::{CfxDibitmap, CfxDibitmapData},
        fx_dib::FxdibFormat,
    },
};
use std::{rc::Rc, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRenderMode {
    Fill,
    Stroke,
    FillStroke,
    Invisible,
    FillClip,
    StrokeClip,
    FillStrokeClip,
    Clip,
}
impl TextRenderMode {
    fn core(self) -> TextRenderingMode {
        match self {
            Self::Fill => TextRenderingMode::Fill,
            Self::Stroke => TextRenderingMode::Stroke,
            Self::FillStroke => TextRenderingMode::FillStroke,
            Self::Invisible => TextRenderingMode::Invisible,
            Self::FillClip => TextRenderingMode::FillClip,
            Self::StrokeClip => TextRenderingMode::StrokeClip,
            Self::FillStrokeClip => TextRenderingMode::FillStrokeClip,
            Self::Clip => TextRenderingMode::Clip,
        }
    }
    fn from_core(value: TextRenderingMode) -> Result<Self> {
        Ok(match value {
            TextRenderingMode::Fill => Self::Fill,
            TextRenderingMode::Stroke => Self::Stroke,
            TextRenderingMode::FillStroke => Self::FillStroke,
            TextRenderingMode::Invisible => Self::Invisible,
            TextRenderingMode::FillClip => Self::FillClip,
            TextRenderingMode::StrokeClip => Self::StrokeClip,
            TextRenderingMode::FillStrokeClip => Self::FillStrokeClip,
            TextRenderingMode::Clip => Self::Clip,
            TextRenderingMode::Unknown => return Err(Error::Format),
        })
    }
}
#[derive(Clone, Debug)]
pub struct TextObjectData {
    pub charcodes: Vec<u32>,
    /// Text-space positions for all characters except the first, whose position is zero.
    pub positions: Vec<f32>,
    pub vertical: bool,
    pub font_size: f32,
    pub render_mode: TextRenderMode,
}
#[derive(Clone, Debug)]
pub struct FontInfo {
    /// Original PDF name bytes, without a NUL terminator.
    pub base_name: Vec<u8>,
    pub family_name: Option<String>,
    pub embedded: bool,
    pub flags: u32,
    pub weight: Option<i32>,
    pub italic_angle: i32,
}
#[derive(Clone, Debug)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub dpi: Option<(f32, f32)>,
    pub bits_per_pixel: u32,
    /// PDFium color-space family value (zero means unknown).
    pub color_space: i32,
    pub marked_content_id: Option<i32>,
    pub filters: Vec<Vec<u8>>,
}
/// JPEG compressed bytes can be retained as a stream or copied into an in-memory stream.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum JpegStorage {
    #[default]
    Retained,
    Inline,
}

fn text(object: &dyn PageObject) -> Result<&CpdfTextObject> {
    object
        .as_any()
        .downcast_ref()
        .ok_or(Error::InvalidArgument("object is not text"))
}
fn text_mut(object: &mut dyn PageObject) -> Result<&mut CpdfTextObject> {
    object
        .as_any_mut()
        .downcast_mut()
        .ok_or(Error::InvalidArgument("object is not text"))
}
fn image(object: &dyn PageObject) -> Result<&CpdfImageObject> {
    object
        .as_any()
        .downcast_ref()
        .ok_or(Error::InvalidArgument("object is not an image"))
}
fn image_mut(object: &mut dyn PageObject) -> Result<&mut CpdfImageObject> {
    object
        .as_any_mut()
        .downcast_mut()
        .ok_or(Error::InvalidArgument("object is not an image"))
}
fn size(value: f32) -> Result<()> {
    if value.is_finite() && value >= 0. {
        Ok(())
    } else {
        Err(Error::InvalidArgument(
            "size must be finite and nonnegative",
        ))
    }
}
pub(crate) fn bitmap(mut source: CfxDibitmap) -> Result<Bitmap> {
    if !source.convert_to_bgra() {
        return Err(Error::Allocation);
    }
    let mut output = Bitmap::new(
        source.width() as u32,
        source.height() as u32,
        PixelFormat::Bgra8,
    )?;
    for (dst, src) in output
        .data
        .chunks_exact_mut(output.stride)
        .zip(source.buffer().chunks_exact(source.pitch() as usize))
    {
        dst.copy_from_slice(&src[..dst.len()]);
    }
    Ok(output)
}
fn dib(bitmap: &mut Bitmap) -> Result<CfxDibitmapData<'_>> {
    if bitmap.format == PixelFormat::Rgba8 {
        for px in bitmap.data.chunks_exact_mut(4) {
            px.swap(0, 2);
        }
        bitmap.format = PixelFormat::Bgra8;
    }
    CfxDibitmapData::from_buffer(
        bitmap.width as i32,
        bitmap.height as i32,
        FxdibFormat::Bgra,
        bitmap.stride as u32,
        &mut bitmap.data,
    )
    .ok_or(Error::Allocation)
}
fn text_data(t: &CpdfTextObject) -> Result<TextObjectData> {
    Ok(TextObjectData {
        charcodes: t.char_codes().to_vec(),
        positions: t.char_positions().iter().skip(1).copied().collect(),
        vertical: t.is_vertical(),
        font_size: t.font_size(),
        render_mode: TextRenderMode::from_core(t.text_render_mode())?,
    })
}
fn image_info(
    object: &CpdfImageObject,
    page: Option<&pdfoxium_core::fpdfapi::page::cpdf_page::CpdfPage>,
) -> Result<ImageInfo> {
    let image = object.image().ok_or(Error::Format)?;
    let data = image_sdk::metadata(object, page).ok_or(Error::Format)?;
    Ok(ImageInfo {
        width: data.width,
        height: data.height,
        dpi: data.dpi,
        bits_per_pixel: data.bits_per_pixel,
        color_space: data.colorspace,
        marked_content_id: (data.marked_content_id >= 0).then_some(data.marked_content_id),
        filters: (0..image_sdk::filter_count(&image))
            .map(|i| image_sdk::filter(&image, i).ok_or(Error::Format))
            .collect::<Result<_>>()?,
    })
}
impl crate::Document {
    /// Embed a CID TrueType font with caller-supplied ToUnicode CMap and big-endian CID-to-GID map.
    pub fn load_cid_type2_font(
        &self,
        data: &[u8],
        to_unicode: &[u8],
        cid_to_gid: &[u8],
    ) -> Result<Font> {
        self.with_core(|doc| {
            let core = text_sdk::load_cid_type2_font(&doc.borrow(), data, to_unicode, cid_to_gid)
                .ok_or(Error::Edit)?;
            Ok(Font {
                core,
                owner: Rc::downgrade(doc),
                _document: self.clone(),
            })
        })
    }
}
impl Font {
    pub fn info(&self) -> Result<FontInfo> {
        self._document.with_core(|_| {
            Ok(FontInfo {
                base_name: self.core.base_font_name().to_vec(),
                family_name: self.core.font().map(|f| f.family_name()),
                embedded: self.core.is_embedded(),
                flags: self.core.font_flags() & 0x7ffff,
                weight: self.core.font_weight(),
                italic_angle: self.core.italic_angle(),
            })
        })
    }
    /// Complete data of the selected font face (including a substitute for nonembedded fonts).
    pub fn data(&self) -> Result<Vec<u8>> {
        self._document.with_core(|_| {
            self.core
                .font()
                .map(|f| f.font_span().to_vec())
                .ok_or(Error::Format)
        })
    }
    pub fn ascent(&self, font_size: f32) -> Result<f32> {
        size(font_size)?;
        self._document
            .with_core(|_| Ok(self.core.type_ascent() as f32 * font_size / 1000.))
    }
    pub fn descent(&self, font_size: f32) -> Result<f32> {
        size(font_size)?;
        self._document
            .with_core(|_| Ok(self.core.type_descent() as f32 * font_size / 1000.))
    }
    pub fn glyph_width(&self, character: char, font_size: f32) -> Result<f32> {
        size(font_size)?;
        self._document.with_core(|_| {
            Ok(text_sdk::glyph_width(
                &self.core,
                character as u32,
                font_size,
            ))
        })
    }
    /// Outline in the same glyph coordinate system returned by PDFium; font_size selects metrics/fallback.
    pub fn glyph_path(&self, character: char, font_size: f32) -> Result<Option<Vec<PathSegment>>> {
        size(font_size)?;
        self._document.with_core(|_| {
            Ok(
                text_sdk::glyph_path(&self.core, character as u32, font_size)
                    .map(|path| path_segments(path.points())),
            )
        })
    }
}
impl Page {
    pub fn text_object_data(&self, id: ObjectId) -> Result<TextObjectData> {
        self.with_object(id, |o, _, _| text_data(text(o)?))
    }
    /// Extract this text object's contribution using PDFium's page text analysis.
    /// Unlike `ObjectInfo::text`, this preserves extraction ordering and generated whitespace.
    pub fn extracted_object_text(&self, id: ObjectId) -> Result<String> {
        self.with_object(id, |o, _, _| text(o).map(|_| ()))?;
        self.with_object_access(false, |page, document, registry| {
            let core_id = registry.core(self.id(), id)?;
            let rtl = pdfoxium_core::fpdfdoc::cpdf_viewerpreferences::CpdfViewerPreferences::new(
                &document.borrow(),
            )
            .is_direction_r2l();
            let extracted = pdfoxium_core::fpdftext::cpdf_textpage::CpdfTextPage::new(page, rtl)
                .get_text_by_object_id(core_id);
            let mut units = Vec::new();
            for unit in extracted {
                if unit <= u16::MAX as u32 {
                    units.push(unit as u16);
                } else {
                    units.extend_from_slice(
                        char::from_u32(unit)
                            .unwrap_or(char::REPLACEMENT_CHARACTER)
                            .encode_utf16(&mut [0; 2]),
                    );
                }
            }
            Ok(String::from_utf16_lossy(&units))
        })
    }
    pub fn object_font(&self, id: ObjectId) -> Result<Font> {
        self.with_object(id, |o, _, doc| {
            Ok(Font {
                core: text(o)?.font().ok_or(Error::Format)?,
                owner: Rc::downgrade(doc),
                _document: self.document().clone(),
            })
        })
    }
    pub fn set_text_font_size(&self, id: ObjectId, font_size: f32) -> Result<()> {
        size(font_size)?;
        self.with_object_mut(id, |o, _| {
            let t = text_mut(o)?;
            t.base_mut().mutable_text_state().set_font_size(font_size);
            t.calc_position_data(1.);
            Ok(())
        })
    }
    pub fn set_text_render_mode(&self, id: ObjectId, mode: TextRenderMode) -> Result<()> {
        self.with_object_mut(id, |o, _| {
            text_mut(o)?.set_text_render_mode(mode.core());
            Ok(())
        })
    }
    /// Set PDF font charcodes directly, without Unicode mapping or missing-glyph rejection.
    pub fn set_text_charcodes(&self, id: ObjectId, codes: &[u32]) -> Result<()> {
        if codes.is_empty() {
            return Err(Error::InvalidArgument("charcodes must not be empty"));
        }
        self.with_object_mut(id, |o, _| {
            if text_sdk::set_charcodes(text_mut(o)?, codes) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
    /// Specify N-1 finite text-space positions for an N-character object; first position is zero.
    pub fn set_text_positions(&self, id: ObjectId, positions: &[f32]) -> Result<()> {
        if !positions.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidArgument("positions must be finite"));
        }
        self.with_object_mut(id, |o, _| {
            if text_mut(o)?.set_absolute_positions(positions) {
                Ok(())
            } else {
                Err(Error::InvalidArgument(
                    "positions require N-1 entries, N>1 and a nonzero font size",
                ))
            }
        })
    }
    pub fn render_text_object(&self, id: ObjectId, scale: f32) -> Result<Bitmap> {
        if !scale.is_finite() || scale <= 0. {
            return Err(Error::InvalidArgument("scale must be positive and finite"));
        }
        self.with_object(id, |o, p, doc| {
            bitmap(
                text_sdk::rendered_bitmap(&doc.borrow(), Some(p), text(o)?, scale)
                    .ok_or(Error::Render)?,
            )
        })
    }
    pub fn image_info(&self, id: ObjectId) -> Result<ImageInfo> {
        self.with_object(id, |o, p, _| image_info(image(o)?, Some(p)))
    }
    /// Image stream bytes. Decoding applies stream filters, not necessarily image-codec decompression.
    pub fn image_data(&self, id: ObjectId, decoded: bool) -> Result<Vec<u8>> {
        self.with_object(id, |o, _, _| {
            image_sdk::with_image_data(
                image(o)?.image().ok_or(Error::Format)?.as_ref(),
                decoded,
                |d| d.to_vec(),
            )
            .ok_or(Error::Format)
        })
    }
    pub fn image_icc_profile(&self, id: ObjectId) -> Result<Option<Vec<u8>>> {
        self.with_object(id, |o, p, _| {
            Ok(image_sdk::with_icc_profile(
                image(o)?.image().ok_or(Error::Format)?.as_ref(),
                p,
                |d| d.to_vec(),
            ))
        })
    }
    /// Decode source pixels without the object transform or soft mask, matching PDFium GetBitmap.
    pub fn image_bitmap(&self, id: ObjectId) -> Result<Bitmap> {
        self.with_object(id, |o, _, _| {
            bitmap(
                image_sdk::get_bitmap(image(o)?.image().ok_or(Error::Format)?.as_ref())
                    .ok_or(Error::Render)?,
            )
        })
    }
    /// Rasterize the image with its transform and mask, matching PDFium GetRenderedBitmap.
    pub fn render_image_object(&self, id: ObjectId) -> Result<Bitmap> {
        self.with_object(id, |o, p, _| {
            bitmap(
                image_sdk::rendered_bitmap(&p.page_data_ref(), Some(p), image(o)?)
                    .ok_or(Error::Render)?,
            )
        })
    }
    /// Replace this object's image resource, leaving other objects sharing the old image unchanged.
    pub fn set_image_bitmap(&self, id: ObjectId, mut pixels: Bitmap) -> Result<()> {
        let source = dib(&mut pixels)?;
        self.with_object_mut(id, |o, doc| {
            let object = image_mut(o)?;
            let mut replacement = image_sdk::new_image_object(&doc.borrow().page_data_ref());
            if !image_sdk::set_bitmap(&mut replacement, &source) {
                return Err(Error::Edit);
            }
            object.set_image(replacement.image());
            Ok(())
        })
    }
    pub fn add_jpeg(
        &self,
        data: impl Into<Arc<[u8]>>,
        transform: Matrix,
        storage: JpegStorage,
    ) -> Result<ObjectId> {
        let transform = matrix(transform)?;
        let data = data.into();
        self.with_object_access(true, |p, doc, registry| {
            let mut object = image_sdk::new_image_object(&doc.borrow().page_data_ref());
            if !object.image().ok_or(Error::Edit)?.set_jpeg_image(
                Rc::new(CfxReadOnlyContainerStream::new(data)),
                storage == JpegStorage::Inline,
            ) {
                return Err(Error::Format);
            }
            object.set_image_matrix(transform);
            append(p, registry, self.id(), Box::new(object))
        })
    }
    /// Invalid JPEG data leaves the previous image unchanged. Retained storage owns the shared bytes.
    pub fn set_image_jpeg(
        &self,
        id: ObjectId,
        data: impl Into<Arc<[u8]>>,
        storage: JpegStorage,
    ) -> Result<()> {
        let data = data.into();
        self.with_object_mut(id, |o, doc| {
            let object = image_mut(o)?;
            let replacement = image_sdk::new_image_object(&doc.borrow().page_data_ref());
            if !replacement.image().ok_or(Error::Edit)?.set_jpeg_image(
                Rc::new(CfxReadOnlyContainerStream::new(data)),
                storage == JpegStorage::Inline,
            ) {
                return Err(Error::Format);
            }
            object.set_image(replacement.image());
            Ok(())
        })
    }
}

impl DetachedObject {
    fn edit_media(
        &mut self,
        edit: impl FnOnce(
            &mut dyn PageObject,
            &Rc<std::cell::RefCell<pdfoxium_core::fpdfapi::parser::cpdf_document::CpdfDocument>>,
        ) -> Result<()>,
    ) -> Result<()> {
        let document = self.document.clone();
        document.with_core(|doc| {
            let result = edit(self.core.as_mut(), doc);
            self.core.base_mut().set_dirty(true);
            pdfoxium_core::fpdfsdk::fpdf_editpage::calc_bounding_box(self.core.as_mut());
            result
        })
    }
    pub fn text_object_data(&self) -> Result<TextObjectData> {
        self.document
            .with_core(|_| text_data(text(self.core.as_ref())?))
    }
    /// Font-mapped text in stored glyph order. Detached objects have no page text
    /// analysis, so this excludes generated whitespace and reading-order changes.
    pub fn text(&self) -> Result<String> {
        self.document.with_core(|_| {
            let object = text(self.core.as_ref())?;
            let font = object.font().ok_or(Error::Format)?;
            let mut value = String::new();
            for &code in object.char_codes() {
                let units = font.unicode_from_char_code(code);
                if units.is_empty() {
                    value.push(char::REPLACEMENT_CHARACTER);
                }
                for unit in units {
                    value.push(char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
            }
            Ok(value)
        })
    }
    pub fn font(&self) -> Result<Font> {
        self.document.with_core(|doc| {
            Ok(Font {
                core: text(self.core.as_ref())?.font().ok_or(Error::Format)?,
                owner: Rc::downgrade(doc),
                _document: self.document.clone(),
            })
        })
    }
    pub fn set_text_font_size(&mut self, font_size: f32) -> Result<()> {
        size(font_size)?;
        self.edit_media(|o, _| {
            let t = text_mut(o)?;
            t.base_mut().mutable_text_state().set_font_size(font_size);
            t.calc_position_data(1.);
            Ok(())
        })
    }
    pub fn set_text_render_mode(&mut self, mode: TextRenderMode) -> Result<()> {
        self.edit_media(|o, _| {
            text_mut(o)?.set_text_render_mode(mode.core());
            Ok(())
        })
    }
    pub fn set_text_charcodes(&mut self, codes: &[u32]) -> Result<()> {
        if codes.is_empty() {
            return Err(Error::InvalidArgument("charcodes must not be empty"));
        }
        self.edit_media(|o, _| {
            if text_sdk::set_charcodes(text_mut(o)?, codes) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
    pub fn set_text_positions(&mut self, positions: &[f32]) -> Result<()> {
        if !positions.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidArgument("positions must be finite"));
        }
        self.edit_media(|o, _| {
            if text_mut(o)?.set_absolute_positions(positions) {
                Ok(())
            } else {
                Err(Error::InvalidArgument(
                    "positions require N-1 entries, N>1 and a nonzero font size",
                ))
            }
        })
    }
    /// Render with an independent page resource view, as PDFium's optional-page API.
    pub fn render_text_object(&self, scale: f32) -> Result<Bitmap> {
        if !scale.is_finite() || scale <= 0. {
            return Err(Error::InvalidArgument("scale must be positive and finite"));
        }
        self.document.with_core(|doc| {
            bitmap(
                text_sdk::rendered_bitmap(&doc.borrow(), None, text(self.core.as_ref())?, scale)
                    .ok_or(Error::Render)?,
            )
        })
    }
    /// Render with an explicit resource page from this document.
    pub fn render_text_object_in(&self, page: &Page, scale: f32) -> Result<Bitmap> {
        if !scale.is_finite() || scale <= 0. {
            return Err(Error::InvalidArgument("scale must be positive and finite"));
        }
        if !self.document.same_document(page.document()) {
            return Err(Error::InvalidArgument(
                "resource page belongs to another document",
            ));
        }
        page.with_object_access(false, |p, doc, _| {
            bitmap(
                text_sdk::rendered_bitmap(&doc.borrow(), Some(p), text(self.core.as_ref())?, scale)
                    .ok_or(Error::Render)?,
            )
        })
    }
    /// Pixel size, DPI, filters and marked-content ID without a page resource context.
    /// As in PDFium, bits_per_pixel and color_space are zero without that context;
    /// use `image_info_in` to decode them with an explicit resource page.
    pub fn image_info(&self) -> Result<ImageInfo> {
        self.document
            .with_core(|_| image_info(image(self.core.as_ref())?, None))
    }
    pub fn image_info_in(&self, page: &Page) -> Result<ImageInfo> {
        if !self.document.same_document(page.document()) {
            return Err(Error::InvalidArgument(
                "resource page belongs to another document",
            ));
        }
        page.with_object_access(false, |p, _, _| {
            image_info(image(self.core.as_ref())?, Some(p))
        })
    }
    pub fn image_data(&self, decoded: bool) -> Result<Vec<u8>> {
        self.document.with_core(|_| {
            image_sdk::with_image_data(
                image(self.core.as_ref())?
                    .image()
                    .ok_or(Error::Format)?
                    .as_ref(),
                decoded,
                |d| d.to_vec(),
            )
            .ok_or(Error::Format)
        })
    }
    /// Resolve ICC color-space names against an explicit page in this document.
    pub fn image_icc_profile(&self, page: &Page) -> Result<Option<Vec<u8>>> {
        if !self.document.same_document(page.document()) {
            return Err(Error::InvalidArgument(
                "resource page belongs to another document",
            ));
        }
        page.with_object_access(false, |p, _, _| {
            Ok(image_sdk::with_icc_profile(
                image(self.core.as_ref())?
                    .image()
                    .ok_or(Error::Format)?
                    .as_ref(),
                p,
                |d| d.to_vec(),
            ))
        })
    }
    pub fn image_bitmap(&self) -> Result<Bitmap> {
        self.document.with_core(|_| {
            bitmap(
                image_sdk::get_bitmap(
                    image(self.core.as_ref())?
                        .image()
                        .ok_or(Error::Format)?
                        .as_ref(),
                )
                .ok_or(Error::Render)?,
            )
        })
    }
    /// Render using an independent resource view; use `render_image_object_in` for page resources.
    pub fn render_image_object(&self) -> Result<Bitmap> {
        self.document.with_core(|doc| {
            bitmap(
                image_sdk::rendered_bitmap(
                    &doc.borrow().page_data_ref(),
                    None,
                    image(self.core.as_ref())?,
                )
                .ok_or(Error::Render)?,
            )
        })
    }
    pub fn render_image_object_in(&self, page: &Page) -> Result<Bitmap> {
        if !self.document.same_document(page.document()) {
            return Err(Error::InvalidArgument(
                "resource page belongs to another document",
            ));
        }
        page.with_object_access(false, |p, _, _| {
            bitmap(
                image_sdk::rendered_bitmap(&p.page_data_ref(), Some(p), image(self.core.as_ref())?)
                    .ok_or(Error::Render)?,
            )
        })
    }
    pub fn set_image_bitmap(&mut self, mut pixels: Bitmap) -> Result<()> {
        let source = dib(&mut pixels)?;
        self.edit_media(|o, doc| {
            let object = image_mut(o)?;
            let mut replacement = image_sdk::new_image_object(&doc.borrow().page_data_ref());
            if !image_sdk::set_bitmap(&mut replacement, &source) {
                return Err(Error::Edit);
            }
            object.set_image(replacement.image());
            Ok(())
        })
    }
    pub fn set_image_jpeg(
        &mut self,
        data: impl Into<Arc<[u8]>>,
        storage: JpegStorage,
    ) -> Result<()> {
        let data = data.into();
        self.edit_media(|o, doc| {
            let object = image_mut(o)?;
            let replacement = image_sdk::new_image_object(&doc.borrow().page_data_ref());
            if !replacement.image().ok_or(Error::Edit)?.set_jpeg_image(
                Rc::new(CfxReadOnlyContainerStream::new(data)),
                storage == JpegStorage::Inline,
            ) {
                return Err(Error::Format);
            }
            object.set_image(replacement.image());
            Ok(())
        })
    }
}
