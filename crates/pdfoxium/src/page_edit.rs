// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Page sequence, page-box and annotation-flattening operations. Structural
//! failures retain the engine's partial changes; existing handles are reconciled
//! against the actual resulting page sequence even on failure.

use crate::{Document, Error, Page, PageId, Rect, Result, Rotation, Size};
use pdfoxium_core::{
    fpdfapi::parser::{
        cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::ObjectRef,
        cpdf_scalar_object::CpdfScalarObject,
    },
    fpdfsdk::{fpdf_editpage, fpdf_flatten, fpdf_transformpage},
    fxcrt::fx_coordinates::CfxFloatRect,
};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet, VecDeque},
    rc::Rc,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PageBox {
    Media,
    Crop,
    Bleed,
    Trim,
    Art,
}
impl PageBox {
    fn key(self) -> &'static [u8] {
        match self {
            Self::Media => b"MediaBox",
            Self::Crop => b"CropBox",
            Self::Bleed => b"BleedBox",
            Self::Trim => b"TrimBox",
            Self::Art => b"ArtBox",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FlattenUsage {
    Display,
    Print,
}

fn order(document: &Rc<RefCell<CpdfDocument>>) -> Vec<Option<ObjectRef>> {
    let mut document = document.borrow_mut();
    (0..document.page_count())
        .map(|index| document.page_dictionary(index))
        .collect()
}

fn reconcile(
    document: &Document,
    before: &[Option<ObjectRef>],
    after: &[Option<ObjectRef>],
) -> Result<()> {
    // An unsuccessful operation may have partially changed the page tree.
    // Preserve unique dictionary identities, but invalidate ambiguous duplicate
    // occurrences rather than silently retargeting a handle to another page.
    let mut occurrences = HashMap::<usize, usize>::new();
    for page in before.iter().flatten() {
        *occurrences
            .entry(Rc::as_ptr(page) as *const () as usize)
            .or_default() += 1;
    }
    let mut positions: HashMap<usize, VecDeque<usize>> = HashMap::new();
    for (index, page) in after.iter().enumerate() {
        if let Some(page) = page {
            positions
                .entry(Rc::as_ptr(page) as *const () as usize)
                .or_default()
                .push_back(index);
        }
    }
    let mapping = before
        .iter()
        .map(|page| {
            page.as_ref()
                .and_then(|page| positions.get_mut(&(Rc::as_ptr(page) as *const () as usize)))
                .filter(|positions| positions.len() == 1)
                .filter(|_| {
                    page.as_ref().is_some_and(|page| {
                        occurrences.get(&(Rc::as_ptr(page) as *const () as usize)) == Some(&1)
                    })
                })
                .and_then(VecDeque::pop_front)
        })
        .collect::<Vec<_>>();
    document.remap_pages(&mapping)
}

impl Document {
    /// Insert a blank page at a zero-based index. Size uses unscaled page units.
    /// Both dimensions must be finite and positive. Existing page IDs survive.
    pub fn insert_blank_page(&self, index: usize, size: Size) -> Result<Page> {
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.0
            || size.height <= 0.0
        {
            return Err(Error::InvalidArgument(
                "page dimensions must be finite and positive",
            ));
        }
        let count = self.page_count()?;
        if index > count || count >= i32::MAX as usize {
            return Err(Error::InvalidArgument(
                "invalid insertion index or too many pages",
            ));
        }
        self.with_structure_edit(|core| {
            let before = order(core);
            let result = fpdf_editpage::new_page(
                core,
                index as i32,
                f64::from(size.width),
                f64::from(size.height),
            );
            let after = order(core);
            if result.is_some() {
                self.remap_pages(
                    &(0..count)
                        .map(|old| Some(if old >= index { old + 1 } else { old }))
                        .collect::<Vec<_>>(),
                )?;
            } else {
                reconcile(self, &before, &after)?;
            }
            result.map(|_| ()).ok_or(Error::Edit)
        })?;
        self.page(index)
    }

    /// Delete a page. All existing handles and its ID become stale. Content
    /// editing history is not retained, so this operation cannot be undone here.
    pub fn delete_page(&self, id: PageId) -> Result<()> {
        let index = self.page_index(id)?;
        self.with_structure_edit(|core| {
            let before = order(core);
            let number = core.borrow_mut().delete_page(index as i32);
            let after = order(core);
            if number == 0 {
                reconcile(self, &before, &after)?;
                return Err(Error::Edit);
            }
            self.remap_pages(
                &(0..before.len())
                    .map(|old| {
                        if old == index {
                            None
                        } else {
                            Some(if old > index { old - 1 } else { old })
                        }
                    })
                    .collect::<Vec<_>>(),
            )?;
            core.borrow_mut().set_page_to_null_object(number);
            Ok(())
        })
    }

    /// Move distinct pages in the supplied order. `destination` is an index in
    /// the page sequence after removing the selected pages. IDs remain stable.
    /// As with import, malformed page trees can cause a partial structural edit;
    /// affected handles are reconciled rather than silently targeting other pages.
    pub fn move_pages(&self, pages: &[PageId], destination: usize) -> Result<()> {
        let count = self.page_count()?;
        if pages.len() > count || destination > count - pages.len() {
            return Err(Error::InvalidArgument(
                "destination exceeds the remaining page count",
            ));
        }
        let mut unique = HashSet::new();
        let mut indices = Vec::with_capacity(pages.len());
        for &page in pages {
            let index = self.page_index(page)?;
            if !unique.insert(index) {
                return Err(Error::InvalidArgument(
                    "move selection contains duplicate pages",
                ));
            }
            indices.push(
                i32::try_from(index).map_err(|_| Error::InvalidArgument("page index too large"))?,
            );
        }
        if indices.is_empty() {
            return Ok(());
        }
        let destination = i32::try_from(destination)
            .map_err(|_| Error::InvalidArgument("destination too large"))?;
        self.with_structure_edit(|core| {
            let before = order(core);
            let success = core.borrow_mut().move_pages(&indices, destination);
            let after = order(core);
            if success {
                let mut sequence: Vec<_> =
                    (0..count).filter(|index| !unique.contains(index)).collect();
                sequence.splice(
                    destination as usize..destination as usize,
                    indices.iter().map(|index| *index as usize),
                );
                let mut mapping = vec![None; count];
                for (new, old) in sequence.into_iter().enumerate() {
                    mapping[old] = Some(new);
                }
                self.remap_pages(&mapping)?;
                Ok(())
            } else {
                reconcile(self, &before, &after)?;
                Err(Error::Edit)
            }
        })
    }
}

impl Page {
    /// Set intrinsic clockwise page rotation. This changes display dimensions,
    /// not the positions or matrices of individual content objects.
    pub fn set_rotation(&self, rotation: Rotation) -> Result<()> {
        let degrees = match rotation {
            Rotation::None => 0,
            Rotation::Cw90 => 90,
            Rotation::Cw180 => 180,
            Rotation::Cw270 => 270,
        };
        self.with_edit(|page, _| {
            page.dict()
                .borrow_mut()
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .ok_or(Error::Format)?
                .set_new_for(b"Rotate", CpdfScalarObject::number_from_signed(degrees));
            page.update_dimensions();
            Ok(())
        })
    }

    /// Set a page boundary in unscaled PDF coordinates. Edges must be finite
    /// with positive width/height; the engine applies media/crop intersection.
    pub fn set_box(&self, which: PageBox, bounds: Rect) -> Result<()> {
        if ![bounds.left, bounds.bottom, bounds.right, bounds.top]
            .into_iter()
            .all(f32::is_finite)
            || bounds.right <= bounds.left
            || bounds.top <= bounds.bottom
        {
            return Err(Error::InvalidArgument(
                "page box must be finite with positive dimensions",
            ));
        }
        self.with_edit(|page, _| {
            fpdf_transformpage::set_bounding_box(
                page,
                which.key(),
                CfxFloatRect::new(bounds.left, bounds.bottom, bounds.right, bounds.top),
            );
            Ok(())
        })
    }

    /// Flatten annotation appearances into page content for the requested usage.
    /// Returns false if there was nothing to flatten. The loaded content is
    /// invalidated after an attempted flatten, including partial failure, so the
    /// next render parses the resulting serialized objects.
    /// Object IDs from the preceding content residency become stale.
    pub fn flatten(&self, usage: FlattenUsage) -> Result<bool> {
        self.with_reload_edit(|page, core| {
            match fpdf_flatten::flatten(
                core,
                page,
                match usage {
                    FlattenUsage::Display => fpdf_flatten::FLAT_NORMALDISPLAY,
                    FlattenUsage::Print => fpdf_flatten::FLAT_PRINT,
                },
            ) {
                fpdf_flatten::FLATTEN_SUCCESS => Ok(true),
                fpdf_flatten::FLATTEN_NOTHINGTODO => Ok(false),
                _ => Err(Error::Edit),
            }
        })
    }
}

/// Reusable imported page content, owned by its destination document.
/// Source page annotations are not part of a page's content XObject.
pub struct PageXObject {
    context: pdfoxium_core::fpdfapi::edit::cpdf_npagetooneexporter::XObjectContext,
    document: Document,
}
impl PageXObject {
    /// Each call creates an independently editable form object referencing imported resources.
    pub fn object(&self) -> Result<crate::DetachedObject> {
        self.document.with_core(|_| {
            Ok(crate::DetachedObject {
                core: Box::new(
                    pdfoxium_core::fpdfsdk::fpdf_ppo::new_form_object_from_xobject(&self.context),
                ),
                document: self.document.clone(),
            })
        })
    }
}
impl Document {
    /// Copy a page's content and resources into a reusable form in this document.
    pub fn import_page_xobject(&self, source: &Document, index: usize) -> Result<PageXObject> {
        let count = source.page_count()?;
        if index >= count {
            return Err(Error::PageOutOfRange { index, count });
        }
        let index =
            i32::try_from(index).map_err(|_| Error::InvalidArgument("page index too large"))?;
        self.with_import_source(source, |dest, src| {
            let context =
                pdfoxium_core::fpdfapi::edit::cpdf_npagetooneexporter::CpdfNPageToOneExporter::new(
                    dest.clone(),
                    src.clone(),
                )
                .create_xobject_context_from_page(index)
                .ok_or(Error::Import)?;
            Ok(PageXObject {
                context,
                document: self.clone(),
            })
        })
    }
    /// Create a new document with source pages arranged in a grid on each sheet.
    /// As in PDFium, a 1x1 grid imports the original pages without resizing.
    pub fn n_up(&self, size: Size, columns: usize, rows: usize) -> Result<Document> {
        if !size.width.is_finite()
            || !size.height.is_finite()
            || size.width <= 0.
            || size.height <= 0.
            || columns == 0
            || rows == 0
            || columns.checked_mul(rows).is_none()
            || !(size.width / columns as f32 > 0.)
            || !(size.height / rows as f32 > 0.)
        {
            return Err(Error::InvalidArgument("invalid N-up dimensions"));
        }
        let output = Document::create()?;
        output.with_import_source(self, |dest, src| {
            pdfoxium_core::fpdfsdk::fpdf_ppo::import_n_pages_to_one(
                dest.clone(),
                src.clone(),
                size.width,
                size.height,
                columns,
                rows,
            )
            .then_some(())
            .ok_or(Error::Import)
        })?;
        Ok(output)
    }
    /// Copy supported viewer preferences. Returns false when the source has none.
    pub fn copy_viewer_preferences(&self, source: &Document) -> Result<bool> {
        self.with_import_source(source, |dest, src| {
            Ok(pdfoxium_core::fpdfsdk::fpdf_ppo::copy_viewer_preferences(
                dest, src,
            ))
        })
    }
}
impl Page {
    /// Read an explicitly stored page box; absent boxes return None rather than an inherited default.
    pub fn page_box(&self, which: PageBox) -> Result<Option<Rect>> {
        self.with_object_access(false, |page, _, _| {
            Ok(
                fpdf_transformpage::bounding_box(page, which.key()).map(|r| Rect {
                    left: r[0],
                    bottom: r[1],
                    right: r[2],
                    top: r[3],
                }),
            )
        })
    }
    /// Wrap content streams in a transform and/or clipping rectangle. Annotation
    /// geometry is unchanged. Existing object IDs become stale because content reloads.
    pub fn transform_content(
        &self,
        matrix: Option<crate::Matrix>,
        clip: Option<Rect>,
    ) -> Result<()> {
        if matrix.is_none() && clip.is_none() {
            return Err(Error::InvalidArgument("a transform or clip is required"));
        }
        if matrix.is_some_and(|m| !m.is_finite())
            || clip.is_some_and(|r| {
                ![r.left, r.bottom, r.right, r.top]
                    .into_iter()
                    .all(f32::is_finite)
            })
        {
            return Err(Error::InvalidArgument("transform and clip must be finite"));
        }
        self.with_reload_edit(|page, doc| {
            fpdf_transformpage::transform_with_clip(
                &doc.borrow(),
                page,
                matrix.map(crate::Matrix::core),
                clip.map(|r| CfxFloatRect::new(r.left, r.bottom, r.right, r.top)),
            )
            .then_some(())
            .ok_or(Error::Edit)
        })
    }
    /// Transform annotation rectangles using PDFium's page-wide annotation operation.
    /// This does not rewrite ink points or attachment quads.
    pub fn transform_annotations(&self, matrix: crate::Matrix) -> Result<()> {
        if !matrix.is_finite() {
            return Err(Error::InvalidArgument("matrix must be finite"));
        }
        self.with_edit(|page, _| {
            fpdf_editpage::transform_annots(page, matrix.core());
            Ok(())
        })
    }
    /// Prepend a rectangular clipping path to the page's existing content streams.
    pub fn clip_content(&self, rect: Rect) -> Result<()> {
        if ![rect.left, rect.bottom, rect.right, rect.top]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(Error::InvalidArgument("clip must be finite"));
        }
        self.with_reload_edit(|page, doc| {
            let clip = fpdf_transformpage::create_clip_path(CfxFloatRect::new(
                rect.left,
                rect.bottom,
                rect.right,
                rect.top,
            ));
            fpdf_transformpage::insert_clip_path(&doc.borrow(), page, &clip)
                .then_some(())
                .ok_or(Error::Edit)
        })
    }
    pub fn transform_object_clip(&self, id: crate::ObjectId, matrix: crate::Matrix) -> Result<()> {
        if !matrix.is_finite() {
            return Err(Error::InvalidArgument("matrix must be finite"));
        }
        self.edit_object_with_full_streams(id, |object| {
            fpdf_transformpage::transform_clip_path(object, matrix.core());
            Ok(())
        })
    }
}

/// Owned clipping-path snapshot. Retains source font resources for text clips.
#[derive(Clone)]
pub struct ClipPath {
    core: pdfoxium_core::fpdfapi::page::cpdf_clippath::CpdfClipPath,
    document: Option<Document>,
}
#[derive(Clone, Debug)]
pub struct ClipContour {
    pub segments: Vec<crate::PathSegment>,
    pub fill_rule: crate::FillRule,
}
impl ClipPath {
    pub fn rectangle(rect: Rect) -> Result<Self> {
        if ![rect.left, rect.bottom, rect.right, rect.top]
            .into_iter()
            .all(f32::is_finite)
        {
            return Err(Error::InvalidArgument("clip must be finite"));
        }
        Ok(Self {
            core: fpdf_transformpage::create_clip_path(CfxFloatRect::new(
                rect.left,
                rect.bottom,
                rect.right,
                rect.top,
            )),
            document: None,
        })
    }
    pub fn paths(&self) -> Vec<ClipContour> {
        (0..self.core.path_count())
            .map(|i| {
                let path = self.core.path(i);
                ClipContour {
                    segments: path
                        .get_object()
                        .map(|p| crate::edit::path_segments(p.points()))
                        .unwrap_or_default(),
                    fill_rule: if self.core.clip_type(i)
                        == pdfoxium_core::fxge::cfx_fillrenderoptions::FillType::EvenOdd
                    {
                        crate::FillRule::EvenOdd
                    } else {
                        crate::FillRule::NonZero
                    },
                }
            })
            .collect()
    }
    /// Whether the snapshot also contains text clipping, which PDFium's page
    /// clip insertion operation cannot serialize.
    pub fn has_text(&self) -> bool {
        self.core.text_count() != 0
    }
}
impl Page {
    pub fn object_clip(&self, id: crate::ObjectId) -> Result<Option<ClipPath>> {
        self.with_object(id, |object, _, _| {
            let clip = object.base().clip_path();
            Ok(clip.has_ref().then(|| ClipPath {
                core: clip.clone(),
                document: Some(self.document().clone()),
            }))
        })
    }
    /// Insert the path contours of a clip. Rejects text clips rather than silently
    /// dropping them. Reloads content and invalidates existing object IDs.
    pub fn insert_clip(&self, clip: &ClipPath) -> Result<()> {
        if clip.has_text() {
            return Err(Error::Unsupported(
                "inserting text clipping is not supported by the content writer",
            ));
        }
        let _retain_source = &clip.document;
        self.with_reload_edit(|page, doc| {
            fpdf_transformpage::insert_clip_path(&doc.borrow(), page, &clip.core)
                .then_some(())
                .ok_or(Error::Edit)
        })
    }
}

impl crate::DetachedObject {
    pub fn clip(&self) -> Option<ClipPath> {
        let clip = self.core.base().clip_path();
        clip.has_ref().then(|| ClipPath {
            core: clip.clone(),
            document: Some(self.document.clone()),
        })
    }
    pub fn transform_clip(&mut self, matrix: crate::Matrix) -> Result<()> {
        if !matrix.is_finite() {
            return Err(Error::InvalidArgument("matrix must be finite"));
        }
        let document = self.document.clone();
        document.with_core(|_| {
            fpdf_transformpage::transform_clip_path(self.core.as_mut(), matrix.core());
            Ok(())
        })
    }
}
