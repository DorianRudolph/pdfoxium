// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Annotation edits regenerate static appearances immediately. Identities survive
//! unloading a page, but not removal. Coordinates use ordinary PDF page units.
use crate::{Color, Error, Page, PageId, Point, Rect, Result};
use pdfoxium_core::{
    fpdfapi::{
        page::cpdf_page::CpdfPage,
        parser::{
            cpdf_dictionary::CpdfDictionary, cpdf_document::CpdfDocument, cpdf_object::ObjectRef,
        },
    },
    fpdfdoc::{
        cpdf_annot::Subtype,
        cpdf_defaultappearance::CpdfDefaultAppearance,
        cpdf_generateap::{generate_annot_ap_for_object, generate_free_text_ap_for_object},
    },
    fpdfsdk::fpdf_annot as sdk,
    fxcrt::fx_coordinates::{CfxFloatRect, CfxPointF},
};
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

/// Stable annotation identity scoped to one page, independent of its array index.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct AnnotId {
    page: PageId,
    slot: u64,
}
#[derive(Default)]
pub(crate) struct AnnotRegistry {
    next: u64,
    entries: HashMap<u64, ObjectRef>,
    linked: std::collections::HashSet<u64>,
    object_contexts: HashMap<u64, Weak<RefCell<AnnotationObjectState>>>,
}
impl AnnotRegistry {
    /// Release removed-page identities without ever recycling their slots.
    pub(crate) fn reset(&mut self) {
        self.invalidate_object_handles();
        self.entries.clear();
        self.linked.clear();
    }

    /// Release parsed appearance resources, including those retained by handles.
    pub(crate) fn invalidate_object_handles(&mut self) {
        for state in self
            .object_contexts
            .drain()
            .filter_map(|(_, state)| state.upgrade())
        {
            state.borrow_mut().invalidate();
        }
    }

    /// Keep identities for annotations surviving a partial flatten operation.
    pub(crate) fn prune(&mut self, page: &CpdfPage) {
        let live: std::collections::HashSet<_> = (0..sdk::annotation_count(page))
            .filter_map(|index| sdk::annotation_at(page, index))
            .map(|object| Rc::as_ptr(&object) as *const ())
            .collect();
        self.entries.retain(|slot, object| {
            self.linked.contains(slot) || live.contains(&(Rc::as_ptr(object) as *const ()))
        });
    }

    fn register(&mut self, page: PageId, object: ObjectRef) -> Result<AnnotId> {
        if let Some((&slot, _)) = self
            .entries
            .iter()
            .find(|(_, value)| Rc::ptr_eq(value, &object))
        {
            return Ok(AnnotId { page, slot });
        }
        let slot = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::Allocation)?;
        self.entries.insert(slot, object);
        Ok(AnnotId { page, slot })
    }
    fn resolve(&self, page: &CpdfPage, owner: PageId, id: AnnotId) -> Result<ObjectRef> {
        if id.page != owner {
            return Err(Error::Stale);
        }
        let object = self.entries.get(&id.slot).ok_or(Error::Stale)?;
        if !self.linked.contains(&id.slot) {
            sdk::annotation_index(page, object).ok_or(Error::Stale)?;
        }
        Ok(object.clone())
    }
}

/// Appearance settings for the common editor annotation types. Alpha controls
/// highlight opacity; a shape's border/fill share PDF's annotation opacity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AnnotationStyle {
    FreeText {
        font_size: f32,
        color: Color,
    },
    Shape {
        border_color: Color,
        border_width: f32,
        fill: Option<Color>,
    },
    Highlight {
        color: Color,
    },
}
impl AnnotationStyle {
    pub(crate) fn validate_for(self, kind: AnnotationKind) -> Result<()> {
        match self {
            Self::FreeText { font_size, .. } if kind == AnnotationKind::FreeText => {
                if !font_size.is_finite() || font_size <= 0. {
                    return Err(Error::InvalidArgument(
                        "font size must be positive and finite",
                    ));
                }
            }
            Self::Shape {
                border_color,
                border_width,
                fill,
            } if matches!(kind, AnnotationKind::Square | AnnotationKind::Circle) => {
                if !border_width.is_finite() || border_width <= 0. {
                    return Err(Error::InvalidArgument(
                        "border width must be positive and finite",
                    ));
                }
                if fill.is_some_and(|c| c.alpha != border_color.alpha) {
                    return Err(Error::InvalidArgument(
                        "shape border and fill must share opacity",
                    ));
                }
            }
            Self::Highlight { .. } if kind == AnnotationKind::Highlight => {}
            _ => return Err(Error::Unsupported("style does not match annotation type")),
        }
        Ok(())
    }
}

/// Description of an annotation to insert. Invalid geometry is rejected at insertion.
#[derive(Clone, Debug)]
pub struct AnnotationSpec {
    kind: Subtype,
    rects: Vec<Rect>,
    strokes: Vec<Vec<Point>>,
    contents: String,
    color: Color,
    fill: Option<Color>,
    width: f32,
    font_size: f32,
}
impl AnnotationSpec {
    fn new(kind: Subtype, rects: Vec<Rect>) -> Self {
        Self {
            kind,
            rects,
            strokes: vec![],
            contents: String::new(),
            color: Color::BLACK,
            fill: None,
            width: 1.0,
            font_size: 12.0,
        }
    }
    pub fn highlight(rects: Vec<Rect>) -> Self {
        Self::new(Subtype::Highlight, rects).color(Color::rgb(255, 255, 0))
    }
    pub fn underline(rects: Vec<Rect>) -> Self {
        Self::new(Subtype::Underline, rects)
    }
    pub fn strikeout(rects: Vec<Rect>) -> Self {
        Self::new(Subtype::StrikeOut, rects)
    }
    pub fn squiggly(rects: Vec<Rect>) -> Self {
        Self::new(Subtype::Squiggly, rects)
    }
    pub fn square(rect: Rect) -> Self {
        Self::new(Subtype::Square, vec![rect])
    }
    pub fn circle(rect: Rect) -> Self {
        Self::new(Subtype::Circle, vec![rect])
    }
    /// The note icon is 20 by 20 page units, anchored at the rectangle's lower left.
    pub fn text_note(rect: Rect, text: impl Into<String>) -> Self {
        Self::new(Subtype::Text, vec![rect]).contents(text)
    }
    /// Top-aligned text with explicit line breaks and wrapping inside the rectangle.
    /// Appearance regeneration after resizing reflows text at the same font size.
    pub fn free_text(rect: Rect, text: impl Into<String>, font_size: f32) -> Self {
        let mut s = Self::new(Subtype::FreeText, vec![rect]).contents(text);
        s.font_size = font_size;
        s
    }
    pub fn ink(strokes: Vec<Vec<Point>>, width: f32) -> Self {
        let mut s = Self::new(Subtype::Ink, vec![]);
        s.strokes = strokes;
        s.width = width;
        s
    }
    pub fn contents(mut self, text: impl Into<String>) -> Self {
        self.contents = text.into();
        self
    }
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
    /// Interior color for square and circle annotations. Its alpha must match
    /// the stroke color: PDF annotation opacity applies to the whole appearance.
    pub fn fill(mut self, color: Color) -> Self {
        self.fill = Some(color);
        self
    }
    pub fn border_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

fn dictionary<T>(object: &ObjectRef, f: impl FnOnce(&mut CpdfDictionary) -> T) -> T {
    let mut value = object.borrow_mut();
    f(value.as_any_mut().downcast_mut::<CpdfDictionary>().unwrap())
}
fn core_rect(r: Rect) -> CfxFloatRect {
    CfxFloatRect::new(r.left, r.bottom, r.right, r.top)
}
fn valid_rect(r: Rect) -> bool {
    [r.left, r.bottom, r.right, r.top, r.width(), r.height()]
        .into_iter()
        .all(f32::is_finite)
        && r.width() > 0.0
        && r.height() > 0.0
}
fn bounds(rects: &[Rect]) -> Result<Rect> {
    if rects.is_empty() || !rects.iter().copied().all(valid_rect) {
        return Err(Error::InvalidArgument(
            "annotation rectangles must be finite and nonempty",
        ));
    }
    let mut r = rects[0];
    for other in &rects[1..] {
        r.left = r.left.min(other.left);
        r.bottom = r.bottom.min(other.bottom);
        r.right = r.right.max(other.right);
        r.top = r.top.max(other.top);
    }
    if !valid_rect(r) {
        return Err(Error::InvalidArgument("annotation bounds overflow"));
    }
    Ok(r)
}
fn ink_bounds(strokes: &[Vec<Point>], width: f32) -> Result<Rect> {
    if !width.is_finite()
        || width <= 0.0
        || strokes.is_empty()
        || strokes.iter().any(|s| s.len() < 2)
    {
        return Err(Error::InvalidArgument(
            "ink needs strokes of at least two points and a positive width",
        ));
    }
    let p = strokes[0][0];
    let mut r = Rect {
        left: p.x,
        right: p.x,
        bottom: p.y,
        top: p.y,
    };
    for p in strokes.iter().flatten() {
        if !p.x.is_finite() || !p.y.is_finite() {
            return Err(Error::InvalidArgument("ink coordinates must be finite"));
        }
        r.left = r.left.min(p.x);
        r.right = r.right.max(p.x);
        r.bottom = r.bottom.min(p.y);
        r.top = r.top.max(p.y);
    }
    if ![
        r.left - width,
        r.right + width,
        r.bottom - width,
        r.top + width,
        r.width(),
        r.height(),
    ]
    .into_iter()
    .all(f32::is_finite)
    {
        return Err(Error::InvalidArgument("ink bounds overflow"));
    }
    Ok(r)
}
fn markup(kind: Subtype) -> bool {
    matches!(
        kind,
        Subtype::Highlight | Subtype::Underline | Subtype::StrikeOut | Subtype::Squiggly
    )
}
fn supported(kind: Subtype) -> Result<()> {
    if markup(kind)
        || matches!(
            kind,
            Subtype::Square | Subtype::Circle | Subtype::Text | Subtype::FreeText | Subtype::Ink
        )
    {
        Ok(())
    } else {
        Err(Error::Unsupported(
            "appearance editing for this annotation subtype",
        ))
    }
}
fn set_rects(object: &ObjectRef, rects: &[Rect], rect: Rect) {
    dictionary(object, |d| {
        d.remove_for(b"QuadPoints");
    });
    for r in rects {
        sdk::set_attachment_points(
            object,
            None,
            [
                r.left, r.top, r.right, r.top, r.left, r.bottom, r.right, r.bottom,
            ],
        );
    }
    sdk::set_rect(object, core_rect(rect));
}
fn rgba(c: Color) -> [u32; 4] {
    [c.red.into(), c.green.into(), c.blue.into(), c.alpha.into()]
}
fn set_color(object: &ObjectRef, color: Color) -> bool {
    if sdk::subtype(object) == Subtype::FreeText {
        // FreeText C is background color; text color belongs to DA. Preserve
        // an imported annotation's background when changing its text style.
        dictionary(object, |d| {
            d.set_new_for(b"CA", pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::CpdfScalarObject::number_from_float(color.alpha as f32 / 255.0));
        });
        true
    } else {
        sdk::set_color(object, false, rgba(color))
    }
}
fn set_da(object: &ObjectRef, size: Option<f32>, color: Color) {
    dictionary(object, |d| {
        let font = CpdfDefaultAppearance::from_dicts(Some(d), None).font();
        let name = font
            .as_ref()
            .filter(|f| !f.name.is_empty())
            .map_or(b"Helv".as_slice(), |f| f.name.as_slice());
        let name = pdfoxium_core::fpdfapi::parser::fpdf_parser_utility::pdf_name_encode(name);
        let size = size.unwrap_or_else(|| font.as_ref().map_or(12.0, |f| f.size));
        let mut da = vec![b'/'];
        da.extend(name);
        da.extend(
            format!(
                " {size} Tf {} {} {} rg",
                color.red as f32 / 255.0,
                color.green as f32 / 255.0,
                color.blue as f32 / 255.0
            )
            .bytes(),
        );
        d.set_new_string_for(b"DA", &da);
    });
}
fn regenerate(page: &CpdfPage, doc: &Rc<RefCell<CpdfDocument>>, object: &ObjectRef) -> Result<()> {
    let kind = sdk::subtype(object);
    supported(kind)?;
    // PDFium inflates ink Rect during generation. Recompute its uninflated
    // bounds each time, rather than accumulating padding on every edit.
    if kind == Subtype::Ink {
        let strokes: Vec<Vec<Point>> = (0..sdk::ink_list_count(object))
            .filter_map(|i| sdk::ink_list_path(object, i))
            .map(|s| s.into_iter().map(|p| Point { x: p.x, y: p.y }).collect())
            .collect();
        let width = dictionary(object, |d| {
            d.get_dict_for(b"BS").and_then(|bs| {
                let bs = bs.borrow();
                let bs = bs.as_any().downcast_ref::<CpdfDictionary>()?;
                bs.key_exist(b"W").then(|| bs.get_float_for(b"W"))
            })
        })
        .unwrap_or_else(|| sdk::border(object).map_or(1.0, |b| b[2]));
        let r = ink_bounds(&strokes, width)?;
        sdk::set_rect(object, core_rect(r));
    }
    let (holder, root) = {
        let doc = doc.borrow();
        (doc.holder(), doc.root())
    };
    if kind == Subtype::FreeText {
        // Native FreeText AP generation initializes resources only when the
        // entire AcroForm is absent. An editor must also support existing forms
        // without DR/Font (e.g. a book with animation widgets). Preserve their
        // fields, DA and existing resources; only supply missing dictionaries.
        if let Some(form) = root
            .as_ref()
            .and_then(|r| dictionary(r, |d| d.get_dict_for(b"AcroForm")))
        {
            let resources = dictionary(&form, |d| {
                if d.key_exist(b"DR") && d.get_dict_for(b"DR").is_none() {
                    return Err(Error::InvalidArgument("AcroForm DR is not a dictionary"));
                }
                Ok(d.get_or_create_dict_for(b"DR"))
            })?;
            dictionary(&resources, |d| {
                if d.key_exist(b"Font") && d.get_dict_for(b"Font").is_none() {
                    return Err(Error::InvalidArgument(
                        "AcroForm Font resources are not a dictionary",
                    ));
                }
                d.get_or_create_dict_for(b"Font");
                Ok(())
            })?;
        }
    }
    let generated = if kind == Subtype::FreeText {
        generate_free_text_ap_for_object(
            &holder,
            root.as_ref(),
            &page.page_data_ref(),
            object,
            true,
        )
    } else {
        generate_annot_ap_for_object(&holder, root.as_ref(), &page.page_data_ref(), object, kind)
    };
    if generated {
        Ok(())
    } else {
        Err(Error::Edit)
    }
}
impl Page {
    /// Identities of current annotations in array order. They survive page unload.
    pub fn annotation_ids(&self) -> Result<Vec<AnnotId>> {
        self.with_annotations(false, |page, _, registry| {
            (0..sdk::annotation_count(page))
                .filter_map(|i| sdk::annotation_at(page, i))
                .map(|o| registry.register(self.id(), o))
                .collect()
        })
    }
    pub fn add_annotation(&self, spec: AnnotationSpec) -> Result<AnnotId> {
        self.add_annotation_with_forms(spec, false)
    }
    pub(crate) fn add_annotation_with_forms(
        &self,
        spec: AnnotationSpec,
        allow_forms: bool,
    ) -> Result<AnnotId> {
        let rect = if spec.kind == Subtype::Ink {
            ink_bounds(&spec.strokes, spec.width)?
        } else {
            bounds(&spec.rects)?
        };
        if !spec.width.is_finite()
            || spec.width <= 0.0
            || !spec.font_size.is_finite()
            || spec.font_size <= 0.0
        {
            return Err(Error::InvalidArgument(
                "annotation width and font size must be positive and finite",
            ));
        }
        if spec.fill.is_some() && !matches!(spec.kind, Subtype::Square | Subtype::Circle) {
            return Err(Error::Unsupported(
                "interior fill is only supported for square and circle",
            ));
        }
        if spec.fill.is_some_and(|fill| fill.alpha != spec.color.alpha) {
            return Err(Error::InvalidArgument(
                "fill and stroke must share annotation opacity",
            ));
        }
        self.with_annotation_access(true, allow_forms, |page, doc, registry| {
            let object = sdk::create_annotation(page, spec.kind as i32).ok_or(Error::Edit)?;
            let result = (|| {
                sdk::set_rect(&object, core_rect(rect));
                sdk::set_flags(&object, 4);
                sdk::set_string(
                    &object,
                    b"Contents",
                    &spec.contents.chars().map(u32::from).collect::<Vec<_>>(),
                );
                set_color(&object, spec.color);
                if let Some(fill) = spec.fill {
                    sdk::set_color(&object, true, rgba(fill));
                }
                sdk::set_border(&object, [0.0, 0.0, spec.width]);
                if markup(spec.kind) {
                    set_rects(&object, &spec.rects, rect);
                }
                if spec.kind == Subtype::Ink {
                    for stroke in &spec.strokes {
                        sdk::add_ink_stroke(
                            &object,
                            stroke.iter().map(|p| CfxPointF::new(p.x, p.y)),
                        )
                        .ok_or(Error::Edit)?;
                    }
                }
                if spec.kind == Subtype::FreeText {
                    set_da(&object, Some(spec.font_size), spec.color);
                }
                regenerate(page, doc, &object)?;
                registry.register(self.id(), object.clone())
            })();
            if result.is_err() {
                if let Some(index) = sdk::annotation_index(page, &object) {
                    sdk::remove_annotation(page, index);
                }
            }
            result
        })
    }
    pub fn remove_annotation(&self, id: AnnotId) -> Result<()> {
        self.remove_annotation_with_forms(id, false)
    }
    pub(crate) fn remove_annotation_with_forms(
        &self,
        id: AnnotId,
        allow_forms: bool,
    ) -> Result<()> {
        self.with_annotation_access(true, allow_forms, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let index = sdk::annotation_index(page, &o).ok_or(Error::Stale)?;
            if !sdk::remove_annotation(page, index) {
                return Err(Error::Edit);
            }
            registry.entries.remove(&id.slot);
            Ok(())
        })
    }
    pub fn set_annotation_contents(&self, id: AnnotId, text: &str) -> Result<()> {
        self.set_annotation_contents_with_forms(id, text, false)
    }
    pub(crate) fn set_annotation_contents_with_forms(
        &self,
        id: AnnotId,
        text: &str,
        allow_forms: bool,
    ) -> Result<()> {
        self.with_annotation_access(true, allow_forms, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            sdk::set_string(
                &o,
                b"Contents",
                &text.chars().map(u32::from).collect::<Vec<_>>(),
            );
            if sdk::subtype(&o) == Subtype::FreeText {
                regenerate(page, doc, &o)?;
            }
            Ok(())
        })
    }
    pub fn set_annotation_flags(&self, id: AnnotId, flags: i32) -> Result<()> {
        self.with_annotations(true, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            sdk::set_flags(&o, flags);
            Ok(())
        })
    }
    /// Read editable dictionary/DA style even when an appearance already exists.
    /// Custom appearance graphics are not inferred or changed by this query.
    pub fn annotation_style(&self, id: AnnotId) -> Result<Option<AnnotationStyle>> {
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let borrowed = o.borrow();
            let d = borrowed
                .as_any()
                .downcast_ref::<CpdfDictionary>()
                .ok_or(Error::Format)?;
            let opacity = if d.key_exist(b"CA") {
                d.get_float_for(b"CA")
            } else {
                1.
            };
            let alpha = (if opacity.is_finite() {
                opacity.clamp(0., 1.)
            } else {
                1.
            } * 255.)
                .round() as u8;
            let color = |key: &[u8], default: Option<Color>| {
                use pdfoxium_core::{
                    fpdfapi::parser::cpdf_array::CpdfArray,
                    fpdfdoc::cpdf_color_utils::cfx_color_from_array, fxge::cfx_color::ColorType,
                };
                let Some(array) = d.get_array_for(key) else {
                    return default;
                };
                let array = array.borrow();
                let array = array.as_any().downcast_ref::<CpdfArray>()?;
                let c = cfx_color_from_array(array);
                if c.color_type == ColorType::Transparent {
                    return None;
                }
                let c = c.convert_color_type(ColorType::Rgb);
                Some(Color::rgba(
                    (c.color1.clamp(0., 1.) * 255.).round() as u8,
                    (c.color2.clamp(0., 1.) * 255.).round() as u8,
                    (c.color3.clamp(0., 1.) * 255.).round() as u8,
                    alpha,
                ))
            };
            Ok(match sdk::subtype(&o) {
                Subtype::FreeText => {
                    let da = CpdfDefaultAppearance::from_dicts(Some(d), None);
                    let font_size = da.font().map_or(12., |f| f.size);
                    let mut color = da_color(&da).unwrap_or(Color::BLACK);
                    color.alpha = alpha;
                    Some(AnnotationStyle::FreeText { font_size, color })
                }
                Subtype::Square | Subtype::Circle => {
                    let border_width = d
                        .get_dict_for(b"BS")
                        .and_then(|bs| {
                            let bs = bs.borrow();
                            let bs = bs.as_any().downcast_ref::<CpdfDictionary>()?;
                            bs.key_exist(b"W").then(|| bs.get_float_for(b"W"))
                        })
                        .unwrap_or_else(|| sdk::border(&o).map_or(1., |v| v[2]));
                    Some(AnnotationStyle::Shape {
                        border_color: color(b"C", Some(Color::rgba(0, 0, 0, alpha)))
                            .unwrap_or(Color::rgba(0, 0, 0, alpha)),
                        border_width,
                        fill: color(b"IC", None),
                    })
                }
                Subtype::Highlight => Some(AnnotationStyle::Highlight {
                    color: color(b"C", Some(Color::rgba(255, 255, 0, alpha)))
                        .unwrap_or(Color::rgba(255, 255, 0, alpha)),
                }),
                _ => None,
            })
        })
    }
    /// Apply a complete style and regenerate the appearance once.
    pub fn set_annotation_style(&self, id: AnnotId, style: AnnotationStyle) -> Result<()> {
        self.set_annotation_style_with_forms(id, style, false)
    }
    pub(crate) fn set_annotation_style_with_forms(
        &self,
        id: AnnotId,
        style: AnnotationStyle,
        allow_forms: bool,
    ) -> Result<()> {
        style.validate_for(self.annotation(id)?.kind)?;
        self.with_annotation_access(true, allow_forms, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            dictionary(&o, |d| {
                d.remove_for(b"AP");
            });
            match style {
                AnnotationStyle::FreeText { font_size, color } => {
                    set_color(&o, color);
                    set_da(&o, Some(font_size), color);
                }
                AnnotationStyle::Shape {
                    border_color,
                    border_width,
                    fill,
                } => {
                    dictionary(&o, |d| {
                        d.remove_for(b"BS");
                    });
                    sdk::set_border(&o, [0., 0., border_width]);
                    if !set_color(&o, border_color) {
                        return Err(Error::Edit);
                    }
                    if let Some(fill) = fill {
                        if !sdk::set_color(&o, true, rgba(fill)) {
                            return Err(Error::Edit);
                        }
                    } else {
                        dictionary(&o, |d| {
                            d.remove_for(b"IC");
                        });
                    }
                }
                AnnotationStyle::Highlight { color } => {
                    if !set_color(&o, color) {
                        return Err(Error::Edit);
                    }
                }
            }
            regenerate(page, doc, &o)
        })
    }
    pub fn set_annotation_color(&self, id: AnnotId, color: Color) -> Result<()> {
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            supported(sdk::subtype(&o))?;
            dictionary(&o, |d| {
                d.remove_for(b"AP");
            });
            if !set_color(&o, color) {
                return Err(Error::Edit);
            }
            if sdk::subtype(&o) == Subtype::FreeText {
                set_da(&o, None, color);
            }
            regenerate(page, doc, &o)
        })
    }
    /// Move or resize a square, circle, note or free-text annotation. For text
    /// markup use `set_annotation_rects`; for ink replace the strokes instead.
    pub fn set_annotation_rect(&self, id: AnnotId, rect: Rect) -> Result<()> {
        self.set_annotation_rect_with_forms(id, rect, false)
    }
    pub(crate) fn set_annotation_rect_with_forms(
        &self,
        id: AnnotId,
        rect: Rect,
        allow_forms: bool,
    ) -> Result<()> {
        bounds(&[rect])?;
        self.with_annotation_access(true, allow_forms, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if !matches!(
                sdk::subtype(&o),
                Subtype::Square | Subtype::Circle | Subtype::Text | Subtype::FreeText
            ) {
                return Err(Error::Unsupported(
                    "use markup rectangles or ink strokes to change this annotation's geometry",
                ));
            }
            sdk::set_rect(&o, core_rect(rect));
            regenerate(page, doc, &o)
        })
    }
    pub fn set_annotation_rects(&self, id: AnnotId, rects: &[Rect]) -> Result<()> {
        let rect = bounds(rects)?;
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if !markup(sdk::subtype(&o)) {
                return Err(Error::Unsupported(
                    "rectangles require a text markup annotation",
                ));
            }
            set_rects(&o, rects, rect);
            regenerate(page, doc, &o)
        })
    }
    pub fn set_annotation_ink(
        &self,
        id: AnnotId,
        strokes: &[Vec<Point>],
        width: f32,
    ) -> Result<()> {
        let rect = ink_bounds(strokes, width)?;
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if sdk::subtype(&o) != Subtype::Ink {
                return Err(Error::Unsupported("strokes require an ink annotation"));
            }
            sdk::remove_ink_list(&o);
            dictionary(&o, |d| {
                d.remove_for(b"BS");
            });
            sdk::set_border(&o, [0.0, 0.0, width]);
            sdk::set_rect(&o, core_rect(rect));
            for s in strokes {
                sdk::add_ink_stroke(&o, s.iter().map(|p| CfxPointF::new(p.x, p.y)))
                    .ok_or(Error::Edit)?;
            }
            regenerate(page, doc, &o)
        })
    }
    /// Set the stroke width of square, circle or ink annotations.
    pub fn set_annotation_border_width(&self, id: AnnotId, width: f32) -> Result<()> {
        if !width.is_finite() || width <= 0.0 {
            return Err(Error::InvalidArgument(
                "border width must be positive and finite",
            ));
        }
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if !matches!(
                sdk::subtype(&o),
                Subtype::Square | Subtype::Circle | Subtype::Ink
            ) {
                return Err(Error::Unsupported(
                    "stroke width for this annotation subtype",
                ));
            }
            dictionary(&o, |d| {
                d.remove_for(b"BS");
            });
            sdk::set_border(&o, [0.0, 0.0, width]);
            regenerate(page, doc, &o)
        })
    }
    /// Set free-text font size and text color, preserving the font resource name.
    pub fn set_annotation_text_style(&self, id: AnnotId, size: f32, color: Color) -> Result<()> {
        if !size.is_finite() || size <= 0.0 {
            return Err(Error::InvalidArgument(
                "font size must be positive and finite",
            ));
        }
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if sdk::subtype(&o) != Subtype::FreeText {
                return Err(Error::Unsupported(
                    "text style requires a free-text annotation",
                ));
            }
            dictionary(&o, |d| {
                d.remove_for(b"AP");
            });
            set_color(&o, color);
            set_da(&o, Some(size), color);
            regenerate(page, doc, &o)
        })
    }
}

/// PDF annotation subtype. Creation supports the same subset as PDFium.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum AnnotationKind {
    Unknown,
    Text,
    Link,
    FreeText,
    Line,
    Square,
    Circle,
    Polygon,
    PolyLine,
    Highlight,
    Underline,
    Squiggly,
    StrikeOut,
    Stamp,
    Caret,
    Ink,
    Popup,
    FileAttachment,
    Sound,
    Movie,
    Widget,
    Screen,
    PrinterMark,
    TrapNet,
    Watermark,
    ThreeD,
    RichMedia,
    XfaWidget,
    Redact,
}
impl AnnotationKind {
    pub fn supports_creation(self) -> bool {
        sdk::supported_subtype(self as i32)
    }
    pub fn supports_objects(self) -> bool {
        sdk::object_supported_subtype(self as i32)
    }
    fn from_core(value: Subtype) -> Self {
        const ALL: [AnnotationKind; 29] = [
            AnnotationKind::Unknown,
            AnnotationKind::Text,
            AnnotationKind::Link,
            AnnotationKind::FreeText,
            AnnotationKind::Line,
            AnnotationKind::Square,
            AnnotationKind::Circle,
            AnnotationKind::Polygon,
            AnnotationKind::PolyLine,
            AnnotationKind::Highlight,
            AnnotationKind::Underline,
            AnnotationKind::Squiggly,
            AnnotationKind::StrikeOut,
            AnnotationKind::Stamp,
            AnnotationKind::Caret,
            AnnotationKind::Ink,
            AnnotationKind::Popup,
            AnnotationKind::FileAttachment,
            AnnotationKind::Sound,
            AnnotationKind::Movie,
            AnnotationKind::Widget,
            AnnotationKind::Screen,
            AnnotationKind::PrinterMark,
            AnnotationKind::TrapNet,
            AnnotationKind::Watermark,
            AnnotationKind::ThreeD,
            AnnotationKind::RichMedia,
            AnnotationKind::XfaWidget,
            AnnotationKind::Redact,
        ];
        ALL[value as usize]
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(i32)]
pub enum AppearanceMode {
    Normal,
    Rollover,
    Down,
}
/// Raw PDF dictionary value type; a reference stays a reference here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AnnotationValueType {
    Boolean,
    Number,
    String,
    Name,
    Array,
    Dictionary,
    Stream,
    Null,
    Reference,
}
impl AnnotationValueType {
    fn from_core(value: pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::ObjectType) -> Self {
        use pdfoxium_core::fpdfapi::parser::cpdf_scalar_object::ObjectType as T;
        match value {
            T::Boolean => Self::Boolean,
            T::Number => Self::Number,
            T::String => Self::String,
            T::Name => Self::Name,
            T::Array => Self::Array,
            T::Dictionary => Self::Dictionary,
            T::Stream => Self::Stream,
            T::Null => Self::Null,
            T::Reference => Self::Reference,
        }
    }
}
/// Detached annotation geometry snapshot. Quad point order follows PDFium:
/// top-left, top-right, bottom-left, bottom-right (not polygon winding order).
#[derive(Clone, Debug)]
pub struct AnnotationDetails {
    pub kind: AnnotationKind,
    pub rect: Rect,
    pub flags: i32,
    pub border: Option<[f32; 3]>,
    pub has_attachment_points: bool,
    pub quads: Vec<[Point; 4]>,
    pub vertices: Option<Vec<Point>>,
    pub ink: Vec<Vec<Point>>,
    pub line: Option<[Point; 2]>,
}
fn public_point(p: CfxPointF) -> Point {
    Point { x: p.x, y: p.y }
}
fn public_rect(r: CfxFloatRect) -> Rect {
    Rect {
        left: r.left,
        bottom: r.bottom,
        right: r.right,
        top: r.top,
    }
}
fn text_string(text: Vec<u32>) -> String {
    text.into_iter()
        .map(|c| char::from_u32(c).unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}
fn key_valid(key: &[u8]) -> Result<()> {
    if key.is_empty() || key.contains(&0) {
        Err(Error::InvalidArgument(
            "dictionary key must be nonempty and contain no NUL",
        ))
    } else {
        Ok(())
    }
}
impl Page {
    /// Create an annotation without generating an appearance. Use this for links,
    /// stamps, popups, attachments, or supplying an explicit appearance stream.
    pub fn create_annotation(&self, kind: AnnotationKind, rect: Rect) -> Result<AnnotId> {
        bounds(&[rect])?;
        if !kind.supports_creation() {
            return Err(Error::Unsupported(
                "annotation subtype cannot be created by PDFium",
            ));
        }
        self.with_annotations(true, |page, _, registry| {
            let o = sdk::create_annotation(page, kind as i32).ok_or(Error::Edit)?;
            sdk::set_rect(&o, core_rect(rect));
            registry.register(self.id(), o)
        })
    }
    pub fn annotation(&self, id: AnnotId) -> Result<AnnotationDetails> {
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            Ok(AnnotationDetails {
                kind: AnnotationKind::from_core(sdk::subtype(&o)),
                rect: public_rect(sdk::rect(&o)),
                flags: sdk::flags(&o),
                border: sdk::border(&o),
                has_attachment_points: sdk::has_attachment_points(&o),
                quads: (0..sdk::attachment_points_count(&o))
                    .filter_map(|i| sdk::attachment_points(&o, i))
                    .map(|q| {
                        std::array::from_fn(|i| Point {
                            x: q[i * 2],
                            y: q[i * 2 + 1],
                        })
                    })
                    .collect(),
                vertices: sdk::vertices(&o).map(|v| v.into_iter().map(public_point).collect()),
                ink: (0..sdk::ink_list_count(&o))
                    .filter_map(|i| sdk::ink_list_path(&o, i))
                    .map(|s| s.into_iter().map(public_point).collect())
                    .collect(),
                line: sdk::line(&o).map(|p| p.map(public_point)),
            })
        })
    }
    /// Set the annotation rectangle using PDFium's low-level semantics, without
    /// replacing its appearance. Unlike `set_annotation_rect`, works for stamps.
    pub fn set_annotation_bounds(&self, id: AnnotId, rect: Rect) -> Result<()> {
        self.set_annotation_bounds_with_forms(id, rect, false)
    }
    pub(crate) fn set_annotation_bounds_with_forms(
        &self,
        id: AnnotId,
        rect: Rect,
        allow_forms: bool,
    ) -> Result<()> {
        bounds(&[rect])?;
        self.with_annotation_access(true, allow_forms, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            sdk::set_rect(&o, core_rect(rect));
            Ok(())
        })
    }
    /// Raw appearance commands, decoded as PDF text just like FPDFAnnot_GetAP.
    /// Missing appearance returns None. Binary streams should be read as bytes.
    pub fn annotation_appearance(
        &self,
        id: AnnotId,
        mode: AppearanceMode,
    ) -> Result<Option<String>> {
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            Ok(sdk::appearance(&o, mode as i32, false)
                .map(|_| text_string(sdk::appearance_text(&o, mode as i32).unwrap_or_default())))
        })
    }
    /// Set PDF appearance content. None removes the chosen appearance; removing
    /// Normal removes the entire AP dictionary, matching PDFium. The content
    /// must use resources already available to the appearance/page.
    pub fn set_annotation_appearance(
        &self,
        id: AnnotId,
        mode: AppearanceMode,
        commands: Option<&str>,
    ) -> Result<()> {
        let text = commands.map(|s| s.chars().map(u32::from).collect::<Vec<_>>());
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let holder = doc.borrow().holder();
            if sdk::set_appearance(&o, &holder, mode as i32, text.as_deref()) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
    pub fn annotation_has_key(&self, id: AnnotId, key: &[u8]) -> Result<bool> {
        key_valid(key)?;
        self.with_annotations(false, |page, _, registry| {
            Ok(sdk::has_key(&registry.resolve(page, self.id(), id)?, key))
        })
    }
    pub fn annotation_value_type(
        &self,
        id: AnnotId,
        key: &[u8],
    ) -> Result<Option<AnnotationValueType>> {
        key_valid(key)?;
        self.with_annotations(false, |page, _, registry| {
            Ok(
                sdk::value_type(&registry.resolve(page, self.id(), id)?, key)
                    .map(AnnotationValueType::from_core),
            )
        })
    }
    pub fn annotation_string(&self, id: AnnotId, key: &[u8]) -> Result<String> {
        key_valid(key)?;
        self.with_annotations(false, |page, _, registry| {
            Ok(text_string(sdk::string(
                &registry.resolve(page, self.id(), id)?,
                key,
            )))
        })
    }
    pub fn annotation_number(&self, id: AnnotId, key: &[u8]) -> Result<Option<f32>> {
        key_valid(key)?;
        self.with_annotations(false, |page, _, registry| {
            Ok(sdk::number(&registry.resolve(page, self.id(), id)?, key))
        })
    }
    /// Set a dictionary string without regenerating appearance. Use the typed
    /// content/style setters when a visible appearance update is wanted.
    pub fn set_annotation_string(&self, id: AnnotId, key: &[u8], value: &str) -> Result<()> {
        key_valid(key)?;
        self.with_annotations(true, |page, _, registry| {
            sdk::set_string(
                &registry.resolve(page, self.id(), id)?,
                key,
                &value.chars().map(u32::from).collect::<Vec<_>>(),
            );
            Ok(())
        })
    }
    pub fn linked_annotation(&self, id: AnnotId, key: &[u8]) -> Result<Option<AnnotId>> {
        key_valid(key)?;
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let Some(linked) = sdk::linked_annotation(&o, key) else {
                return Ok(None);
            };
            let outside = sdk::annotation_index(page, &linked).is_none();
            let id = registry.register(self.id(), linked)?;
            if outside {
                registry.linked.insert(id.slot);
            }
            Ok(Some(id))
        })
    }
    pub fn set_annotation_uri(&self, id: AnnotId, uri: &[u8]) -> Result<()> {
        if uri.contains(&0) {
            return Err(Error::InvalidArgument("URI must contain no NUL"));
        }
        self.with_annotations(true, |page, _, registry| {
            if sdk::set_uri(&registry.resolve(page, self.id(), id)?, uri) {
                Ok(())
            } else {
                Err(Error::Unsupported("URI requires a link annotation"))
            }
        })
    }
    /// Raw border setter. It removes AP, as PDFium does, and does not generate it.
    pub fn set_annotation_border(&self, id: AnnotId, border: [f32; 3]) -> Result<()> {
        if !border.into_iter().all(|v| v.is_finite() && v >= 0.) {
            return Err(Error::InvalidArgument(
                "border values must be finite and nonnegative",
            ));
        }
        self.with_annotations(true, |page, _, registry| {
            sdk::set_border(&registry.resolve(page, self.id(), id)?, border);
            Ok(())
        })
    }
    /// Dictionary color is unavailable when a normal appearance exists, matching PDFium.
    pub fn annotation_color(&self, id: AnnotId, interior: bool) -> Result<Option<Color>> {
        self.with_annotations(false, |page, _, registry| {
            Ok(
                sdk::color(&registry.resolve(page, self.id(), id)?, interior)
                    .map(|c| Color::rgba(c[0] as u8, c[1] as u8, c[2] as u8, c[3] as u8)),
            )
        })
    }
    /// Raw color setter. An existing appearance must first be explicitly removed.
    pub fn set_annotation_dictionary_color(
        &self,
        id: AnnotId,
        interior: bool,
        color: Color,
    ) -> Result<()> {
        self.with_annotations(true, |page, _, registry| {
            if sdk::set_color(
                &registry.resolve(page, self.id(), id)?,
                interior,
                rgba(color),
            ) {
                Ok(())
            } else {
                Err(Error::Unsupported(
                    "remove the normal appearance before setting dictionary color",
                ))
            }
        })
    }
    /// Replace or append a PDFium-ordered attachment quad. Preserves custom AP.
    pub fn set_annotation_quad(
        &self,
        id: AnnotId,
        index: Option<usize>,
        quad: [Point; 4],
    ) -> Result<()> {
        if !quad.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Err(Error::InvalidArgument("quad coordinates must be finite"));
        }
        let q = std::array::from_fn(|i| {
            if i % 2 == 0 {
                quad[i / 2].x
            } else {
                quad[i / 2].y
            }
        });
        self.with_annotations(true, |page, _, registry| {
            if sdk::set_attachment_points(&registry.resolve(page, self.id(), id)?, index, q) {
                Ok(())
            } else {
                Err(Error::InvalidArgument(
                    "annotation has no attachment points or index is out of range",
                ))
            }
        })
    }
    /// Append an ink stroke without regenerating AP. Returns its zero-based index.
    pub fn add_annotation_ink_stroke(&self, id: AnnotId, points: &[Point]) -> Result<usize> {
        if points.is_empty() || !points.iter().all(|p| p.x.is_finite() && p.y.is_finite()) {
            return Err(Error::InvalidArgument("ink stroke needs finite points"));
        }
        self.with_annotations(true, |page, _, registry| {
            sdk::add_ink_stroke(
                &registry.resolve(page, self.id(), id)?,
                points.iter().map(|p| CfxPointF::new(p.x, p.y)),
            )
            .map(|i| i as usize)
            .ok_or(Error::InvalidArgument(
                "ink stroke requires an ink annotation",
            ))
        })
    }
    pub fn clear_annotation_ink(&self, id: AnnotId) -> Result<()> {
        self.with_annotations(true, |page, _, registry| {
            if sdk::remove_ink_list(&registry.resolve(page, self.id(), id)?) {
                Ok(())
            } else {
                Err(Error::Unsupported("ink list requires an ink annotation"))
            }
        })
    }
    /// Explicitly regenerate a supported built-in static appearance after raw edits.
    pub fn regenerate_annotation_appearance(&self, id: AnnotId) -> Result<()> {
        self.with_annotations(true, |page, doc, registry| {
            regenerate(page, doc, &registry.resolve(page, self.id(), id)?)
        })
    }
}

/// Snapshot identity of an object in an annotation's normal appearance.
/// This identity expires after any document edit, preventing index reuse from
/// silently selecting a different object. Use `Page::annotation_object` to
/// acquire a retained handle before editing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AnnotationObjectId {
    annotation: AnnotId,
    index: usize,
    revision: u64,
}
#[derive(Clone, Debug)]
pub struct AnnotationObjectInfo {
    pub id: AnnotationObjectId,
    pub kind: crate::ObjectKind,
    pub bounds: Rect,
    pub matrix: Option<crate::Matrix>,
    /// Path geometry is included only for path objects.
    pub path: Option<crate::edit::PathData>,
}
fn annotation_context(
    page: &CpdfPage,
    object: ObjectRef,
    append: bool,
) -> Result<pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext> {
    let mut context =
        pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext::new(object);
    let success = if append {
        sdk::ensure_append_form(&mut context, page)
    } else {
        sdk::ensure_form(&mut context, page)
    };
    if success {
        Ok(context)
    } else {
        Err(Error::Unsupported("annotation has no editable appearance"))
    }
}
impl Page {
    pub fn annotation_file_attachment(&self, id: AnnotId) -> Result<Option<crate::Attachment>> {
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            Ok(
                pdfoxium_core::fpdfsdk::fpdf_attachment::annotation_attachment(&o)
                    .map(|file| crate::Attachment::from_core(self.document().clone(), file)),
            )
        })
    }
    pub fn add_annotation_file_attachment(
        &self,
        id: AnnotId,
        name: &str,
    ) -> Result<crate::Attachment> {
        if name.is_empty() {
            return Err(Error::InvalidArgument("attachment name must be nonempty"));
        }
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let holder = doc.borrow().holder();
            let file = pdfoxium_core::fpdfsdk::fpdf_attachment::add_annotation_attachment(
                &o,
                &holder,
                &name.chars().map(u32::from).collect::<Vec<_>>(),
            )
            .ok_or(Error::Unsupported("requires a file-attachment annotation"))?;
            Ok(crate::Attachment::from_core(self.document().clone(), file))
        })
    }
    /// Enumerate normal appearance objects. Unlike page object IDs, these IDs
    /// expire at the next document edit. No core object is retained by a snapshot.
    pub fn annotation_objects(&self, id: AnnotId) -> Result<Vec<AnnotationObjectInfo>> {
        let revision = self.document().revision();
        self.with_annotations(false, |page, _, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            if sdk::appearance(&o, 0, true).is_none() {
                return Ok(vec![]);
            }
            let context = annotation_context(page, o, false)?;
            let holder = context.form().unwrap().holder();
            (0..holder.page_object_count())
                .map(|index| {
                    let object = holder.page_object_by_index(index).ok_or(Error::Edit)?;
                    use pdfoxium_core::fpdfapi::page::cpdf_pageobject::PageObjectType as T;
                    let kind = match object.object_type() {
                        T::Text => crate::ObjectKind::Text,
                        T::Path => crate::ObjectKind::Path,
                        T::Image => crate::ObjectKind::Image,
                        T::Shading => crate::ObjectKind::Shading,
                        T::Form => crate::ObjectKind::Form,
                    };
                    let matrix =
                        pdfoxium_core::fpdfsdk::fpdf_editstate::object_matrix(object).map(|m| {
                            crate::Matrix {
                                a: m.a,
                                b: m.b,
                                c: m.c,
                                d: m.d,
                                e: m.e,
                                f: m.f,
                            }
                        });
                    let path = if kind == crate::ObjectKind::Path {
                        Some(crate::edit::path_snapshot(object)?)
                    } else {
                        None
                    };
                    Ok(AnnotationObjectInfo {
                        id: AnnotationObjectId {
                            annotation: id,
                            index,
                            revision,
                        },
                        kind,
                        bounds: public_rect(object.base().rect()),
                        matrix,
                        path,
                    })
                })
                .collect()
        })
    }
    fn check_annotation_object(&self, id: AnnotationObjectId) -> Result<()> {
        if self.document().revision() != id.revision || self.id() != id.annotation.page {
            Err(Error::Stale)
        } else {
            Ok(())
        }
    }
    /// Transform an ink/stamp appearance object and immediately rewrite AP.
    pub fn transform_annotation_object(
        &self,
        id: AnnotationObjectId,
        matrix: crate::Matrix,
    ) -> Result<()> {
        self.check_annotation_object(id)?;
        if !matrix.is_finite() {
            return Err(Error::InvalidArgument("matrix must be finite"));
        }
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id.annotation)?;
            if !sdk::object_supported_subtype(sdk::subtype(&o) as i32) {
                return Err(Error::Unsupported(
                    "object editing requires ink or stamp annotation",
                ));
            }
            let mut context = annotation_context(page, o, false)?;
            let object = context
                .form_mut()
                .unwrap()
                .holder_mut()
                .page_object_by_index_mut(id.index)
                .ok_or(Error::Stale)?;
            object.transform(matrix.core());
            object.base_mut().set_dirty(true);
            pdfoxium_core::fpdfsdk::fpdf_editpage::calc_bounding_box(object);
            if sdk::update_content_stream(&mut context, &mut doc.borrow_mut()) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
    pub fn remove_annotation_object(&self, id: AnnotationObjectId) -> Result<()> {
        self.check_annotation_object(id)?;
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id.annotation)?;
            if !sdk::object_supported_subtype(sdk::subtype(&o) as i32) {
                return Err(Error::Unsupported(
                    "object editing requires ink or stamp annotation",
                ));
            }
            let mut context = annotation_context(page, o, false)?;
            if !context
                .form_mut()
                .unwrap()
                .holder_mut()
                .erase_page_object_at_index(id.index)
            {
                return Err(Error::Stale);
            }
            if sdk::update_content_stream(&mut context, &mut doc.borrow_mut()) {
                Ok(())
            } else {
                Err(Error::Edit)
            }
        })
    }
}
impl Page {
    /// Remove an ink/stamp appearance object and retain it as an owned object.
    /// The AP is regenerated before returning. The object remains document-bound.
    pub fn take_annotation_object(&self, id: AnnotationObjectId) -> Result<crate::DetachedObject> {
        self.check_annotation_object(id)?;
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id.annotation)?;
            if !sdk::object_supported_subtype(sdk::subtype(&o) as i32) {
                return Err(Error::Unsupported(
                    "object editing requires ink or stamp annotation",
                ));
            }
            let mut context = annotation_context(page, o, false)?;
            let holder = context.form_mut().unwrap().holder_mut();
            let core = holder
                .page_object_id_by_index(id.index)
                .ok_or(Error::Stale)?;
            let object = holder.remove_page_object(core).ok_or(Error::Stale)?;
            if !sdk::update_content_stream(&mut context, &mut doc.borrow_mut()) {
                return Err(Error::Edit);
            }
            Ok(crate::DetachedObject {
                core: object,
                document: self.document().clone(),
            })
        })
    }
    /// Append a document-bound object to an ink/stamp appearance and regenerate it.
    /// Insertion consumes the object, including on error; inspect the destination first.
    pub fn insert_annotation_object(
        &self,
        id: AnnotId,
        mut object: crate::DetachedObject,
    ) -> Result<AnnotationObjectId> {
        if !object.document.same_document(self.document()) {
            return Err(Error::InvalidArgument("object belongs to another document"));
        }
        if object.core.base().content_marks().count_items() != 0 {
            return Err(Error::Unsupported(
                "moving marked objects requires rebinding tags and properties",
            ));
        }
        let revision = self
            .document()
            .revision()
            .checked_add(1)
            .ok_or(Error::Internal("revision exhausted"))?;
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let mut context = annotation_context(page, o, true)?;
            let holder = context.form_mut().unwrap().holder_mut();
            let index = holder.page_object_count();
            object.core.base_mut().set_content_stream(
                pdfoxium_core::fpdfapi::page::cpdf_pageobject::CpdfPageObject::NO_CONTENT_STREAM,
            );
            object.core.base_mut().set_resource_name(&[]);
            object
                .core
                .base_mut()
                .mutable_general_state()
                .set_graphics_resource_names(Vec::new());
            object.core.base_mut().set_dirty(true);
            holder.append_page_object(object.core);
            if !sdk::update_content_stream(&mut context, &mut doc.borrow_mut()) {
                return Err(Error::Edit);
            }
            Ok(AnnotationObjectId {
                annotation: id,
                index,
                revision,
            })
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetFieldType {
    Unknown,
    PushButton,
    CheckBox,
    RadioButton,
    ComboBox,
    ListBox,
    Text,
    Signature,
}
#[derive(Clone, Debug)]
pub struct WidgetOption {
    pub label: String,
    pub value: String,
    pub selected: bool,
}
/// Static AcroForm field snapshot. Reading this does not create a form event
/// environment or execute JavaScript, and does not lay out dynamic XFA.
#[derive(Clone, Debug)]
pub struct WidgetFieldInfo {
    pub kind: WidgetFieldType,
    pub flags: u32,
    pub name: String,
    pub alternate_name: String,
    pub value: String,
    pub options: Vec<WidgetOption>,
    pub control_count: usize,
    pub control_index: Option<usize>,
    pub checked: Option<bool>,
    pub export_value: Option<String>,
    pub font_size: Option<f32>,
    pub font_color: Option<Color>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldAction {
    KeyStroke,
    Format,
    Validate,
    Calculate,
}
fn static_form(
    doc: &Rc<RefCell<CpdfDocument>>,
    page: &CpdfPage,
) -> Rc<pdfoxium_core::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm> {
    let form = pdfoxium_core::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm::new(doc);
    form.fix_page_fields(page);
    form
}
fn da_color(da: &CpdfDefaultAppearance) -> Option<Color> {
    da.color_argb().map(|c| {
        Color::rgba(
            (c.argb >> 16) as u8,
            (c.argb >> 8) as u8,
            c.argb as u8,
            (c.argb >> 24) as u8,
        )
    })
}
impl Page {
    pub fn annotation_field(&self, id: AnnotId) -> Result<Option<WidgetFieldInfo>> {
        self.with_annotations(false, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let form = static_form(doc, page);
            let Some(field) = form.get_field_by_dict(&o) else {
                return Ok(None);
            };
            let control = form.get_control_by_dict(&o);
            let da = control.as_ref().map(|c| c.default_appearance());
            use pdfoxium_core::fpdfdoc::cpdf_formfield::FormFieldType as T;
            let kind = match field.field_type() {
                T::PushButton => WidgetFieldType::PushButton,
                T::CheckBox => WidgetFieldType::CheckBox,
                T::RadioButton => WidgetFieldType::RadioButton,
                T::ComboBox => WidgetFieldType::ComboBox,
                T::ListBox => WidgetFieldType::ListBox,
                T::TextField => WidgetFieldType::Text,
                T::Signature => WidgetFieldType::Signature,
                _ => WidgetFieldType::Unknown,
            };
            let button = matches!(
                kind,
                WidgetFieldType::CheckBox | WidgetFieldType::RadioButton
            );
            Ok(Some(WidgetFieldInfo {
                kind,
                flags: field.field_flags(),
                name: text_string(field.full_name()),
                alternate_name: text_string(field.alternate_name()),
                value: text_string(field.value()),
                options: (0..field.count_options())
                    .map(|i| WidgetOption {
                        label: text_string(field.option_label(i as i32)),
                        value: text_string(field.option_value(i as i32)),
                        selected: field.is_item_selected(i as i32),
                    })
                    .collect(),
                control_count: field.count_controls(),
                control_index: control
                    .as_ref()
                    .and_then(|c| usize::try_from(field.control_index(c)).ok()),
                checked: control.as_ref().filter(|_| button).map(|c| c.is_checked()),
                export_value: control
                    .as_ref()
                    .filter(|_| button)
                    .map(|c| text_string(c.export_value())),
                font_size: da.as_ref().and_then(|d| d.font()).map(|f| f.size),
                font_color: da.as_ref().and_then(da_color),
            }))
        })
    }
    /// Set AcroForm field flags without dispatching events or scripts.
    pub fn set_annotation_field_flags(&self, id: AnnotId, flags: u32) -> Result<()> {
        self.with_annotations(true, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let form = static_form(doc, page);
            let field = form
                .get_field_by_dict(&o)
                .ok_or(Error::Unsupported("annotation is not an AcroForm field"))?;
            field.set_field_flags(flags);
            Ok(())
        })
    }
    pub fn annotation_field_at(&self, point: Point) -> Result<Option<AnnotId>> {
        if !point.x.is_finite() || !point.y.is_finite() {
            return Err(Error::InvalidArgument("point must be finite"));
        }
        self.with_annotations(false, |page, doc, registry| {
            let form = static_form(doc, page);
            let Some((control, _)) =
                form.get_control_at_point(page, CfxPointF::new(point.x, point.y))
            else {
                return Ok(None);
            };
            registry
                .register(self.id(), control.widget_dict())
                .map(Some)
        })
    }
    /// Return a field's additional-action source as data; never execute it.
    pub fn annotation_field_script(
        &self,
        id: AnnotId,
        event: FieldAction,
    ) -> Result<Option<String>> {
        self.with_annotations(false, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let form = static_form(doc, page);
            let Some(field) = form.get_field_by_dict(&o) else {
                return Ok(None);
            };
            use pdfoxium_core::fpdfdoc::cpdf_aaction::AActionType as A;
            let action = match event {
                FieldAction::KeyStroke => A::KeyStroke,
                FieldAction::Format => A::Format,
                FieldAction::Validate => A::Validate,
                FieldAction::Calculate => A::Calculate,
            };
            Ok(Some(text_string(
                field.additional_action().action(action).javascript(),
            )))
        })
    }
    /// Default-appearance font style for free text or a static widget.
    pub fn annotation_font_style(&self, id: AnnotId) -> Result<Option<(f32, Color)>> {
        self.with_annotations(false, |page, doc, registry| {
            let o = registry.resolve(page, self.id(), id)?;
            let form = static_form(doc, page);
            let da = if sdk::subtype(&o) == Subtype::FreeText {
                let root = form.form_dict();
                let annot = o.borrow();
                let annot = annot
                    .as_any()
                    .downcast_ref::<CpdfDictionary>()
                    .ok_or(Error::Format)?;
                let root = root.as_ref().map(|r| r.borrow());
                let root = root
                    .as_ref()
                    .and_then(|r| r.as_any().downcast_ref::<CpdfDictionary>());
                CpdfDefaultAppearance::from_dicts(Some(annot), root)
            } else if let Some(control) = form.get_control_by_dict(&o) {
                control.default_appearance()
            } else {
                return Ok(None);
            };
            Ok(da
                .font()
                .and_then(|font| da_color(&da).map(|color| (font.size, color))))
        })
    }
}
impl AnnotationObjectId {
    pub fn index(self) -> usize {
        self.index
    }
}
/// A handle to one object in an annotation's normal appearance.
///
/// Handles retain their page/document and share a parsed appearance with other
/// handles acquired for that annotation. Setters immediately regenerate its AP;
/// clones and sibling handles remain usable. Other document edits and page
/// unloading invalidate the handles, returning `Error::Stale` on later access.
/// Obtain a fresh handle after those operations. No commit or callback is needed.
///
/// ```no_run
/// # use pdfoxium::{Page, AnnotId, Color, Matrix, Result};
/// # fn edit_appearance(page: &Page, stamp: AnnotId) -> Result<()> {
/// let objects = page.annotation_objects(stamp)?;
/// let object = page.annotation_object(objects[0].id)?;
/// let path = object.path()?;
/// object.set_fill_color(Color::rgb(0, 128, 255))?;
/// object.transform(Matrix::translate(10., 0.))?;
/// # Ok(())
/// # }
/// ```
#[derive(Clone)]
pub struct AnnotationObject {
    page: Page,
    annotation: AnnotId,
    index: usize,
    state: Rc<RefCell<AnnotationObjectState>>,
}

struct AnnotationObjectState {
    revision: u64,
    valid: bool,
    context: Option<pdfoxium_core::fpdfapi::page::cpdf_annotcontext::CpdfAnnotContext>,
}
impl AnnotationObjectState {
    fn invalidate(&mut self) {
        self.valid = false;
        self.context = None;
    }
    fn check(&mut self, revision: u64) -> Result<()> {
        if !self.valid || self.revision != revision {
            self.invalidate();
            Err(Error::Stale)
        } else if self.context.is_none() {
            Err(Error::Busy)
        } else {
            Ok(())
        }
    }
}
impl Page {
    /// Open a retained appearance-object handle from an enumeration snapshot.
    /// The snapshot ID must still match the current document revision.
    pub fn annotation_object(&self, id: AnnotationObjectId) -> Result<AnnotationObject> {
        self.check_annotation_object(id)?;
        self.with_annotations(false, |page, _, registry| {
            let object = registry.resolve(page, self.id(), id.annotation)?;
            let existing = registry
                .object_contexts
                .get(&id.annotation.slot)
                .and_then(Weak::upgrade);
            let state = if let Some(state) = existing {
                let usable = {
                    let mut value = state.try_borrow_mut().map_err(|_| Error::Busy)?;
                    match value.check(id.revision) {
                        Ok(()) => true,
                        Err(Error::Stale) => false,
                        Err(e) => return Err(e),
                    }
                };
                if usable {
                    Some(state)
                } else {
                    None
                }
            } else {
                None
            };
            let state = match state {
                Some(state) => state,
                None => {
                    let context = annotation_context(page, object, false)?;
                    let state = Rc::new(RefCell::new(AnnotationObjectState {
                        revision: id.revision,
                        valid: true,
                        context: Some(context),
                    }));
                    registry
                        .object_contexts
                        .retain(|_, state| state.strong_count() != 0);
                    registry
                        .object_contexts
                        .insert(id.annotation.slot, Rc::downgrade(&state));
                    state
                }
            };
            if state
                .borrow()
                .context
                .as_ref()
                .and_then(|c| c.form())
                .and_then(|f| f.holder().page_object_by_index(id.index))
                .is_none()
            {
                return Err(Error::Stale);
            }
            Ok(AnnotationObject {
                page: self.clone(),
                annotation: id.annotation,
                index: id.index,
                state,
            })
        })
    }
}
impl AnnotationObject {
    pub fn index(&self) -> usize {
        self.index
    }
    pub fn annotation_id(&self) -> AnnotId {
        self.annotation
    }

    /// Current snapshot identity, suitable for the page-level remove/take APIs.
    pub fn id(&self) -> Result<AnnotationObjectId> {
        self.page.with_annotations(false, |page, _, registry| {
            registry.resolve(page, self.page.id(), self.annotation)?;
            let mut state = self.state.try_borrow_mut().map_err(|_| Error::Busy)?;
            state.check(self.page.document().revision())?;
            Ok(AnnotationObjectId {
                annotation: self.annotation,
                index: self.index,
                revision: state.revision,
            })
        })
    }

    // Move, never clone, the selected object into the existing detached-object
    // adapter. The parsed form keeps its resources; no engine guard or RefCell
    // borrow spans the adapter call, whose font/image methods acquire guards.
    fn access<T>(
        &self,
        mutating: bool,
        operation: impl FnOnce(&mut crate::DetachedObject) -> Result<T>,
    ) -> Result<T> {
        let mut extracted = false;
        let prepared = self.page.with_annotations(false, |page, _, registry| {
            let current = registry.resolve(page, self.page.id(), self.annotation)?;
            let mut state = self.state.try_borrow_mut().map_err(|_| Error::Busy)?;
            state.check(self.page.document().revision())?;
            let context = state.context.as_mut().ok_or(Error::Busy)?;
            if !Rc::ptr_eq(&current, context.dict()) {
                return Err(Error::Stale);
            }
            if mutating && !sdk::object_supported_subtype(sdk::subtype(&current) as i32) {
                return Err(Error::Unsupported(
                    "object editing requires ink or stamp annotation",
                ));
            }
            let holder = context.form_mut().ok_or(Error::Format)?.holder_mut();
            let core = holder
                .page_object_id_by_index(self.index)
                .ok_or(Error::Stale)?;
            let object = holder.remove_page_object(core).ok_or(Error::Stale)?;
            extracted = true;
            Ok((
                state.context.take().unwrap(),
                crate::DetachedObject {
                    core: object,
                    document: self.page.document().clone(),
                },
            ))
        });
        let (mut context, mut object) = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                // A lazy input error can be reported after the closure ran.
                // Its returned context has already been dropped in that case.
                if extracted {
                    self.state.borrow_mut().invalidate();
                }
                return Err(error);
            }
        };
        let result = operation(&mut object);
        if mutating && result.is_err() {
            // Some detached operations can partially mutate before reporting an
            // error. Discard this private parse rather than commit it on retry.
            self.state.borrow_mut().invalidate();
            return result;
        }
        if mutating && object.core.base().content_marks().count_items() != 0 {
            self.state.borrow_mut().invalidate();
            return Err(Error::Unsupported(
                "editing marked appearance objects requires rebinding tags and properties",
            ));
        }
        let restored = self.page.with_annotations(mutating, |page, doc, registry| {
            let current = registry.resolve(page, self.page.id(), self.annotation)?;
            if !Rc::ptr_eq(&current, context.dict()) {
                return Err(Error::Stale);
            }
            let state = self.state.borrow();
            if !state.valid || state.revision != self.page.document().revision() {
                return Err(Error::Stale);
            }
            drop(state);
            if mutating {
                object.core.base_mut().set_dirty(true);
                pdfoxium_core::fpdfsdk::fpdf_editpage::calc_bounding_box(object.core.as_mut());
            }
            if !context
                .form_mut()
                .ok_or(Error::Format)?
                .holder_mut()
                .insert_page_object_at_index(self.index, object.core)
            {
                return Err(Error::Edit);
            }
            if mutating && !sdk::update_content_stream(&mut context, &mut doc.borrow_mut()) {
                return Err(Error::Edit);
            }
            Ok(())
        });
        let mut state = self.state.borrow_mut();
        if let Err(error) = restored {
            state.invalidate();
            return Err(error);
        }
        state.revision = self.page.document().revision();
        state.context = Some(context);
        result
    }
    pub fn kind(&self) -> Result<crate::ObjectKind> {
        self.access(false, |object| Ok(object.kind()))
    }
    pub fn bounds(&self) -> Result<Rect> {
        self.access(false, |object| Ok(object.bounds()))
    }
    pub fn style(&self) -> Result<crate::ObjectStyle> {
        self.access(false, |object| Ok(object.style()))
    }
    pub fn matrix(&self) -> Result<Option<crate::Matrix>> {
        self.access(false, |object| Ok(object.matrix()))
    }
    pub fn rotated_bounds(&self) -> Result<Option<[Point; 4]>> {
        self.access(false, |object| Ok(object.rotated_bounds()))
    }
    pub fn has_transparency(&self) -> Result<bool> {
        self.access(false, |object| Ok(object.has_transparency()))
    }
    pub fn marked_content_id(&self) -> Result<i32> {
        self.access(false, |object| Ok(object.marked_content_id()))
    }
    pub fn marks(&self) -> Result<Vec<crate::ContentMark>> {
        self.access(false, |object| Ok(object.marks()))
    }
    pub fn path(&self) -> Result<crate::PathData> {
        self.access(false, |object| object.path())
    }
    pub fn path_segments(&self) -> Result<Vec<crate::PathSegment>> {
        self.access(false, |object| object.path_segments())
    }
    pub fn text_object_data(&self) -> Result<crate::TextObjectData> {
        self.access(false, |object| object.text_object_data())
    }
    pub fn text(&self) -> Result<String> {
        self.access(false, |object| object.text())
    }
    pub fn font(&self) -> Result<crate::Font> {
        self.access(false, |object| object.font())
    }
    pub fn image_info(&self) -> Result<crate::ImageInfo> {
        self.access(false, |object| object.image_info())
    }
    pub fn image_bitmap(&self) -> Result<crate::Bitmap> {
        self.access(false, |object| object.image_bitmap())
    }
    pub fn render_image_object(&self) -> Result<crate::Bitmap> {
        self.access(false, |object| object.render_image_object())
    }
    pub fn pattern(&self, stroke: bool) -> Result<Option<crate::Bitmap>> {
        self.access(false, |object| object.pattern(stroke))
    }
    pub fn render_text_object(&self, scale: f32) -> Result<crate::Bitmap> {
        self.access(false, |object| object.render_text_object(scale))
    }
    pub fn render_text_object_in(&self, page: &Page, scale: f32) -> Result<crate::Bitmap> {
        self.access(false, |object| object.render_text_object_in(page, scale))
    }
    pub fn image_info_in(&self, page: &Page) -> Result<crate::ImageInfo> {
        self.access(false, |object| object.image_info_in(page))
    }
    pub fn image_data(&self, decoded: bool) -> Result<Vec<u8>> {
        self.access(false, |object| object.image_data(decoded))
    }
    pub fn image_icc_profile(&self, page: &Page) -> Result<Option<Vec<u8>>> {
        self.access(false, |object| object.image_icc_profile(page))
    }
    pub fn render_image_object_in(&self, page: &Page) -> Result<crate::Bitmap> {
        self.access(false, |object| object.render_image_object_in(page))
    }
    pub fn transform(&self, matrix: crate::Matrix) -> Result<()> {
        self.access(true, |object| object.transform(matrix))
    }
    pub fn set_matrix(&self, matrix: crate::Matrix) -> Result<()> {
        self.access(true, |object| object.set_matrix(matrix))
    }
    pub fn set_stroke_width(&self, width: f32) -> Result<()> {
        self.access(true, |object| object.set_stroke_width(width))
    }
    pub fn set_text(&self, text: &str) -> Result<()> {
        self.access(true, |object| object.set_text(text))
    }
    pub fn append_path_commands(&self, commands: &[crate::PathCommand]) -> Result<()> {
        self.access(true, |object| object.append_path_commands(commands))
    }
    pub fn set_path_commands(&self, commands: &[crate::PathCommand]) -> Result<()> {
        self.access(true, |object| object.set_path_commands(commands))
    }
    pub fn set_path_draw_mode(&self, fill: Option<crate::FillRule>, stroke: bool) -> Result<()> {
        self.access(true, |object| object.set_path_draw_mode(fill, stroke))
    }
    pub fn set_line_cap(&self, cap: crate::LineCap) -> Result<()> {
        self.access(true, |object| object.set_line_cap(cap))
    }
    pub fn set_line_join(&self, join: crate::LineJoin) -> Result<()> {
        self.access(true, |object| object.set_line_join(join))
    }
    pub fn set_dash(&self, dashes: &[f32], phase: f32) -> Result<()> {
        self.access(true, |object| object.set_dash(dashes, phase))
    }
    pub fn set_dash_phase(&self, phase: f32) -> Result<()> {
        self.access(true, |object| object.set_dash_phase(phase))
    }
    pub fn set_blend_mode(&self, mode: &str) -> Result<()> {
        self.access(true, |object| object.set_blend_mode(mode))
    }
    pub fn set_text_font_size(&self, font_size: f32) -> Result<()> {
        self.access(true, |object| object.set_text_font_size(font_size))
    }
    pub fn set_text_render_mode(&self, mode: crate::TextRenderMode) -> Result<()> {
        self.access(true, |object| object.set_text_render_mode(mode))
    }
    pub fn set_text_charcodes(&self, codes: &[u32]) -> Result<()> {
        self.access(true, |object| object.set_text_charcodes(codes))
    }
    pub fn set_text_positions(&self, positions: &[f32]) -> Result<()> {
        self.access(true, |object| object.set_text_positions(positions))
    }
    pub fn set_image_bitmap(&self, pixels: crate::Bitmap) -> Result<()> {
        self.access(true, |object| object.set_image_bitmap(pixels))
    }
    pub fn set_image_jpeg(
        &self,
        data: impl Into<std::sync::Arc<[u8]>>,
        storage: crate::JpegStorage,
    ) -> Result<()> {
        self.access(true, |object| object.set_image_jpeg(data, storage))
    }
    /// Remove the sole remaining mark. Other retained marks cannot currently
    /// be serialized by this appearance editor; use `clear_marks` for all marks.
    pub fn remove_mark(&self, mark: &crate::ContentMark) -> Result<()> {
        self.access(true, |object| object.remove_mark(mark))
    }
    /// Remove all content marks before further appearance-object editing.
    pub fn clear_marks(&self) -> Result<()> {
        self.access(true, |object| {
            for mark in object.marks() {
                object.remove_mark(&mark)?;
            }
            Ok(())
        })
    }
    pub fn set_fill_color(&self, color: Color) -> Result<()> {
        self.access(true, |object| {
            object.set_fill_color(color);
            Ok(())
        })
    }
    pub fn set_stroke_color(&self, color: Color) -> Result<()> {
        self.access(true, |object| {
            object.set_stroke_color(color);
            Ok(())
        })
    }
    pub fn set_active(&self, active: bool) -> Result<()> {
        self.access(true, |object| {
            object.set_active(active);
            Ok(())
        })
    }
}
impl Page {
    /// Count raw annotation-array entries, matching PDFium even for malformed entries.
    pub fn annotation_count(&self) -> Result<usize> {
        self.with_annotations(false, |page, _, _| Ok(sdk::annotation_count(page)))
    }
    pub fn annotation_at(&self, index: usize) -> Result<Option<AnnotId>> {
        self.with_annotations(false, |page, _, registry| {
            sdk::annotation_at(page, index)
                .map(|o| registry.register(self.id(), o))
                .transpose()
        })
    }
    /// Linked annotations outside this page's array have no array index.
    pub fn annotation_index(&self, id: AnnotId) -> Result<Option<usize>> {
        self.with_annotations(false, |page, _, registry| {
            Ok(sdk::annotation_index(
                page,
                &registry.resolve(page, self.id(), id)?,
            ))
        })
    }
    pub fn annotation_link(&self, id: AnnotId) -> Result<Option<crate::Link>> {
        let Some(index) = self.annotation_index(id)? else {
            return Ok(None);
        };
        Ok(self
            .links()?
            .into_iter()
            .find(|link| link.annotation_index == index))
    }
}

impl Page {
    /// Evaluate an annotation's optional content in the current viewing context.
    /// Ordinary annotation flags remain available in its details separately.
    pub fn annotation_layer_visible(&self, id: AnnotId) -> Result<bool> {
        self.with_annotations(false, |page, doc, registry| {
            let object = registry.resolve(page, self.id(), id)?;
            let oc = object
                .borrow()
                .as_dictionary()
                .and_then(|d| d.get_dict_for(b"OC"));
            let visible = pdfoxium_core::fpdfapi::page::cpdf_occontext::CpdfOcContext::new(
                &doc.borrow(),
                pdfoxium_core::fpdfapi::page::cpdf_occontext::UsageType::View,
            )
            .check_ocg_dict_visible(oc.as_ref());
            Ok(visible)
        })
    }
    /// Set or remove the optional-content group on an annotation.
    pub fn set_annotation_layer(
        &self,
        id: AnnotId,
        group: Option<crate::OptionalContentGroupId>,
    ) -> Result<()> {
        let number = group.map(|g| g.object(self.document())).transpose()?;
        self.with_annotations(true, |page, doc, registry| {
            let object = registry.resolve(page, self.id(), id)?;
            let mut object = object.borrow_mut();
            let d = object
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .ok_or(Error::Format)?;
            if let Some(n) = number {
                d.set_new_for(
                    b"OC",
                    pdfoxium_core::fpdfapi::parser::cpdf_reference::CpdfReference::new(
                        &doc.borrow().holder(),
                        n,
                    ),
                );
            } else {
                d.remove_for(b"OC");
            }
            Ok(())
        })
    }
}
