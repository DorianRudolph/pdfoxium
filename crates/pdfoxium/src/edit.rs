// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Immediate page-object editing. IDs identify live objects, not list positions.
use crate::{
    Bitmap, Color, Document, Error, Matrix, Page, PageId, PixelFormat, Point, Rect, Result,
};
use pdfoxium_core::{
    fpdfapi::{
        font::cpdf_font::CpdfFont,
        page::{
            cpdf_formobject::CpdfFormObject,
            cpdf_page::CpdfPage,
            cpdf_pageobject::{CpdfPageObject, PageObject, PageObjectType},
            cpdf_pageobjectholder::{CpdfPageObjectHolder, PageObjectId},
            cpdf_textobject::CpdfTextObject,
        },
        parser::cpdf_document::CpdfDocument,
    },
    fpdfsdk::{fpdf_editimg, fpdf_editpage, fpdf_editpath, fpdf_editstate, fpdf_edittext},
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF},
    fxge::dib::{cfx_dibitmap::CfxDibitmapData, fx_dib::FxdibFormat},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

/// Valid until removal, transfer to another page, or unloading its page.
/// Reordering other objects does not change this identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ObjectId {
    page: PageId,
    slot: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ObjectKind {
    Text,
    Path,
    Image,
    Shading,
    Form,
}
#[derive(Clone, Debug)]
pub struct ObjectInfo {
    pub id: ObjectId,
    pub kind: ObjectKind,
    pub bounds: Rect,
    pub matrix: Option<Matrix>,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub stroke_width: f32,
    /// Logical font-mapped characters in content order, without layout whitespace.
    /// Unmapped characters use the Unicode replacement character.
    pub text: Option<String>,
    pub font_size: Option<f32>,
}
#[derive(Default)]
pub(crate) struct ObjectRegistry {
    next: u64,
    slots: HashMap<u64, PageObjectId>,
    reverse: HashMap<PageObjectId, u64>,
}
impl ObjectRegistry {
    pub(crate) fn reset(&mut self) {
        self.slots.clear();
        self.reverse.clear();
    }
    pub(crate) fn id(&mut self, page: PageId, core: PageObjectId) -> Result<ObjectId> {
        let slot = if let Some(slot) = self.reverse.get(&core) {
            *slot
        } else {
            let slot = self.next;
            self.next = self
                .next
                .checked_add(1)
                .ok_or(Error::Internal("object identity exhausted"))?;
            self.slots.insert(slot, core);
            self.reverse.insert(core, slot);
            slot
        };
        Ok(ObjectId { page, slot })
    }
    pub(crate) fn core(&self, page: PageId, id: ObjectId) -> Result<PageObjectId> {
        if id.page != page {
            return Err(Error::Stale);
        }
        self.slots.get(&id.slot).copied().ok_or(Error::Stale)
    }
    pub(crate) fn remove(&mut self, id: ObjectId) {
        if let Some(core) = self.slots.remove(&id.slot) {
            self.reverse.remove(&core);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum FillRule {
    EvenOdd,
    #[default]
    NonZero,
}
#[derive(Clone, Copy, Debug)]
pub struct Stroke {
    pub color: Color,
    pub width: f32,
}
#[derive(Clone, Copy, Debug)]
pub enum PathCommand {
    MoveTo(Point),
    LineTo(Point),
    CurveTo(Point, Point, Point),
    Close,
}
/// Path coordinates are transformed by `matrix`. No stroke/fill means no paint.
#[derive(Clone, Debug)]
pub struct PathSpec {
    pub commands: Vec<PathCommand>,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
    pub fill_rule: FillRule,
    pub matrix: Matrix,
}
impl Default for PathSpec {
    fn default() -> Self {
        Self {
            commands: Vec::new(),
            fill: Some(Color::BLACK),
            stroke: None,
            fill_rule: FillRule::NonZero,
            matrix: Matrix::default(),
        }
    }
}
impl PathSpec {
    pub fn rect(rect: Rect) -> Self {
        Self {
            commands: vec![
                PathCommand::MoveTo(Point {
                    x: rect.left,
                    y: rect.bottom,
                }),
                PathCommand::LineTo(Point {
                    x: rect.right,
                    y: rect.bottom,
                }),
                PathCommand::LineTo(Point {
                    x: rect.right,
                    y: rect.top,
                }),
                PathCommand::LineTo(Point {
                    x: rect.left,
                    y: rect.top,
                }),
                PathCommand::Close,
            ],
            ..Self::default()
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandardFont {
    Helvetica,
    HelveticaBold,
    HelveticaOblique,
    HelveticaBoldOblique,
    TimesRoman,
    TimesBold,
    TimesItalic,
    TimesBoldItalic,
    Courier,
    CourierBold,
    CourierOblique,
    CourierBoldOblique,
    Symbol,
    ZapfDingbats,
}
impl StandardFont {
    fn name(self) -> &'static [u8] {
        match self {
            Self::Helvetica => b"Helvetica",
            Self::HelveticaBold => b"Helvetica-Bold",
            Self::HelveticaOblique => b"Helvetica-Oblique",
            Self::HelveticaBoldOblique => b"Helvetica-BoldOblique",
            Self::TimesRoman => b"Times-Roman",
            Self::TimesBold => b"Times-Bold",
            Self::TimesItalic => b"Times-Italic",
            Self::TimesBoldItalic => b"Times-BoldItalic",
            Self::Courier => b"Courier",
            Self::CourierBold => b"Courier-Bold",
            Self::CourierOblique => b"Courier-Oblique",
            Self::CourierBoldOblique => b"Courier-BoldOblique",
            Self::Symbol => b"Symbol",
            Self::ZapfDingbats => b"ZapfDingbats",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontType {
    Type1,
    TrueType,
}
/// A document-bound font. Clones share its resources and retain the document.
#[derive(Clone)]
pub struct Font {
    pub(crate) core: CpdfFont,
    pub(crate) owner: Weak<RefCell<CpdfDocument>>,
    pub(crate) _document: Document,
}
impl Document {
    pub fn standard_font(&self, font: StandardFont) -> Result<Font> {
        self.with_core(|doc| {
            let core = CpdfFont::get_stock_font(&doc.borrow(), font.name()).ok_or(Error::Edit)?;
            Ok(Font {
                core,
                owner: Rc::downgrade(doc),
                _document: self.clone(),
            })
        })
    }
    /// Embed font data. CID fonts support broader Unicode mappings than simple fonts.
    pub fn load_font(&self, bytes: &[u8], kind: FontType, cid: bool) -> Result<Font> {
        self.with_core(|doc| {
            let kind = match kind {
                FontType::Type1 => fpdf_edittext::FONT_TYPE1,
                FontType::TrueType => fpdf_edittext::FONT_TRUETYPE,
            };
            let core =
                fpdf_edittext::load_font(&doc.borrow(), bytes, kind, cid).ok_or(Error::Edit)?;
            Ok(Font {
                core,
                owner: Rc::downgrade(doc),
                _document: self.clone(),
            })
        })
    }
}
/// Text is encoded using the font's existing character mapping; no shaping is performed.
#[derive(Clone)]
pub struct TextSpec<'a> {
    pub text: &'a str,
    pub font: &'a Font,
    pub size: f32,
    pub matrix: Matrix,
    pub color: Color,
}
pub(crate) fn matrix(m: Matrix) -> Result<CfxMatrix> {
    if !m.is_finite() {
        return Err(Error::InvalidArgument("matrix must be finite"));
    }
    Ok(m.core())
}
pub(crate) fn public_matrix(m: CfxMatrix) -> Matrix {
    Matrix {
        a: m.a,
        b: m.b,
        c: m.c,
        d: m.d,
        e: m.e,
        f: m.f,
    }
}
pub(crate) fn rgba(c: Color) -> [u32; 4] {
    [c.red.into(), c.green.into(), c.blue.into(), c.alpha.into()]
}
pub(crate) fn color(c: [u32; 4]) -> Color {
    Color::rgba(c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8)
}
pub(crate) fn append(
    page: &mut CpdfPage,
    registry: &mut ObjectRegistry,
    id: PageId,
    mut object: Box<dyn PageObject>,
) -> Result<ObjectId> {
    // Reserve the identity before changing ownership (including exhaustion failure).
    if registry.next == u64::MAX {
        return Err(Error::Internal("object identity exhausted"));
    }
    fpdf_editpage::calc_bounding_box(object.as_mut());
    object.base_mut().set_dirty(true);
    let i = page.holder().page_object_count();
    page.mutable_holder().append_page_object(object);
    registry.id(
        id,
        page.holder()
            .page_object_id_by_index(i)
            .ok_or(Error::Edit)?,
    )
}
fn validate_text(font: &CpdfFont, text: &str) -> Result<()> {
    if text.is_empty() {
        return Err(Error::InvalidArgument("text must not be empty"));
    }
    for ch in text.chars() {
        let code = font.char_code_from_unicode(ch as u32);
        if code == u32::MAX
            || font.glyph_from_char_code(code) <= 0
            || font.unicode_from_char_code(code) != [ch as u32]
        {
            return Err(Error::Unsupported(
                "font cannot encode a requested character",
            ));
        }
    }
    Ok(())
}
impl Page {
    pub fn object_count(&self) -> Result<usize> {
        self.with_object_access(false, |page, _, _| Ok(page.holder().page_object_count()))
    }
    pub fn object_at(&self, index: usize) -> Result<Option<ObjectInfo>> {
        self.with_object_access(false, |page, _, registry| {
            let holder = page.holder();
            let Some(core) = holder.page_object_id_by_index(index) else {
                return Ok(None);
            };
            let id = registry.id(self.id(), core)?;
            Ok(Some(info(
                id,
                holder.page_object_by_index(index).ok_or(Error::Edit)?,
            )))
        })
    }
    pub fn form_object_count(&self, id: ObjectId) -> Result<usize> {
        self.with_object(id, |o, _, _| {
            Ok(o.as_any()
                .downcast_ref::<CpdfFormObject>()
                .ok_or(Error::Unsupported("object is not a form"))?
                .form()
                .holder()
                .page_object_count())
        })
    }
    pub fn form_object_at(&self, id: ObjectId, index: usize) -> Result<Option<ObjectInfo>> {
        self.with_object_access(false, |page, _, registry| {
            let path =
                object_path(page.holder(), registry.core(self.id(), id)?).ok_or(Error::Stale)?;
            let form = at_path(page.holder(), &path)?
                .as_any()
                .downcast_ref::<CpdfFormObject>()
                .ok_or(Error::Unsupported("object is not a form"))?;
            let holder = form.form().holder();
            let Some(core) = holder.page_object_id_by_index(index) else {
                return Ok(None);
            };
            let id = registry.id(self.id(), core)?;
            Ok(Some(info(
                id,
                holder.page_object_by_index(index).ok_or(Error::Edit)?,
            )))
        })
    }
    pub fn objects(&self) -> Result<Vec<ObjectInfo>> {
        let pid = self.id();
        self.with_object_access(false, |page, _, registry| {
            (0..page.holder().page_object_count())
                .map(|i| {
                    let id = registry.id(
                        pid,
                        page.holder()
                            .page_object_id_by_index(i)
                            .ok_or(Error::Edit)?,
                    )?;
                    Ok(info(
                        id,
                        page.holder().page_object_by_index(i).ok_or(Error::Edit)?,
                    ))
                })
                .collect()
        })
    }
    pub fn object(&self, id: ObjectId) -> Result<ObjectInfo> {
        self.with_object(id, |object, _, _| Ok(info(id, object)))
    }
    /// Direct children of a Form XObject. Coordinates are local to the form;
    /// compose ancestor matrices when mapping them to page space.
    pub fn form_objects(&self, id: ObjectId) -> Result<Vec<ObjectInfo>> {
        self.with_object_access(false, |page, _, registry| {
            let path =
                object_path(page.holder(), registry.core(self.id(), id)?).ok_or(Error::Stale)?;
            let form = at_path(page.holder(), &path)?
                .as_any()
                .downcast_ref::<CpdfFormObject>()
                .ok_or(Error::Unsupported("object is not a form"))?;
            let holder = form.form().holder();
            (0..holder.page_object_count())
                .map(|i| {
                    let id = registry.id(
                        self.id(),
                        holder.page_object_id_by_index(i).ok_or(Error::Edit)?,
                    )?;
                    Ok(info(id, holder.page_object_by_index(i).ok_or(Error::Edit)?))
                })
                .collect()
        })
    }
    pub fn add_path(&self, spec: &PathSpec) -> Result<ObjectId> {
        let m = matrix(spec.matrix)?;
        if let Some(stroke) = spec.stroke {
            validate_width(stroke.width)?;
        }
        let Some(PathCommand::MoveTo(start)) = spec.commands.first() else {
            return Err(Error::InvalidArgument("path must start with MoveTo"));
        };
        let check = |p: Point| {
            if p.x.is_finite() && p.y.is_finite() {
                Ok(())
            } else {
                Err(Error::InvalidArgument("path coordinates must be finite"))
            }
        };
        check(*start)?;
        let mut object = fpdf_editpath::create_new_path(start.x, start.y);
        for cmd in &spec.commands[1..] {
            match *cmd {
                PathCommand::MoveTo(p) => {
                    check(p)?;
                    fpdf_editpath::move_to(&mut object, p.x, p.y);
                }
                PathCommand::LineTo(p) => {
                    check(p)?;
                    fpdf_editpath::line_to(&mut object, p.x, p.y);
                }
                PathCommand::CurveTo(a, b, c) => {
                    check(a)?;
                    check(b)?;
                    check(c)?;
                    fpdf_editpath::bezier_to(
                        &mut object,
                        CfxPointF::new(a.x, a.y),
                        CfxPointF::new(b.x, b.y),
                        CfxPointF::new(c.x, c.y),
                    );
                }
                PathCommand::Close => {
                    fpdf_editpath::close(&mut object);
                }
            }
        }
        fpdf_editpath::set_draw_mode(
            &mut object,
            if spec.fill.is_none() {
                0
            } else if spec.fill_rule == FillRule::EvenOdd {
                1
            } else {
                2
            },
            spec.stroke.is_some(),
        );
        if let Some(c) = spec.fill {
            fpdf_editstate::set_fill_color(&mut object, rgba(c));
        }
        if let Some(s) = spec.stroke {
            fpdf_editstate::set_stroke_color(&mut object, rgba(s.color));
            fpdf_editstate::set_stroke_width(&mut object, s.width);
        }
        fpdf_editpage::set_object_matrix(&mut object, m);
        let pid = self.id();
        self.with_object_access(true, |page, _, registry| {
            append(page, registry, pid, Box::new(object))
        })
    }
    pub fn add_text(&self, spec: &TextSpec<'_>) -> Result<ObjectId> {
        let m = matrix(spec.matrix)?;
        if !spec.size.is_finite() || spec.size <= 0. {
            return Err(Error::InvalidArgument(
                "font size must be finite and positive",
            ));
        }
        let pid = self.id();
        self.with_object_access(true, |page, doc, registry| {
            if !Weak::ptr_eq(&spec.font.owner, &Rc::downgrade(doc)) {
                return Err(Error::InvalidArgument("font belongs to another document"));
            }
            validate_text(&spec.font.core, spec.text)?;
            let mut object =
                fpdf_edittext::create_text_object(&doc.borrow(), &spec.font.core, spec.size)
                    .ok_or(Error::Edit)?;
            if !fpdf_edittext::set_text(&mut object, &spec.text.encode_utf16().collect::<Vec<_>>())
            {
                return Err(Error::Edit);
            }
            fpdf_editstate::set_fill_color(&mut object, rgba(spec.color));
            fpdf_editpage::set_object_matrix(&mut object, m);
            append(page, registry, pid, Box::new(object))
        })
    }
    /// Embed owned pixels. The matrix maps the image's unit square to page coordinates.
    pub fn add_image(&self, mut bitmap: Bitmap, transform: Matrix) -> Result<ObjectId> {
        let m = matrix(transform)?;
        if bitmap.format == PixelFormat::Rgba8 {
            for px in bitmap.data.chunks_exact_mut(4) {
                px.swap(0, 2);
            }
        }
        let pitch = u32::try_from(bitmap.stride)
            .map_err(|_| Error::InvalidArgument("image stride exceeds u32"))?;
        let dib = CfxDibitmapData::from_buffer(
            bitmap.width as i32,
            bitmap.height as i32,
            FxdibFormat::Bgra,
            pitch,
            &mut bitmap.data,
        )
        .ok_or(Error::Allocation)?;
        let pid = self.id();
        self.with_object_access(true, |page, doc, registry| {
            let mut object = fpdf_editimg::new_image_object(&doc.borrow().page_data_ref());
            if !fpdf_editimg::set_bitmap(&mut object, &dib) {
                return Err(Error::Edit);
            }
            fpdf_editpage::set_object_matrix(&mut object, m);
            append(page, registry, pid, Box::new(object))
        })
    }
    pub fn remove_object(&self, id: ObjectId) -> Result<()> {
        self.with_object_access(true, |page, doc, registry| {
            let core = registry.core(self.id(), id)?;
            let path = object_path(page.holder(), core).ok_or(Error::Stale)?;
            let document_holder = doc.borrow().holder();
            detach_ancestors(page.mutable_holder(), &path, &document_holder)?;
            let object = take_at(page.mutable_holder(), &path)?;
            registry.discard_tree(core, object.as_ref());
            Ok(())
        })
    }
    /// Set the final paint-order index within the object's current page or form.
    pub fn reorder_object(&self, id: ObjectId, new_index: usize) -> Result<()> {
        self.with_object_access(true, |page, doc, registry| {
            let path =
                object_path(page.holder(), registry.core(self.id(), id)?).ok_or(Error::Stale)?;
            let document_holder = doc.borrow().holder();
            detach_ancestors(page.mutable_holder(), &path, &document_holder)?;
            reorder_at(page.mutable_holder(), &path, new_index)
        })
    }
    /// Move an object to the end of another page in the same document.
    /// The source ID becomes stale. Validation failures leave the object on its
    /// source page; an I/O failure may be reported after the transfer.
    /// Marked/tagged objects require page-tag rebinding and cannot yet be moved.
    /// Within the same page this reorders to the end and preserves the ID.
    pub fn move_object_to(&self, id: ObjectId, destination: &Page) -> Result<ObjectId> {
        if self.id() == destination.id() {
            self.object(id)?;
            let count = self.with_object_access(false, |page, _, registry| {
                let path = object_path(page.holder(), registry.core(self.id(), id)?)
                    .ok_or(Error::Stale)?;
                let mut holder = page.holder();
                for &i in &path[..path.len() - 1] {
                    holder = holder
                        .page_object_by_index(i)
                        .unwrap()
                        .as_any()
                        .downcast_ref::<CpdfFormObject>()
                        .unwrap()
                        .form()
                        .holder();
                }
                Ok(holder.page_object_count())
            })?;
            self.reorder_object(id, count - 1)?;
            return Ok(id);
        }
        let src_id = self.id();
        let dst_id = destination.id();
        self.with_two_edits(destination, |src, dst, doc, sr, dr| {
            let core = sr.core(src_id, id)?;
            let source_path = object_path(src.holder(), core).ok_or(Error::Stale)?;
            if object_has_marks(at_path(src.holder(), &source_path)?) {
                return Err(Error::Unsupported(
                    "moving marked content requires rebinding page tags and properties",
                ));
            }
            if dr.next == u64::MAX {
                return Err(Error::Internal("object identity exhausted"));
            }
            let document_holder = doc.borrow().holder();
            detach_ancestors(src.mutable_holder(), &source_path, &document_holder)?;
            let mut object = take_at(src.mutable_holder(), &source_path)?;
            sr.discard_tree(core, object.as_ref());
            object
                .base_mut()
                .set_content_stream(CpdfPageObject::NO_CONTENT_STREAM);
            object.base_mut().set_resource_name(&[]);
            object
                .base_mut()
                .mutable_general_state()
                .set_graphics_resource_names(Vec::new());
            let moved = append(dst, dr, dst_id, object)?;
            sr.remove(id);
            Ok(moved)
        })
    }
    pub(crate) fn with_object<T>(
        &self,
        id: ObjectId,
        f: impl FnOnce(&dyn PageObject, &CpdfPage, &Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        self.with_object_access(false, |page, doc, registry| {
            let core = registry.core(self.id(), id)?;
            let path = object_path(page.holder(), core).ok_or(Error::Stale)?;
            f(at_path(page.holder(), &path)?, page, doc)
        })
    }
    pub(crate) fn with_object_mut<T>(
        &self,
        id: ObjectId,
        f: impl FnOnce(&mut dyn PageObject, &Rc<RefCell<CpdfDocument>>) -> Result<T>,
    ) -> Result<T> {
        self.with_object_access(true, |page, doc, registry| {
            let core = registry.core(self.id(), id)?;
            let path = object_path(page.holder(), core).ok_or(Error::Stale)?;
            let document_holder = doc.borrow().holder();
            detach_ancestors(page.mutable_holder(), &path, &document_holder)?;
            mutate_at(
                page.mutable_holder(),
                &path,
                &mut Some(|object: &mut dyn PageObject| f(object, doc)),
            )
        })
    }
    pub(crate) fn edit_object_with_full_streams(
        &self,
        id: ObjectId,
        f: impl FnOnce(&mut dyn PageObject) -> Result<()>,
    ) -> Result<()> {
        self.with_object_access(true, |page, doc, registry| {
            let core = registry.core(self.id(), id)?;
            let path = object_path(page.holder(), core).ok_or(Error::Stale)?;
            let document_holder = doc.borrow().holder();
            detach_ancestors(page.mutable_holder(), &path, &document_holder)?;
            mutate_at(page.mutable_holder(), &path, &mut Some(f))?;
            // Drop inherited prefix/suffix state; each parsed object carries its
            // own clip and matrix, so the regenerated page can widen clips too.
            page.mutable_holder().prepare_full_rewrite();
            Ok(())
        })
    }
    pub(crate) fn edit_object(
        &self,
        id: ObjectId,
        f: impl FnOnce(&mut dyn PageObject) -> Result<()>,
    ) -> Result<()> {
        self.with_object_mut(id, |o, _| f(o))
    }
    pub fn transform_object(&self, id: ObjectId, transform: Matrix) -> Result<()> {
        let m = matrix(transform)?;
        self.edit_object(id, |o| {
            o.transform(m);
            Ok(())
        })
    }
    pub fn set_object_matrix(&self, id: ObjectId, transform: Matrix) -> Result<()> {
        let m = matrix(transform)?;
        self.edit_object(id, |o| {
            if fpdf_editpage::set_object_matrix(o, m) {
                Ok(())
            } else {
                Err(Error::Unsupported("object has no editable matrix"))
            }
        })
    }
    /// Change fill color. For paths, this also enables filling with the chosen rule.
    pub fn set_object_fill(&self, id: ObjectId, fill: Option<Color>, rule: FillRule) -> Result<()> {
        self.edit_object(id, |o| {
            if !matches!(o.object_type(), PageObjectType::Path | PageObjectType::Text) {
                return Err(Error::Unsupported("fill editing requires a path or text"));
            }
            if o.object_type() != PageObjectType::Path && fill.is_none() {
                return Err(Error::Unsupported("only path fill can be disabled"));
            }
            if let Some(c) = fill {
                if !fpdf_editstate::set_fill_color(o, rgba(c)) {
                    return Err(Error::Edit);
                }
            }
            if let Some(path) =
                o.as_any_mut()
                    .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
            {
                let (_, stroke) = fpdf_editpath::draw_mode(path);
                fpdf_editpath::set_draw_mode(
                    path,
                    if fill.is_none() {
                        0
                    } else if rule == FillRule::EvenOdd {
                        1
                    } else {
                        2
                    },
                    stroke,
                );
            }
            Ok(())
        })
    }
    pub fn set_object_stroke(&self, id: ObjectId, stroke: Option<Stroke>) -> Result<()> {
        if let Some(s) = stroke {
            validate_width(s.width)?;
        }
        self.edit_object(id, |o| {
            if o.object_type() != PageObjectType::Path {
                return Err(Error::Unsupported("stroke editing requires a path"));
            }
            if let Some(s) = stroke {
                fpdf_editstate::set_stroke_color(o, rgba(s.color));
                fpdf_editstate::set_stroke_width(o, s.width);
            }
            let path = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(Error::Edit)?;
            let (fill, _) = fpdf_editpath::draw_mode(path);
            fpdf_editpath::set_draw_mode(path, fill, stroke.is_some());
            Ok(())
        })
    }
    pub fn set_object_text(&self, id: ObjectId, text: &str) -> Result<()> {
        self.edit_object(id, |o| {
            let object = o
                .as_any_mut()
                .downcast_mut::<CpdfTextObject>()
                .ok_or(Error::Unsupported("object is not text"))?;
            validate_text(&object.font().ok_or(Error::Edit)?, text)?;
            if fpdf_edittext::set_text(object, &text.encode_utf16().collect::<Vec<_>>()) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
}
fn validate_width(width: f32) -> Result<()> {
    if width.is_finite() && width >= 0. {
        Ok(())
    } else {
        Err(Error::InvalidArgument(
            "stroke width must be finite and nonnegative",
        ))
    }
}
pub(crate) fn info(id: ObjectId, o: &dyn PageObject) -> ObjectInfo {
    let r = o.base().rect();
    let text_object = o.as_any().downcast_ref::<CpdfTextObject>();
    let text = text_object.and_then(|object| {
        object.font().map(|font| {
            let mut result = String::new();
            for &code in object.char_codes() {
                let mapped = font.unicode_from_char_code(code);
                if mapped.is_empty() {
                    result.push(char::REPLACEMENT_CHARACTER);
                }
                for unicode in mapped {
                    result.push(char::from_u32(unicode).unwrap_or(char::REPLACEMENT_CHARACTER));
                }
            }
            result
        })
    });
    ObjectInfo {
        id,
        kind: match o.object_type() {
            PageObjectType::Text => ObjectKind::Text,
            PageObjectType::Path => ObjectKind::Path,
            PageObjectType::Image => ObjectKind::Image,
            PageObjectType::Shading => ObjectKind::Shading,
            PageObjectType::Form => ObjectKind::Form,
        },
        bounds: Rect {
            left: r.left,
            bottom: r.bottom,
            right: r.right,
            top: r.top,
        },
        matrix: fpdf_editstate::object_matrix(o).map(public_matrix),
        fill: fpdf_editstate::fill_color(o).map(color),
        stroke: fpdf_editstate::stroke_color(o).map(color),
        stroke_width: o.base().graph_state().get_line_width(),
        text,
        font_size: text_object.map(CpdfTextObject::font_size),
    }
}

pub(crate) fn object_path(holder: &CpdfPageObjectHolder, id: PageObjectId) -> Option<Vec<usize>> {
    for i in 0..holder.page_object_count() {
        if holder.page_object_id_by_index(i) == Some(id) {
            return Some(vec![i]);
        }
        if let Some(form) = holder
            .page_object_by_index(i)?
            .as_any()
            .downcast_ref::<CpdfFormObject>()
        {
            if let Some(mut path) = object_path(form.form().holder(), id) {
                path.insert(0, i);
                return Some(path);
            }
        }
    }
    None
}
pub(crate) fn at_path<'a>(
    holder: &'a CpdfPageObjectHolder,
    path: &[usize],
) -> Result<&'a dyn PageObject> {
    let object = holder.page_object_by_index(path[0]).ok_or(Error::Stale)?;
    if path.len() == 1 {
        return Ok(object);
    }
    let form = object
        .as_any()
        .downcast_ref::<CpdfFormObject>()
        .ok_or(Error::Stale)?;
    at_path(form.form().holder(), &path[1..])
}
fn mutate_at<T, F: FnOnce(&mut dyn PageObject) -> Result<T>>(
    holder: &mut CpdfPageObjectHolder,
    path: &[usize],
    f: &mut Option<F>,
) -> Result<T> {
    let object = holder
        .page_object_by_index_mut(path[0])
        .ok_or(Error::Stale)?;
    let result = if path.len() == 1 {
        f.take().unwrap()(object)
    } else {
        let form = object
            .as_any_mut()
            .downcast_mut::<CpdfFormObject>()
            .ok_or(Error::Stale)?;
        mutate_at(form.form_mut().holder_mut(), &path[1..], f)
    };
    // A core operation can fail after mutation. Ancestors must still regenerate.
    object.base_mut().set_dirty(true);
    fpdf_editpage::calc_bounding_box(object);
    result
}

/// One raw PDF path point. Cubic curves have three consecutive Bezier points.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PathSegmentKind {
    Move,
    Line,
    Bezier,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PathSegment {
    pub point: Point,
    pub kind: PathSegmentKind,
    pub close: bool,
}
/// Owned geometry in object coordinates. `commands` groups cubic control points;
/// `segments` preserves the exact core point/closure representation.
#[derive(Clone, Debug)]
pub struct PathData {
    pub segments: Vec<PathSegment>,
    pub commands: Vec<PathCommand>,
    pub fill_rule: Option<FillRule>,
    pub stroke: bool,
    pub matrix: Matrix,
}
pub(crate) fn path_segments(
    points: &[pdfoxium_core::fxge::cfx_path::PathPoint],
) -> Vec<PathSegment> {
    use pdfoxium_core::fxge::cfx_path::PointType;
    points
        .iter()
        .map(|p| PathSegment {
            point: Point {
                x: p.point.x,
                y: p.point.y,
            },
            kind: match p.point_type {
                PointType::Move => PathSegmentKind::Move,
                PointType::Line => PathSegmentKind::Line,
                PointType::Bezier => PathSegmentKind::Bezier,
            },
            close: p.close_figure,
        })
        .collect()
}
pub(crate) fn path_snapshot(object: &dyn PageObject) -> Result<PathData> {
    let path = object
        .as_any()
        .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
        .ok_or(Error::Unsupported("object is not a path"))?;
    let segments = path_segments(fpdf_editpath::segments(path));
    let mut commands = Vec::new();
    let mut i = 0;
    while i < segments.len() {
        let p = segments[i];
        match p.kind {
            PathSegmentKind::Move => commands.push(PathCommand::MoveTo(p.point)),
            PathSegmentKind::Line => commands.push(PathCommand::LineTo(p.point)),
            PathSegmentKind::Bezier => {
                let b = segments.get(i + 1).ok_or(Error::Format)?;
                let c = segments.get(i + 2).ok_or(Error::Format)?;
                if b.kind != PathSegmentKind::Bezier
                    || c.kind != PathSegmentKind::Bezier
                    || p.close
                    || b.close
                {
                    return Err(Error::Format);
                }
                commands.push(PathCommand::CurveTo(p.point, b.point, c.point));
                i += 2;
            }
        }
        if segments[i].close {
            commands.push(PathCommand::Close);
        }
        i += 1;
    }
    let (fill, stroke) = fpdf_editpath::draw_mode(path);
    Ok(PathData {
        segments,
        commands,
        fill_rule: match fill {
            1 => Some(FillRule::EvenOdd),
            2 => Some(FillRule::NonZero),
            _ => None,
        },
        stroke,
        matrix: public_matrix(path.matrix()),
    })
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineCap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineJoin {
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Debug)]
pub struct ObjectStyle {
    pub line_cap: LineCap,
    pub line_join: LineJoin,
    pub stroke_width: f32,
    pub dash_array: Vec<f32>,
    pub dash_phase: f32,
    pub fill: Option<Color>,
    pub stroke: Option<Color>,
    pub active: bool,
    pub has_transparency: bool,
}
fn style(o: &dyn PageObject) -> ObjectStyle {
    let g = o.base().graph_state();
    ObjectStyle {
        line_cap: match g.get_line_cap().0 {
            1 => LineCap::Round,
            2 => LineCap::Square,
            _ => LineCap::Butt,
        },
        line_join: match g.get_line_join().0 {
            1 => LineJoin::Round,
            2 => LineJoin::Bevel,
            _ => LineJoin::Miter,
        },
        stroke_width: g.get_line_width(),
        dash_array: fpdf_editstate::dash_array(o).to_vec(),
        dash_phase: g.get_line_dash_phase(),
        fill: fpdf_editstate::fill_color(o).map(color),
        stroke: fpdf_editstate::stroke_color(o).map(color),
        active: o.base().is_active(),
        has_transparency: fpdf_editstate::has_transparency(o),
    }
}
fn append_commands(
    path: &mut pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject,
    commands: &[PathCommand],
) -> Result<()> {
    let finite = |p: Point| p.x.is_finite() && p.y.is_finite();
    for c in commands {
        let valid = match *c {
            PathCommand::MoveTo(p) | PathCommand::LineTo(p) => finite(p),
            PathCommand::CurveTo(a, b, c) => finite(a) && finite(b) && finite(c),
            PathCommand::Close => true,
        };
        if !valid {
            return Err(Error::InvalidArgument("path coordinates must be finite"));
        }
    }
    for c in commands {
        match *c {
            PathCommand::MoveTo(p) => fpdf_editpath::move_to(path, p.x, p.y),
            PathCommand::LineTo(p) => fpdf_editpath::line_to(path, p.x, p.y),
            PathCommand::CurveTo(a, b, c) => fpdf_editpath::bezier_to(
                path,
                CfxPointF::new(a.x, a.y),
                CfxPointF::new(b.x, b.y),
                CfxPointF::new(c.x, c.y),
            ),
            PathCommand::Close => {
                if !fpdf_editpath::close(path) {
                    return Err(Error::InvalidArgument("cannot close an empty path"));
                }
            }
        }
    }
    Ok(())
}
impl Page {
    /// Raw segments remain available even if a malformed cubic cannot be grouped.
    pub fn path_segments(&self, id: ObjectId) -> Result<Vec<PathSegment>> {
        self.with_object(id, |o, _, _| {
            let path = o
                .as_any()
                .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(Error::Unsupported("object is not a path"))?;
            Ok(path_segments(fpdf_editpath::segments(path)))
        })
    }
    pub fn path(&self, id: ObjectId) -> Result<PathData> {
        self.with_object(id, |o, _, _| path_snapshot(o))
    }
    pub fn object_style(&self, id: ObjectId) -> Result<ObjectStyle> {
        self.with_object(id, |o, _, _| Ok(style(o)))
    }
    pub fn object_rotated_bounds(&self, id: ObjectId) -> Result<Option<[Point; 4]>> {
        self.with_object(id, |o, _, _| {
            Ok(fpdf_editstate::rotated_bounds(o).map(|p| p.map(|p| Point { x: p.x, y: p.y })))
        })
    }
    pub fn append_path_commands(&self, id: ObjectId, commands: &[PathCommand]) -> Result<()> {
        self.edit_object(id, |o| {
            append_commands(
                o.as_any_mut()
                    .downcast_mut()
                    .ok_or(Error::Unsupported("object is not a path"))?,
                commands,
            )
        })
    }
    pub fn set_path_commands(&self, id: ObjectId, commands: &[PathCommand]) -> Result<()> {
        let mut replacement =
            pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject::default();
        if !matches!(commands.first(), Some(PathCommand::MoveTo(_))) {
            return Err(Error::InvalidArgument("path must start with MoveTo"));
        }
        append_commands(&mut replacement, commands)?;
        self.edit_object(id, |o| {
            let p = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(Error::Unsupported("object is not a path"))?;
            *p.mutable_path() = replacement.path().clone();
            Ok(())
        })
    }
    pub fn set_path_draw_mode(
        &self,
        id: ObjectId,
        fill: Option<FillRule>,
        stroke: bool,
    ) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editpath::set_draw_mode(
                o.as_any_mut()
                    .downcast_mut()
                    .ok_or(Error::Unsupported("object is not a path"))?,
                match fill {
                    None => 0,
                    Some(FillRule::EvenOdd) => 1,
                    Some(FillRule::NonZero) => 2,
                },
                stroke,
            );
            Ok(())
        })
    }
    pub fn set_object_active(&self, id: ObjectId, active: bool) -> Result<()> {
        self.edit_object(id, |o| {
            o.base_mut().set_is_active(active);
            Ok(())
        })
    }
    /// PDF blend mode name, e.g. `Multiply`; unknown names follow PDFium's Normal fallback.
    pub fn set_object_blend_mode(&self, id: ObjectId, mode: &str) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editstate::set_blend_mode(o, mode.as_bytes());
            Ok(())
        })
    }
    pub fn set_object_fill_color(&self, id: ObjectId, c: Color) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editstate::set_fill_color(o, rgba(c));
            Ok(())
        })
    }
    pub fn set_object_stroke_color(&self, id: ObjectId, c: Color) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editstate::set_stroke_color(o, rgba(c));
            Ok(())
        })
    }
    pub fn set_object_stroke_width(&self, id: ObjectId, width: f32) -> Result<()> {
        validate_width(width)?;
        self.edit_object(id, |o| {
            fpdf_editstate::set_stroke_width(o, width);
            Ok(())
        })
    }
    pub fn set_object_line_cap(&self, id: ObjectId, cap: LineCap) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editstate::set_line_cap(o, cap as i32);
            Ok(())
        })
    }
    pub fn set_object_line_join(&self, id: ObjectId, join: LineJoin) -> Result<()> {
        self.edit_object(id, |o| {
            fpdf_editstate::set_line_join(o, join as i32);
            Ok(())
        })
    }
    pub fn set_object_dash(&self, id: ObjectId, dashes: &[f32], phase: f32) -> Result<()> {
        if !phase.is_finite()
            || dashes.iter().any(|d| !d.is_finite() || *d < 0.)
            || (!dashes.is_empty() && dashes.iter().all(|d| *d == 0.))
        {
            return Err(Error::InvalidArgument(
                "dash pattern must be finite, nonnegative and not all zero",
            ));
        }
        self.edit_object(id, |o| {
            fpdf_editstate::set_dash_array(o, dashes.to_vec(), phase);
            Ok(())
        })
    }
    pub fn set_object_dash_phase(&self, id: ObjectId, phase: f32) -> Result<()> {
        if !phase.is_finite() {
            return Err(Error::InvalidArgument("dash phase must be finite"));
        }
        self.edit_object(id, |o| {
            fpdf_editstate::set_dash_phase(o, phase);
            Ok(())
        })
    }
    pub fn object_pattern(&self, id: ObjectId, stroke: bool) -> Result<Option<Bitmap>> {
        self.with_object(id, |o, _, doc| {
            let data = doc.borrow().page_data_ref();
            let Some(bitmap) =
                pdfoxium_core::fpdfsdk::fpdf_editpattern::rendered_pattern(data, o, stroke)
            else {
                return Ok(None);
            };
            let mut out = Bitmap::new(
                bitmap.width() as u32,
                bitmap.height() as u32,
                PixelFormat::Bgra8,
            )?;
            for y in 0..out.height as usize {
                let line = bitmap.scanline(y as i32).ok_or(Error::Edit)?;
                let start = y * out.stride;
                out.data[start..start + out.stride].copy_from_slice(&line[..out.stride]);
            }
            Ok(Some(out))
        })
    }
}

impl ObjectRegistry {
    fn discard_tree(&mut self, core: PageObjectId, object: &dyn PageObject) {
        if let Some(slot) = self.reverse.remove(&core) {
            self.slots.remove(&slot);
        }
        if let Some(form) = object.as_any().downcast_ref::<CpdfFormObject>() {
            let holder = form.form().holder();
            for i in 0..holder.page_object_count() {
                self.discard_tree(
                    holder.page_object_id_by_index(i).unwrap(),
                    holder.page_object_by_index(i).unwrap(),
                );
            }
        }
    }
}
fn take_at(holder: &mut CpdfPageObjectHolder, path: &[usize]) -> Result<Box<dyn PageObject>> {
    if path.len() == 1 {
        let id = holder
            .page_object_id_by_index(path[0])
            .ok_or(Error::Stale)?;
        return holder.remove_page_object(id).ok_or(Error::Stale);
    }
    let form = holder
        .page_object_by_index_mut(path[0])
        .ok_or(Error::Stale)?
        .as_any_mut()
        .downcast_mut::<CpdfFormObject>()
        .ok_or(Error::Stale)?;
    let result = take_at(form.form_mut().holder_mut(), &path[1..]);
    form.base_mut().set_dirty(true);
    form.calc_bounding_box();
    result
}
fn reorder_at(holder: &mut CpdfPageObjectHolder, path: &[usize], new_index: usize) -> Result<()> {
    if path.len() > 1 {
        let form = holder
            .page_object_by_index_mut(path[0])
            .ok_or(Error::Stale)?
            .as_any_mut()
            .downcast_mut::<CpdfFormObject>()
            .ok_or(Error::Stale)?;
        let result = reorder_at(form.form_mut().holder_mut(), &path[1..], new_index);
        form.base_mut().set_dirty(true);
        form.calc_bounding_box();
        return result;
    }
    if new_index >= holder.page_object_count() {
        return Err(Error::InvalidArgument("object index out of range"));
    }
    if path[0] == new_index {
        return Ok(());
    }
    let id = holder
        .page_object_id_by_index(path[0])
        .ok_or(Error::Stale)?;
    let mut object = holder.remove_page_object(id).ok_or(Error::Stale)?;
    object
        .base_mut()
        .set_content_stream(CpdfPageObject::NO_CONTENT_STREAM);
    object.base_mut().set_dirty(true);
    holder.insert_page_object_at_index(new_index, object);
    Ok(())
}
/// An owned, document-bound object removed from a page or annotation, or created
/// as a Form XObject. Dropping it releases it. Insertion consumes it.
pub struct DetachedObject {
    pub(crate) core: Box<dyn PageObject>,
    pub(crate) document: Document,
}
impl DetachedObject {
    pub fn marks(&self) -> Vec<ContentMark> {
        let marks = self.core.base().content_marks();
        (0..marks.count_items())
            .map(|i| ContentMark {
                core: marks.get_item(i),
                document: self.document.clone(),
            })
            .collect()
    }
    pub fn remove_mark(&mut self, mark: &ContentMark) -> Result<()> {
        if pdfoxium_core::fpdfsdk::fpdf_editmark::remove_mark(self.core.as_mut(), &mark.core) {
            Ok(())
        } else {
            Err(Error::Stale)
        }
    }
    pub fn kind(&self) -> ObjectKind {
        match self.core.object_type() {
            PageObjectType::Text => ObjectKind::Text,
            PageObjectType::Path => ObjectKind::Path,
            PageObjectType::Image => ObjectKind::Image,
            PageObjectType::Form => ObjectKind::Form,
            PageObjectType::Shading => ObjectKind::Shading,
        }
    }
    pub fn bounds(&self) -> Rect {
        let r = self.core.base().rect();
        Rect {
            left: r.left,
            bottom: r.bottom,
            right: r.right,
            top: r.top,
        }
    }
    pub fn path(&self) -> Result<PathData> {
        path_snapshot(self.core.as_ref())
    }
    pub fn style(&self) -> ObjectStyle {
        style(self.core.as_ref())
    }
    pub fn transform(&mut self, m: Matrix) -> Result<()> {
        let matrix = matrix(m)?;
        let document = self.document.clone();
        document.with_core(|_| {
            self.core.transform(matrix);
            Ok(())
        })
    }
    pub fn set_matrix(&mut self, m: Matrix) -> Result<()> {
        let matrix = matrix(m)?;
        let document = self.document.clone();
        document.with_core(|_| {
            if fpdf_editpage::set_object_matrix(self.core.as_mut(), matrix) {
                Ok(())
            } else {
                Err(Error::Unsupported("object has no editable matrix"))
            }
        })
    }
    pub fn set_fill_color(&mut self, c: Color) {
        fpdf_editstate::set_fill_color(self.core.as_mut(), rgba(c));
    }
    pub fn set_stroke_color(&mut self, c: Color) {
        fpdf_editstate::set_stroke_color(self.core.as_mut(), rgba(c));
    }
    pub fn set_stroke_width(&mut self, width: f32) -> Result<()> {
        validate_width(width)?;
        let document = self.document.clone();
        document.with_core(|_| {
            fpdf_editstate::set_stroke_width(self.core.as_mut(), width);
            fpdf_editpage::calc_bounding_box(self.core.as_mut());
            Ok(())
        })
    }
    pub fn set_active(&mut self, active: bool) {
        self.core.base_mut().set_is_active(active);
    }
    pub fn has_transparency(&self) -> bool {
        fpdf_editstate::has_transparency(self.core.as_ref())
    }
    pub fn set_text(&mut self, text: &str) -> Result<()> {
        let document = self.document.clone();
        document.with_core(|_| {
            let o = self
                .core
                .as_any_mut()
                .downcast_mut::<CpdfTextObject>()
                .ok_or(Error::Unsupported("object is not text"))?;
            validate_text(&o.font().ok_or(Error::Edit)?, text)?;
            if !fpdf_edittext::set_text(o, &text.encode_utf16().collect::<Vec<_>>()) {
                return Err(Error::Edit);
            }
            fpdf_editpage::calc_bounding_box(o);
            Ok(())
        })
    }
}
impl Page {
    pub fn take_object(&self, id: ObjectId) -> Result<DetachedObject> {
        self.with_object_access(true, |page, doc, registry| {
            let core = registry.core(self.id(), id)?;
            let path = object_path(page.holder(), core).ok_or(Error::Stale)?;
            let document_holder = doc.borrow().holder();
            detach_ancestors(page.mutable_holder(), &path, &document_holder)?;
            let object = take_at(page.mutable_holder(), &path)?;
            registry.discard_tree(core, object.as_ref());
            Ok(DetachedObject {
                core: object,
                document: self.document().clone(),
            })
        })
    }
    /// Insert at a zero-based paint-order position, consuming the object.
    pub fn insert_object_at(&self, index: usize, mut object: DetachedObject) -> Result<ObjectId> {
        if !object.document.same_document(self.document()) {
            return Err(Error::InvalidArgument("object belongs to another document"));
        }
        if insertion_requires_mark_rebinding(object.core.as_ref()) {
            return Err(Error::Unsupported(
                "inserting marked content requires rebinding page tags and properties",
            ));
        }
        self.with_object_access(true, |page, _, registry| {
            if index > page.holder().page_object_count() {
                return Err(Error::InvalidArgument("object index out of range"));
            }
            object
                .core
                .base_mut()
                .set_content_stream(CpdfPageObject::NO_CONTENT_STREAM);
            object.core.base_mut().set_resource_name(&[]);
            object
                .core
                .base_mut()
                .mutable_general_state()
                .set_graphics_resource_names(Vec::new());
            let id = append(page, registry, self.id(), object.core)?;
            let last = page.holder().page_object_count() - 1;
            reorder_at(page.mutable_holder(), &[last], index)?;
            Ok(id)
        })
    }
    /// Insert at the end of the page. The object must belong to this document.
    /// The owned object is consumed even on error; validate destination availability first.
    pub fn insert_object(&self, mut object: DetachedObject) -> Result<ObjectId> {
        if !object.document.same_document(self.document()) {
            return Err(Error::InvalidArgument("object belongs to another document"));
        }
        if insertion_requires_mark_rebinding(object.core.as_ref()) {
            return Err(Error::Unsupported(
                "inserting marked content requires rebinding page tags and properties",
            ));
        }
        self.with_object_access(true, |page, _, registry| {
            object
                .core
                .base_mut()
                .set_content_stream(CpdfPageObject::NO_CONTENT_STREAM);
            object.core.base_mut().set_resource_name(&[]);
            object
                .core
                .base_mut()
                .mutable_general_state()
                .set_graphics_resource_names(Vec::new());
            append(page, registry, self.id(), object.core)
        })
    }
}

/// A retained content-mark identity. Mutation always checks live membership.
#[derive(Clone)]
pub struct ContentMark {
    core: pdfoxium_core::fpdfapi::page::cpdf_contentmarks::MarkItemRef,
    document: Document,
}
#[derive(Clone, Debug, PartialEq)]
pub enum MarkValue {
    Integer(i32),
    Number(f32),
    String(Vec<u8>),
    Blob(Vec<u8>),
    Unsupported(String),
}
#[derive(Clone, Debug)]
pub struct MarkParameter {
    pub key: Vec<u8>,
    pub value: MarkValue,
}
impl ContentMark {
    pub fn name(&self) -> Vec<u8> {
        self.core.borrow().name().to_vec()
    }
    pub fn parameters(&self) -> Result<Vec<MarkParameter>> {
        use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::ObjectType;
        use pdfoxium_core::fpdfsdk::fpdf_editmark as sdk;
        self.document.with_core(|_| {
            (0..sdk::param_count(&self.core))
                .map(|i| {
                    let key = sdk::param_key(&self.core, i).ok_or(Error::Edit)?;
                    let object = sdk::param_object(&self.core, &key).ok_or(Error::Edit)?;
                    let object = object.borrow();
                    let value = match object.object_type() {
                        ObjectType::Number => {
                            if object.as_any().downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject>().is_some_and(|n|n.is_integer()) {
                                MarkValue::Integer(object.get_integer())
                            } else { MarkValue::Number(object.get_number()) }
                        }
                        ObjectType::String => {
                            if object.as_any().downcast_ref::<pdfoxium_core::fpdfapi::parser::cpdf_string::CpdfString>().is_some_and(|s|s.is_hex()) {
                                MarkValue::Blob(object.get_string())
                            } else { MarkValue::String(object.get_string()) }
                        }
                        other => MarkValue::Unsupported(format!("{other:?}")),
                    };
                    Ok(MarkParameter { key, value })
                })
                .collect()
        })
    }
}
fn contains_mark(object: &dyn PageObject, mark: &ContentMark) -> bool {
    let marks = object.base().content_marks();
    (0..marks.count_items()).any(|i| Rc::ptr_eq(&marks.get_item(i), &mark.core))
}
impl Page {
    pub fn object_marked_content_id(&self, id: ObjectId) -> Result<i32> {
        self.with_object(id, |o, _, _| {
            Ok(o.base().content_marks().marked_content_id())
        })
    }
    pub fn object_marks(&self, id: ObjectId) -> Result<Vec<ContentMark>> {
        self.with_object(id, |o, _, _| {
            let marks = o.base().content_marks();
            Ok((0..marks.count_items())
                .map(|i| ContentMark {
                    core: marks.get_item(i),
                    document: self.document().clone(),
                })
                .collect())
        })
    }
    pub fn add_object_mark(&self, id: ObjectId, name: &[u8]) -> Result<ContentMark> {
        if name.is_empty() {
            return Err(Error::InvalidArgument("mark name must not be empty"));
        }
        self.with_object_mut(id, |o, _| {
            Ok(ContentMark {
                core: pdfoxium_core::fpdfsdk::fpdf_editmark::add_mark(o, name),
                document: self.document().clone(),
            })
        })
    }
    pub fn add_existing_object_mark(&self, id: ObjectId, mark: &ContentMark) -> Result<()> {
        if !mark.document.same_document(self.document()) {
            return Err(Error::InvalidArgument("mark belongs to another document"));
        }
        self.with_object(id, |_, _, _| Ok(()))?;
        if mark.core.borrow().param_type()
            == pdfoxium_core::fpdfapi::page::cpdf_contentmarkitem::ParamType::PropertiesDict
        {
            let mut normalized = false;
            self.document().with_all_pages_edit(|page, doc| {
                if !normalized {
                    let params = pdfoxium_core::fpdfsdk::fpdf_editmark::mark_params(&mark.core)
                        .ok_or(Error::Format)?;
                    mark.core.borrow_mut().set_direct_dict(params);
                    normalized = true;
                }
                let document_holder = doc.borrow().holder();
                dirty_mark_owners(page.mutable_holder(), mark, &document_holder);
                Ok(())
            })?;
        }
        self.edit_object(id, |o| {
            pdfoxium_core::fpdfsdk::fpdf_editmark::add_existing_mark(o, mark.core.clone());
            Ok(())
        })
    }
    pub fn remove_object_mark(&self, id: ObjectId, mark: &ContentMark) -> Result<()> {
        self.edit_object(id, |o| {
            if pdfoxium_core::fpdfsdk::fpdf_editmark::remove_mark(o, &mark.core) {
                Ok(())
            } else {
                Err(Error::Stale)
            }
        })
    }
    pub fn set_object_mark_parameter(
        &self,
        id: ObjectId,
        mark: &ContentMark,
        key: &[u8],
        value: MarkValue,
    ) -> Result<()> {
        use pdfoxium_core::fpdfsdk::fpdf_editmark::{self as sdk, ParamValue};
        if key.is_empty() {
            return Err(Error::InvalidArgument("parameter key must not be empty"));
        }
        if matches!(&value,MarkValue::Number(n) if !n.is_finite()) {
            return Err(Error::InvalidArgument("parameter number must be finite"));
        }
        if matches!(&value, MarkValue::Unsupported(_)) {
            return Err(Error::Unsupported("unsupported mark parameter type"));
        }
        self.with_object(id, |o, _, _| {
            if contains_mark(o, mark) {
                Ok(())
            } else {
                Err(Error::Stale)
            }
        })?;
        let mut changed = false;
        self.document().with_all_pages_edit(|page, doc| {
            let holder = doc.borrow().holder();
            if !changed {
                let value = match &value {
                    MarkValue::Integer(n) => ParamValue::Int(*n),
                    MarkValue::Number(n) => ParamValue::Float(*n),
                    MarkValue::String(b) => ParamValue::String(b),
                    MarkValue::Blob(b) => ParamValue::Blob(b),
                    MarkValue::Unsupported(_) => unreachable!(),
                };
                sdk::set_param(&holder, &mark.core, key, value);
                changed = true;
            }
            dirty_mark_owners(page.mutable_holder(), mark, &holder);
            Ok(())
        })
    }
    pub fn remove_object_mark_parameter(
        &self,
        id: ObjectId,
        mark: &ContentMark,
        key: &[u8],
    ) -> Result<bool> {
        self.with_object(id, |o, _, _| {
            if contains_mark(o, mark) {
                Ok(())
            } else {
                Err(Error::Stale)
            }
        })?;
        let mut removed = None;
        self.document().with_all_pages_edit(|page, doc| {
            if removed.is_none() {
                removed = Some(pdfoxium_core::fpdfsdk::fpdf_editmark::remove_param(
                    &mark.core, key,
                ));
            }
            let document_holder = doc.borrow().holder();
            dirty_mark_owners(page.mutable_holder(), mark, &document_holder);
            Ok(())
        })?;
        Ok(removed.unwrap_or(false))
    }
}

fn detach_ancestors(
    holder: &mut CpdfPageObjectHolder,
    path: &[usize],
    objects: &pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef,
) -> Result<()> {
    if path.len() <= 1 {
        return Ok(());
    }
    let form = holder
        .page_object_by_index_mut(path[0])
        .ok_or(Error::Stale)?
        .as_any_mut()
        .downcast_mut::<CpdfFormObject>()
        .ok_or(Error::Stale)?;
    form.form_mut().detach_stream_for_edit(objects);
    detach_ancestors(form.form_mut().holder_mut(), &path[1..], objects)
}

fn dirty_mark_owners(
    holder: &mut CpdfPageObjectHolder,
    mark: &ContentMark,
    objects: &pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef,
) -> bool {
    let mut any = false;
    for i in 0..holder.page_object_count() {
        let o = holder.page_object_by_index_mut(i).unwrap();
        let mut dirty = contains_mark(o, mark);
        if let Some(form) = o.as_any_mut().downcast_mut::<CpdfFormObject>() {
            if dirty_mark_owners(form.form_mut().holder_mut(), mark, objects) {
                form.form_mut().detach_stream_for_edit(objects);
                dirty = true;
            }
        }
        if dirty {
            o.base_mut().set_dirty(true);
            any = true;
        }
    }
    any
}

// A Form XObject carries its own resources. Its children's OC properties do
// not depend on the destination page's property names or structure tree.
fn insertion_requires_mark_rebinding(object: &dyn PageObject) -> bool {
    fn portable_children(object: &dyn PageObject) -> bool {
        let marks = object.base().content_marks();
        for i in 0..marks.count_items() {
            let mark = marks.get_item(i);
            let mark = mark.borrow();
            if mark.name() != b"OC"
                || mark.param_type()
                    != pdfoxium_core::fpdfapi::page::cpdf_contentmarkitem::ParamType::PropertiesDict
            {
                return false;
            }
        }
        object
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .is_none_or(|form| {
                form.form()
                    .holder()
                    .iter()
                    .all(|child| portable_children(child.as_ref()))
            })
    }
    object.base().content_marks().count_items() != 0
        || object
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .is_some_and(|form| {
                !form
                    .form()
                    .holder()
                    .iter()
                    .all(|child| portable_children(child.as_ref()))
            })
}

pub(crate) fn object_has_marks(object: &dyn PageObject) -> bool {
    object.base().content_marks().count_items() != 0
        || object
            .as_any()
            .downcast_ref::<CpdfFormObject>()
            .is_some_and(|form| {
                form.form()
                    .holder()
                    .iter()
                    .any(|child| object_has_marks(child.as_ref()))
            })
}

impl DetachedObject {
    fn mutate<T>(&mut self, f: impl FnOnce(&mut dyn PageObject) -> Result<T>) -> Result<T> {
        let document = self.document.clone();
        document.with_core(|_| {
            let result = f(self.core.as_mut());
            self.core.base_mut().set_dirty(true);
            fpdf_editpage::calc_bounding_box(self.core.as_mut());
            result
        })
    }
    pub fn path_segments(&self) -> Result<Vec<PathSegment>> {
        let path = self
            .core
            .as_any()
            .downcast_ref::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
            .ok_or(Error::Unsupported("object is not a path"))?;
        Ok(path_segments(fpdf_editpath::segments(path)))
    }
    pub fn append_path_commands(&mut self, commands: &[PathCommand]) -> Result<()> {
        self.mutate(|o| {
            append_commands(
                o.as_any_mut()
                    .downcast_mut()
                    .ok_or(Error::Unsupported("object is not a path"))?,
                commands,
            )
        })
    }
    pub fn set_path_commands(&mut self, commands: &[PathCommand]) -> Result<()> {
        if !matches!(commands.first(), Some(PathCommand::MoveTo(_))) {
            return Err(Error::InvalidArgument("path must start with MoveTo"));
        }
        let mut replacement =
            pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject::default();
        append_commands(&mut replacement, commands)?;
        self.mutate(|o| {
            let path = o
                .as_any_mut()
                .downcast_mut::<pdfoxium_core::fpdfapi::page::cpdf_pathobject::CpdfPathObject>()
                .ok_or(Error::Unsupported("object is not a path"))?;
            *path.mutable_path() = replacement.path().clone();
            Ok(())
        })
    }
    pub fn set_path_draw_mode(&mut self, fill: Option<FillRule>, stroke: bool) -> Result<()> {
        self.mutate(|o| {
            fpdf_editpath::set_draw_mode(
                o.as_any_mut()
                    .downcast_mut()
                    .ok_or(Error::Unsupported("object is not a path"))?,
                match fill {
                    None => 0,
                    Some(FillRule::EvenOdd) => 1,
                    Some(FillRule::NonZero) => 2,
                },
                stroke,
            );
            Ok(())
        })
    }
    pub fn set_line_cap(&mut self, cap: LineCap) -> Result<()> {
        self.mutate(|o| {
            fpdf_editstate::set_line_cap(o, cap as i32);
            Ok(())
        })
    }
    pub fn set_line_join(&mut self, join: LineJoin) -> Result<()> {
        self.mutate(|o| {
            fpdf_editstate::set_line_join(o, join as i32);
            Ok(())
        })
    }
    pub fn set_dash(&mut self, dashes: &[f32], phase: f32) -> Result<()> {
        if !phase.is_finite()
            || dashes.iter().any(|d| !d.is_finite() || *d < 0.)
            || (!dashes.is_empty() && dashes.iter().all(|d| *d == 0.))
        {
            return Err(Error::InvalidArgument(
                "dash pattern must be finite, nonnegative and not all zero",
            ));
        }
        self.mutate(|o| {
            fpdf_editstate::set_dash_array(o, dashes.to_vec(), phase);
            Ok(())
        })
    }
    pub fn set_dash_phase(&mut self, phase: f32) -> Result<()> {
        if !phase.is_finite() {
            return Err(Error::InvalidArgument("dash phase must be finite"));
        }
        self.mutate(|o| {
            fpdf_editstate::set_dash_phase(o, phase);
            Ok(())
        })
    }
    pub fn set_blend_mode(&mut self, mode: &str) -> Result<()> {
        self.mutate(|o| {
            fpdf_editstate::set_blend_mode(o, mode.as_bytes());
            Ok(())
        })
    }
    pub fn rotated_bounds(&self) -> Option<[Point; 4]> {
        fpdf_editstate::rotated_bounds(self.core.as_ref())
            .map(|p| p.map(|p| Point { x: p.x, y: p.y }))
    }
    pub fn marked_content_id(&self) -> i32 {
        self.core.base().content_marks().marked_content_id()
    }
    pub fn add_mark(&mut self, name: &[u8]) -> Result<ContentMark> {
        if name.is_empty() {
            return Err(Error::InvalidArgument("mark name must not be empty"));
        }
        let document = self.document.clone();
        self.mutate(|o| {
            Ok(ContentMark {
                core: pdfoxium_core::fpdfsdk::fpdf_editmark::add_mark(o, name),
                document,
            })
        })
    }
    pub fn add_existing_mark(&mut self, mark: &ContentMark) -> Result<()> {
        if !mark.document.same_document(&self.document) {
            return Err(Error::InvalidArgument("mark belongs to another document"));
        }
        if mark.core.borrow().param_type()
            == pdfoxium_core::fpdfapi::page::cpdf_contentmarkitem::ParamType::PropertiesDict
        {
            mutate_shared_mark(&self.document, mark, |_| {
                let params = pdfoxium_core::fpdfsdk::fpdf_editmark::mark_params(&mark.core)
                    .ok_or(Error::Format)?;
                mark.core.borrow_mut().set_direct_dict(params);
                Ok(())
            })?;
        }
        self.mutate(|o| {
            pdfoxium_core::fpdfsdk::fpdf_editmark::add_existing_mark(o, mark.core.clone());
            Ok(())
        })
    }
    pub fn set_mark_parameter(
        &mut self,
        mark: &ContentMark,
        key: &[u8],
        value: MarkValue,
    ) -> Result<()> {
        use pdfoxium_core::fpdfsdk::fpdf_editmark::{self as sdk, ParamValue};
        if !contains_mark(self.core.as_ref(), mark) {
            return Err(Error::Stale);
        }
        if key.is_empty() {
            return Err(Error::InvalidArgument("parameter key must not be empty"));
        }
        if matches!(&value,MarkValue::Number(n) if !n.is_finite()) {
            return Err(Error::InvalidArgument("parameter number must be finite"));
        }
        if matches!(&value, MarkValue::Unsupported(_)) {
            return Err(Error::Unsupported("unsupported mark parameter type"));
        }
        mutate_shared_mark(&self.document, mark, |holder| {
            let value = match &value {
                MarkValue::Integer(n) => ParamValue::Int(*n),
                MarkValue::Number(n) => ParamValue::Float(*n),
                MarkValue::String(b) => ParamValue::String(b),
                MarkValue::Blob(b) => ParamValue::Blob(b),
                MarkValue::Unsupported(_) => unreachable!(),
            };
            sdk::set_param(holder, &mark.core, key, value);
            Ok(())
        })?;
        self.core.base_mut().set_dirty(true);
        Ok(())
    }
    pub fn remove_mark_parameter(&mut self, mark: &ContentMark, key: &[u8]) -> Result<bool> {
        if !contains_mark(self.core.as_ref(), mark) {
            return Err(Error::Stale);
        }
        let removed = mutate_shared_mark(&self.document, mark, |_| {
            Ok(pdfoxium_core::fpdfsdk::fpdf_editmark::remove_param(
                &mark.core, key,
            ))
        })?;
        self.core.base_mut().set_dirty(true);
        Ok(removed)
    }
}
fn mutate_shared_mark<T>(
    document: &Document,
    mark: &ContentMark,
    f: impl FnOnce(&pdfoxium_core::fpdfapi::parser::cpdf_indirect_object_holder::HolderRef) -> Result<T>,
) -> Result<T> {
    if !document.any_loaded_page(|page| holder_has_mark(page.holder(), mark))? {
        return document.with_core(|doc| {
            let holder = doc.borrow().holder();
            f(&holder)
        });
    }
    let mut operation = Some(f);
    let mut result = None;
    document.with_all_pages_edit(|page, doc| {
        let holder = doc.borrow().holder();
        if let Some(f) = operation.take() {
            result = Some(f(&holder)?);
        }
        dirty_mark_owners(page.mutable_holder(), mark, &holder);
        Ok(())
    })?;
    if let Some(f) = operation {
        result = Some(document.with_core(|doc| {
            let holder = doc.borrow().holder();
            f(&holder)
        })?);
    }
    result.ok_or(Error::Internal("shared mark operation was not run"))
}

fn holder_has_mark(holder: &CpdfPageObjectHolder, mark: &ContentMark) -> bool {
    holder.iter().any(|o| {
        contains_mark(o.as_ref(), mark)
            || o.as_any()
                .downcast_ref::<CpdfFormObject>()
                .is_some_and(|form| holder_has_mark(form.form().holder(), mark))
    })
}

impl DetachedObject {
    pub fn matrix(&self) -> Option<Matrix> {
        fpdf_editstate::object_matrix(self.core.as_ref()).map(public_matrix)
    }
    pub fn pattern(&self, stroke: bool) -> Result<Option<Bitmap>> {
        self.document.with_core(|doc| {
            let data = doc.borrow().page_data_ref();
            let Some(bitmap) = pdfoxium_core::fpdfsdk::fpdf_editpattern::rendered_pattern(
                data,
                self.core.as_ref(),
                stroke,
            ) else {
                return Ok(None);
            };
            let mut out = Bitmap::new(
                bitmap.width() as u32,
                bitmap.height() as u32,
                PixelFormat::Bgra8,
            )?;
            for y in 0..out.height as usize {
                let line = bitmap.scanline(y as i32).ok_or(Error::Edit)?;
                let start = y * out.stride;
                out.data[start..start + out.stride].copy_from_slice(&line[..out.stride]);
            }
            Ok(Some(out))
        })
    }
}
