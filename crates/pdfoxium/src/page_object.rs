// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Retained handles over the page's existing object identities and edit paths.
use crate::{
    Bitmap, ClipPath, Color, ContentMark, DetachedObject, Error, FillRule, Font, ImageInfo,
    JpegStorage, LineCap, LineJoin, MarkValue, Matrix, ObjectId, ObjectInfo, ObjectKind,
    ObjectStyle, Page, PathCommand, PathData, PathSegment, Point, Rect, Result, Stroke,
    TextObjectData, TextRenderMode,
};
use pdfoxium_core::{
    fpdfapi::page::{cpdf_formobject::CpdfFormObject, cpdf_pageobject::PageObjectType},
    fpdfsdk::fpdf_editstate,
};
use std::sync::Arc;

/// A clonable handle to a page object, including a child of a Form XObject.
///
/// The handle retains its page and document. Clones address the same live object;
/// ordinary edits, edits to other objects, and reordering preserve its identity.
/// Removing or taking the object, moving it to another page, removing an ancestor,
/// or unloading/deleting its page makes access return [`Error::Stale`]. Operations
/// that reload page content also invalidate handles, as they do [`ObjectId`]s.
///
/// Setters use the existing page edit operations: changes are immediately visible
/// to rendering, and serialized page content is regenerated before save/unload.
/// No explicit commit, public callback, or separate parsed-object cache is needed.
/// Coordinates of children are local to their form; compose ancestor matrices for
/// page coordinates.
///
/// ```no_run
/// # use pdfoxium::{Page, ObjectId, Color, Matrix, Result};
/// # fn edit_object(page: &Page, id: ObjectId) -> Result<()> {
/// let object = page.object_handle(id)?;
/// object.set_fill_color(Color::rgb(0, 128, 255))?;
/// object.transform(Matrix::translate(10., 0.))?;
/// let bounds = object.bounds()?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone, Debug)]
pub struct PageObject {
    page: Page,
    id: ObjectId,
}

impl Page {
    /// Retain an existing object by identity. Works for page and form-child IDs.
    /// [`Page::object`] remains available when an owned information snapshot is wanted.
    pub fn object_handle(&self, id: ObjectId) -> Result<PageObject> {
        self.with_object(id, |_, _, _| Ok(()))?;
        Ok(PageObject::new(self.clone(), id))
    }

    /// Retain a top-level page object by zero-based paint-order index.
    pub fn object_handle_at(&self, index: usize) -> Result<Option<PageObject>> {
        self.with_object_access(false, |page, _, registry| {
            page.holder()
                .page_object_id_by_index(index)
                .map(|core| {
                    registry
                        .id(self.id(), core)
                        .map(|id| PageObject::new(self.clone(), id))
                })
                .transpose()
        })
    }

    /// Retain the current top-level page objects in paint order.
    /// The returned vector can be used directly in a `for` loop.
    pub fn object_handles(&self) -> Result<Vec<PageObject>> {
        self.with_object_access(false, |page, _, registry| {
            let holder = page.holder();
            (0..holder.page_object_count())
                .map(|index| {
                    let core = holder.page_object_id_by_index(index).ok_or(Error::Stale)?;
                    Ok(PageObject::new(self.clone(), registry.id(self.id(), core)?))
                })
                .collect()
        })
    }
}

impl PageObject {
    fn new(page: Page, id: ObjectId) -> Self {
        Self { page, id }
    }

    /// Validate the handle and return its identity for the existing ID-based APIs.
    pub fn id(&self) -> Result<ObjectId> {
        self.page.with_object(self.id, |_, _, _| Ok(self.id))
    }

    /// Capture current object information as an owned snapshot.
    pub fn info(&self) -> Result<ObjectInfo> {
        self.page.object(self.id)
    }

    pub fn kind(&self) -> Result<ObjectKind> {
        self.page.with_object(self.id, |object, _, _| {
            Ok(match object.object_type() {
                PageObjectType::Text => ObjectKind::Text,
                PageObjectType::Path => ObjectKind::Path,
                PageObjectType::Image => ObjectKind::Image,
                PageObjectType::Shading => ObjectKind::Shading,
                PageObjectType::Form => ObjectKind::Form,
            })
        })
    }

    pub fn bounds(&self) -> Result<Rect> {
        self.page.with_object(self.id, |object, _, _| {
            let bounds = object.base().rect();
            Ok(Rect {
                left: bounds.left,
                bottom: bounds.bottom,
                right: bounds.right,
                top: bounds.top,
            })
        })
    }

    pub fn matrix(&self) -> Result<Option<Matrix>> {
        self.page.with_object(self.id, |object, _, _| {
            Ok(fpdf_editstate::object_matrix(object).map(crate::edit::public_matrix))
        })
    }

    pub fn style(&self) -> Result<ObjectStyle> {
        self.page.object_style(self.id)
    }

    pub fn rotated_bounds(&self) -> Result<Option<[Point; 4]>> {
        self.page.object_rotated_bounds(self.id)
    }

    pub fn has_transparency(&self) -> Result<bool> {
        self.page.with_object(self.id, |object, _, _| {
            Ok(fpdf_editstate::has_transparency(object))
        })
    }

    pub fn path(&self) -> Result<PathData> {
        self.page.path(self.id)
    }

    pub fn path_segments(&self) -> Result<Vec<PathSegment>> {
        self.page.path_segments(self.id)
    }

    pub fn transform(&self, matrix: Matrix) -> Result<()> {
        self.page.transform_object(self.id, matrix)
    }

    pub fn set_matrix(&self, matrix: Matrix) -> Result<()> {
        self.page.set_object_matrix(self.id, matrix)
    }

    /// Set the fill color without changing the path draw mode.
    pub fn set_fill_color(&self, color: Color) -> Result<()> {
        self.page.set_object_fill_color(self.id, color)
    }

    /// Change fill color for paths or text. For paths, also enable filling with
    /// the chosen rule, or disable it when `color` is `None`.
    pub fn set_fill(&self, color: Option<Color>, rule: FillRule) -> Result<()> {
        self.page.set_object_fill(self.id, color, rule)
    }

    /// Set the stroke color without changing the path draw mode.
    pub fn set_stroke_color(&self, color: Color) -> Result<()> {
        self.page.set_object_stroke_color(self.id, color)
    }

    pub fn set_stroke_width(&self, width: f32) -> Result<()> {
        self.page.set_object_stroke_width(self.id, width)
    }

    /// Change stroke style and enable/disable stroking for a path.
    pub fn set_stroke(&self, stroke: Option<Stroke>) -> Result<()> {
        self.page.set_object_stroke(self.id, stroke)
    }

    pub fn set_active(&self, active: bool) -> Result<()> {
        self.page.set_object_active(self.id, active)
    }

    pub fn set_blend_mode(&self, mode: &str) -> Result<()> {
        self.page.set_object_blend_mode(self.id, mode)
    }

    pub fn set_line_cap(&self, cap: LineCap) -> Result<()> {
        self.page.set_object_line_cap(self.id, cap)
    }

    pub fn set_line_join(&self, join: LineJoin) -> Result<()> {
        self.page.set_object_line_join(self.id, join)
    }

    pub fn set_dash(&self, dashes: &[f32], phase: f32) -> Result<()> {
        self.page.set_object_dash(self.id, dashes, phase)
    }

    pub fn set_dash_phase(&self, phase: f32) -> Result<()> {
        self.page.set_object_dash_phase(self.id, phase)
    }

    pub fn append_path_commands(&self, commands: &[PathCommand]) -> Result<()> {
        self.page.append_path_commands(self.id, commands)
    }

    pub fn set_path_commands(&self, commands: &[PathCommand]) -> Result<()> {
        self.page.set_path_commands(self.id, commands)
    }

    pub fn set_path_draw_mode(&self, fill: Option<FillRule>, stroke: bool) -> Result<()> {
        self.page.set_path_draw_mode(self.id, fill, stroke)
    }

    pub fn pattern(&self, stroke: bool) -> Result<Option<Bitmap>> {
        self.page.object_pattern(self.id, stroke)
    }

    pub fn clip(&self) -> Result<Option<ClipPath>> {
        self.page.object_clip(self.id)
    }

    pub fn transform_clip(&self, matrix: Matrix) -> Result<()> {
        self.page.transform_object_clip(self.id, matrix)
    }

    /// Logical font-mapped characters in content order, without layout whitespace.
    /// Use [`Self::extracted_text`] for the page text extractor's ordering/whitespace.
    pub fn text(&self) -> Result<String> {
        let info = self.info()?;
        if info.kind != ObjectKind::Text {
            return Err(Error::Unsupported("object is not text"));
        }
        info.text.ok_or(Error::Format)
    }

    pub fn extracted_text(&self) -> Result<String> {
        self.page.extracted_object_text(self.id)
    }

    pub fn text_object_data(&self) -> Result<TextObjectData> {
        self.page.text_object_data(self.id)
    }

    pub fn font(&self) -> Result<Font> {
        self.page.object_font(self.id)
    }

    pub fn set_text(&self, text: &str) -> Result<()> {
        self.page.set_object_text(self.id, text)
    }

    pub fn set_text_font_size(&self, size: f32) -> Result<()> {
        self.page.set_text_font_size(self.id, size)
    }

    pub fn set_text_render_mode(&self, mode: TextRenderMode) -> Result<()> {
        self.page.set_text_render_mode(self.id, mode)
    }

    pub fn set_text_charcodes(&self, codes: &[u32]) -> Result<()> {
        self.page.set_text_charcodes(self.id, codes)
    }

    pub fn set_text_positions(&self, positions: &[f32]) -> Result<()> {
        self.page.set_text_positions(self.id, positions)
    }

    /// Render text using the owning page's resources.
    pub fn render_text_object(&self, scale: f32) -> Result<Bitmap> {
        self.page.render_text_object(self.id, scale)
    }

    /// Image metadata resolved with the owning page's resources.
    pub fn image_info(&self) -> Result<ImageInfo> {
        self.page.image_info(self.id)
    }

    /// Stream bytes; decoding applies filters, not necessarily codec decompression.
    pub fn image_data(&self, decoded: bool) -> Result<Vec<u8>> {
        self.page.image_data(self.id, decoded)
    }

    pub fn image_icc_profile(&self) -> Result<Option<Vec<u8>>> {
        self.page.image_icc_profile(self.id)
    }

    pub fn image_bitmap(&self) -> Result<Bitmap> {
        self.page.image_bitmap(self.id)
    }

    pub fn render_image_object(&self) -> Result<Bitmap> {
        self.page.render_image_object(self.id)
    }

    /// Replace this object's image resource, preserving other image instances.
    pub fn set_image_bitmap(&self, pixels: Bitmap) -> Result<()> {
        self.page.set_image_bitmap(self.id, pixels)
    }

    pub fn set_image_jpeg(&self, data: impl Into<Arc<[u8]>>, storage: JpegStorage) -> Result<()> {
        self.page.set_image_jpeg(self.id, data, storage)
    }

    pub fn marked_content_id(&self) -> Result<i32> {
        self.page.object_marked_content_id(self.id)
    }

    pub fn marks(&self) -> Result<Vec<ContentMark>> {
        self.page.object_marks(self.id)
    }

    pub fn add_mark(&self, name: &[u8]) -> Result<ContentMark> {
        self.page.add_object_mark(self.id, name)
    }

    pub fn add_existing_mark(&self, mark: &ContentMark) -> Result<()> {
        self.page.add_existing_object_mark(self.id, mark)
    }

    pub fn remove_mark(&self, mark: &ContentMark) -> Result<()> {
        self.page.remove_object_mark(self.id, mark)
    }

    pub fn set_mark_parameter(
        &self,
        mark: &ContentMark,
        key: &[u8],
        value: MarkValue,
    ) -> Result<()> {
        self.page
            .set_object_mark_parameter(self.id, mark, key, value)
    }

    pub fn remove_mark_parameter(&self, mark: &ContentMark, key: &[u8]) -> Result<bool> {
        self.page.remove_object_mark_parameter(self.id, mark, key)
    }

    /// Number of direct children; returns `Unsupported` for non-form objects.
    pub fn child_count(&self) -> Result<usize> {
        self.page.form_object_count(self.id)
    }

    /// Direct child by zero-based paint-order index, in the form's local coordinates.
    pub fn child_at(&self, index: usize) -> Result<Option<PageObject>> {
        self.page.with_object_access(false, |page, _, registry| {
            let path =
                crate::edit::object_path(page.holder(), registry.core(self.page.id(), self.id)?)
                    .ok_or(Error::Stale)?;
            let form = crate::edit::at_path(page.holder(), &path)?
                .as_any()
                .downcast_ref::<CpdfFormObject>()
                .ok_or(Error::Unsupported("object is not a form"))?;
            form.form()
                .holder()
                .page_object_id_by_index(index)
                .map(|core| {
                    registry
                        .id(self.page.id(), core)
                        .map(|id| Self::new(self.page.clone(), id))
                })
                .transpose()
        })
    }

    /// Direct children in paint order, in the form's local coordinates.
    pub fn children(&self) -> Result<Vec<PageObject>> {
        self.page.with_object_access(false, |page, _, registry| {
            let path =
                crate::edit::object_path(page.holder(), registry.core(self.page.id(), self.id)?)
                    .ok_or(Error::Stale)?;
            let form = crate::edit::at_path(page.holder(), &path)?
                .as_any()
                .downcast_ref::<CpdfFormObject>()
                .ok_or(Error::Unsupported("object is not a form"))?;
            let holder = form.form().holder();
            (0..holder.page_object_count())
                .map(|index| {
                    let core = holder.page_object_id_by_index(index).ok_or(Error::Stale)?;
                    Ok(Self::new(
                        self.page.clone(),
                        registry.id(self.page.id(), core)?,
                    ))
                })
                .collect()
        })
    }

    /// Remove this object. All handles to it and its descendants become stale.
    pub fn remove(&self) -> Result<()> {
        self.page.remove_object(self.id)
    }

    /// Remove and own this object. Its handles and descendant handles become stale.
    /// Reinsertion of the returned object produces a fresh identity.
    pub fn take(&self) -> Result<DetachedObject> {
        self.page.take_object(self.id)
    }

    /// Set the paint-order index within the current page or form, preserving identity.
    pub fn reorder(&self, index: usize) -> Result<()> {
        self.page.reorder_object(self.id, index)
    }

    /// Move to the end of another page in this document and return its new handle.
    /// Source handles and descendant handles become stale. Moving to the same page
    /// reorders within the current holder and preserves identity. Marked/tagged
    /// objects cannot move between pages until tag/resource rebinding is supported.
    /// Validation errors leave the source intact; I/O errors may follow a transfer.
    pub fn move_to(&self, destination: &Page) -> Result<PageObject> {
        let id = self.page.move_object_to(self.id, destination)?;
        Ok(Self::new(destination.clone(), id))
    }
}
