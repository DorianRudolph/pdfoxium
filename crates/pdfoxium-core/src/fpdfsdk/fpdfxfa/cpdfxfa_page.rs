// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cpdfxfa_page.{h,cpp}`. Foreground XFA uses the actual PDF page; full XFA
//! uses the current layout page. Cached handles never duplicate a page tree.
use super::cpdfxfa_context::CpdfXfaContext;
use crate::{
    fpdfapi::{
        page::cpdf_page::CpdfPage,
        parser::{cpdf_document::CpdfDocument, cpdf_object::ObjectRef},
    },
    fpdfdoc::cpdf_formfield::FormFieldType,
    fpdfsdk::{cpdfsdk_annot::CpdfSdkAnnot, cpdfsdk_pageview::CpdfSdkPageView},
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, FxRect},
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fxfa::{
            cxfa_ffwidget::{FfResult, FfWidgetIterator, FocusOption, HighlightOption},
            fxfa::XfaWidgetStatus as S,
            layout::cxfa_viewlayoutitem::CxfaViewLayoutItem,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
pub struct CpdfXfaPage {
    context: Weak<CpdfXfaContext>,
    pdf: RefCell<Option<Rc<CpdfPage>>>,
    live_pdf: RefCell<Weak<CpdfPage>>,
    index: Cell<i32>,
}
impl CpdfXfaPage {
    pub fn new(context: Weak<CpdfXfaContext>, index: i32) -> Rc<Self> {
        Rc::new(Self {
            context,
            pdf: RefCell::new(None),
            live_pdf: RefCell::new(Weak::new()),
            index: Cell::new(index),
        })
    }
    /// The public new-page path already created and parsed this PDF page.
    /// Adopt its exact owner rather than parsing a duplicate page object tree.
    pub fn new_with_pdf_page(
        context: Weak<CpdfXfaContext>,
        index: i32,
        page: CpdfPage,
    ) -> Rc<Self> {
        let owner = Self::new(context, index);
        owner.pdf.replace(Some(Rc::new(page)));
        owner
    }
    pub fn context(&self) -> Option<Rc<CpdfXfaContext>> {
        self.context.upgrade()
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.context()?.pdf_document()
    }
    pub fn page_index(&self) -> i32 {
        self.index.get()
    }
    pub fn set_xfa_page_view_index(&self, index: i32) {
        self.index.set(index);
    }
    pub fn pdf_page(&self) -> Option<Rc<CpdfPage>> {
        self.pdf
            .borrow()
            .clone()
            .or_else(|| self.live_pdf.borrow().upgrade())
    }
    /// Transfer the initial parsed owner to the public page handle. Retain a
    /// weak cache so SDK lookup does not prevent exclusive editing when there
    /// is no second public/progressive reader.
    pub fn take_pdf_page(&self) -> Option<Rc<CpdfPage>> {
        let page = self
            .pdf
            .borrow_mut()
            .take()
            .or_else(|| self.live_pdf.borrow().upgrade())?;
        *self.live_pdf.borrow_mut() = Rc::downgrade(&page);
        Some(page)
    }
    /// The C ABI mutation lease suspends this weak observer while the sole
    /// parsed page owner is borrowed exclusively, then restores it on drop.
    /// A weak observer alone must not prevent otherwise exclusive editing.
    pub fn set_live_pdf_page(&self, page: Option<&Rc<CpdfPage>>) {
        *self.live_pdf.borrow_mut() = page.map_or_else(Weak::new, Rc::downgrade);
    }
    pub fn get_xfa_page_view(&self) -> Option<CxfaViewLayoutItem> {
        self.context()?.doc_view()?.get_page_view(self.page_index())
    }
    pub fn load_page(&self) -> bool {
        let Some(context) = self.context() else {
            return false;
        };
        if context.contains_extension_full_form() {
            self.get_xfa_page_view().is_some()
        } else {
            self.load_pdf_page()
        }
    }
    pub fn load_pdf_page(&self) -> bool {
        let Ok(index) = usize::try_from(self.page_index()) else {
            return false;
        };
        let Some(doc) = self.document() else {
            return false;
        };
        let Some(dict) = doc.borrow_mut().page_dictionary(index) else {
            return false;
        };
        if self
            .pdf_page()
            .is_some_and(|page| Rc::ptr_eq(&page.dict(), &dict))
        {
            return true;
        }
        self.load_pdf_page_from_dict(dict)
    }
    pub fn load_pdf_page_from_dict(&self, dict: ObjectRef) -> bool {
        let Some(doc) = self.document() else {
            return false;
        };
        let data = doc.borrow().page_data_ref();
        let Some(mut page) = CpdfPage::new_with_page_data(dict, data) else {
            return false;
        };
        page.parse_path_content();
        self.pdf.replace(Some(Rc::new(page)));
        true
    }
    fn prefer_pdf(&self) -> bool {
        self.context()
            .is_some_and(|ctx| !ctx.contains_extension_full_form())
    }
    pub fn width(&self) -> f32 {
        if self.prefer_pdf() {
            if let Some(pdf) = self.pdf_page() {
                return pdf.width();
            }
        }
        self.get_xfa_page_view()
            .map_or(0., |page| page.get_page_size().width)
    }
    pub fn height(&self) -> f32 {
        if self.prefer_pdf() {
            if let Some(pdf) = self.pdf_page() {
                return pdf.height();
            }
        }
        self.get_xfa_page_view()
            .map_or(0., |page| page.get_page_size().height)
    }
    pub fn display_matrix_for_rect(&self, rect: FxRect, rotation: i32) -> CfxMatrix {
        if self.prefer_pdf() {
            if let Some(pdf) = self.pdf_page() {
                return pdf.display_matrix_for_rect(rect, rotation);
            }
        }
        self.get_xfa_page_view()
            .and_then(|item| item.page_view())
            .and_then(|page| {
                page.as_ff_page_view()?
                    .get_display_matrix_for_rect(rect, rotation)
            })
            .unwrap_or_default()
    }
    pub fn device_to_page(
        &self,
        rect: FxRect,
        rotation: i32,
        point: CfxPointF,
    ) -> Option<CfxPointF> {
        if self.pdf_page().is_none() && self.get_xfa_page_view().is_none() {
            return None;
        }
        Some(
            self.display_matrix_for_rect(rect, rotation)
                .get_inverse()
                .transform(point),
        )
    }
    pub fn page_to_device(
        &self,
        rect: FxRect,
        rotation: i32,
        point: CfxPointF,
    ) -> Option<CfxPointF> {
        if self.pdf_page().is_none() && self.get_xfa_page_view().is_none() {
            return None;
        }
        Some(
            self.display_matrix_for_rect(rect, rotation)
                .transform(point),
        )
    }
    fn edge_annot(&self, view: &CpdfSdkPageView, last: bool) -> Option<Rc<CpdfSdkAnnot>> {
        let item = self.get_xfa_page_view()?;
        let owner = item.page_view()?;
        let page = owner.as_ff_page_view()?;
        let mut iterator =
            page.create_traverse_widget_iterator(S::kVisible | S::kViewable | S::kFocused);
        if !view.is_alive() {
            return None;
        }
        let widget = if last {
            iterator.move_to_last()
        } else {
            iterator.move_to_first()
        }?;
        if !view.is_alive() {
            return None;
        }
        view.get_annot_for_ff_widget(widget.as_ff_widget()?)
    }
    pub fn get_first_xfa_annot(&self, view: &CpdfSdkPageView) -> Option<Rc<CpdfSdkAnnot>> {
        self.edge_annot(view, false)
    }
    pub fn get_last_xfa_annot(&self, view: &CpdfSdkPageView) -> Option<Rc<CpdfSdkAnnot>> {
        self.edge_annot(view, true)
    }
    fn adjacent_annot(&self, annot: &Rc<CpdfSdkAnnot>, previous: bool) -> Option<Rc<CpdfSdkAnnot>> {
        let xfa = annot.xfa_widget()?;
        let item = self.get_xfa_page_view()?;
        let owner = item.page_view()?;
        let page = owner.as_ff_page_view()?;
        let mut iterator =
            page.create_traverse_widget_iterator(S::kVisible | S::kViewable | S::kFocused);
        if !annot.is_alive() {
            return None;
        }
        if !iterator.set_current_widget(xfa.ff_widget().as_ff_widget()?) {
            return None;
        }
        let next = if previous {
            iterator.move_to_previous()
        } else {
            iterator.move_to_next()
        }?;
        if !annot.is_alive() {
            return None;
        }
        annot
            .page_view()?
            .get_annot_for_ff_widget(next.as_ff_widget()?)
    }
    pub fn get_next_xfa_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        self.adjacent_annot(annot, false)
    }
    pub fn get_prev_xfa_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        self.adjacent_annot(annot, true)
    }
    pub fn has_form_field_at_point(&self, point: CfxPointF) -> i32 {
        let Some(owner) = self.get_xfa_page_view().and_then(|item| item.page_view()) else {
            return -1;
        };
        let Some(page) = owner
            .as_ff_page_view()
            .filter(|page| page.doc_view().is_some())
        else {
            return -1;
        };
        let mut iterator = page.create_widget_iterator(S::kViewable);
        while let Some(owner) = iterator.move_to_next() {
            let Some(widget) = owner.as_ff_widget() else {
                continue;
            };
            let kind = widget.get_form_field_type();
            if kind == FormFieldType::Xfa {
                continue;
            }
            let mut rect = widget.widget().get_widget_rect().to_float_rect();
            rect.inflate_xy(1., 1.);
            if rect.contains_point(point) {
                return kind as i32;
            }
        }
        -1
    }
    pub fn draw_focus_annot(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        annot: Option<&Rc<CpdfSdkAnnot>>,
        matrix: CfxMatrix,
        clip: FxRect,
    ) -> FfResult<bool> {
        let Some(owner) = self.get_xfa_page_view().and_then(|item| item.page_view()) else {
            return Ok(false);
        };
        let Some(page) = owner.as_ff_page_view() else {
            return Ok(false);
        };
        let rect_clip = CfxRectF::from_fx_rect(clip);
        graphics.save_graph_state();
        graphics.set_clip_rect(rect_clip);
        let result = (|| {
            let mut iterator = page.create_widget_iterator(S::kVisible | S::kViewable);
            while let Some(owner) = iterator.move_to_next() {
                let Some(widget) = owner.as_ff_widget() else {
                    continue;
                };
                let mut bbox = widget.get_bbox(FocusOption::DoNotDrawFocus);
                bbox.width += 1.;
                bbox.height += 1.;
                if bbox.intersects(rect_clip) {
                    widget.render_widget(graphics, matrix, HighlightOption::Highlight)?;
                }
            }
            if let Some(xfa) = annot
                .filter(|annot| annot.is_alive())
                .and_then(|annot| annot.xfa_widget())
            {
                if xfa.is_alive() {
                    xfa.ff_widget().as_ff_widget().unwrap().render_widget(
                        graphics,
                        matrix,
                        HighlightOption::NoHighlight,
                    )?;
                }
            }
            Ok(true)
        })();
        graphics.restore_graph_state();
        result
    }
}
