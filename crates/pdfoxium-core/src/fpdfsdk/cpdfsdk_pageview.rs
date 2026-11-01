// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordinary page-view ownership, event routing and hover/focus behavior from
//! `fpdfsdk/cpdfsdk_pageview.{h,cpp}`. XFA page/widget integration remains open.
use super::cpdfsdk_annot::CpdfSdkAnnot;
use super::cpdfsdk_annotiteration::CpdfSdkAnnotIteration;
use super::cpdfsdk_annotiterator::CpdfSdkAnnotIterator;
use super::cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment;
use super::cpdfsdk_widget::CpdfSdkWidget;
use super::fpdf_fwlevent::{
    FWL_EVENTFLAG_AltKey, FWL_EVENTFLAG_ControlKey, FWL_EVENTFLAG_MetaKey, FWL_EVENTFLAG_ShiftKey,
    FWL_VKEY_Tab,
};
#[cfg(feature = "xfa")]
use super::fpdfxfa::{cpdfxfa_page::CpdfXfaPage, cpdfxfa_widget::CpdfXfaWidget};
use super::pwl::pwl_draw::PwlDraw;
use crate::fpdfapi::page::cpdf_docpagedata::CpdfDocPageData;
use crate::fpdfapi::page::cpdf_page::CpdfPage;
use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfdoc::cpdf_annot::Subtype;
use crate::fpdfdoc::cpdf_annotlist::CpdfAnnotList;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF, FxRect};
#[cfg(feature = "xfa")]
use crate::xfa::fxfa::{
    cxfa_ffwidget::{FfWidget, FfWidgetIterator},
    fxfa::XfaWidgetStatus,
    layout::cxfa_layoutitem::LayoutItemWidget,
};
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

thread_local! {static NEXT_PAGE_ID:Cell<u64>=const {Cell::new(1)};}
/// Embedder-owned lifetime bridge for an actual loaded page. It retains the
/// existing dictionary and resource cache, never the parsed page-object graph.
/// Closing the C ABI page invalidates this bridge before dropping its owner.
pub struct SdkPage {
    id: u64,
    document: Weak<RefCell<CpdfDocument>>,
    backing: SdkPageBacking,
    alive: Cell<bool>,
    environments: RefCell<Vec<Weak<CpdfSdkFormFillEnvironment>>>,
}
enum SdkPageBacking {
    Pdf {
        dict: ObjectRef,
        data: Rc<CpdfDocPageData>,
    },
    #[cfg(feature = "xfa")]
    Xfa(Rc<CpdfXfaPage>),
}
impl SdkPage {
    pub fn new(document: &Rc<RefCell<CpdfDocument>>, page: &CpdfPage) -> Rc<Self> {
        let id = NEXT_PAGE_ID.with(|next| {
            let id = next.get();
            next.set(id.checked_add(1).expect("page identity exhausted"));
            id
        });
        Rc::new(Self {
            id,
            document: Rc::downgrade(document),
            backing: SdkPageBacking::Pdf {
                dict: page.dict(),
                data: page.page_data_ref(),
            },
            alive: Cell::new(true),
            environments: RefCell::new(Vec::new()),
        })
    }
    #[cfg(feature = "xfa")]
    pub fn new_xfa(document: &Rc<RefCell<CpdfDocument>>, page: Rc<CpdfXfaPage>) -> Rc<Self> {
        let id = NEXT_PAGE_ID.with(|next| {
            let id = next.get();
            next.set(id.checked_add(1).expect("page identity exhausted"));
            id
        });
        Rc::new(Self {
            id,
            document: Rc::downgrade(document),
            backing: SdkPageBacking::Xfa(page),
            alive: Cell::new(true),
            environments: RefCell::new(Vec::new()),
        })
    }
    #[cfg(feature = "xfa")]
    pub fn xfa_page(&self) -> Option<Rc<CpdfXfaPage>> {
        match &self.backing {
            #[cfg(feature = "xfa")]
            SdkPageBacking::Xfa(page) => Some(page.clone()),
            _ => None,
        }
    }
    pub fn pdf_dict(&self) -> Option<ObjectRef> {
        match &self.backing {
            SdkPageBacking::Pdf { dict, .. } => Some(dict.clone()),
            #[cfg(feature = "xfa")]
            SdkPageBacking::Xfa(page) => page.pdf_page().map(|page| page.dict()),
        }
    }
    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn is_alive(&self) -> bool {
        self.alive.get() && self.document.strong_count() != 0
    }
    pub fn invalidate(&self) {
        if !self.alive.replace(false) {
            return;
        }
        // Closing a page during a callback must tear down retained UI state too.
        // Release the observer-list borrow before environment callbacks.
        let observers = std::mem::take(&mut *self.environments.borrow_mut());
        for environment in observers {
            if let Some(environment) = environment.upgrade() {
                environment.on_page_closed(self);
            }
        }
    }
    fn register_environment(&self, environment: &Rc<CpdfSdkFormFillEnvironment>) {
        let weak = Rc::downgrade(environment);
        let mut environments = self.environments.borrow_mut();
        environments.retain(|entry| entry.strong_count() != 0);
        if !environments.iter().any(|entry| entry.ptr_eq(&weak)) {
            environments.push(weak);
        }
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.document.upgrade()
    }
    pub fn dict(&self) -> ObjectRef {
        self.pdf_dict()
            .expect("full XFA pages have no PDF page dictionary")
    }
    pub fn page_data(&self) -> Rc<CpdfDocPageData> {
        match &self.backing {
            SdkPageBacking::Pdf { data, .. } => data.clone(),
            #[cfg(feature = "xfa")]
            SdkPageBacking::Xfa(page) => page
                .document()
                .expect("live document")
                .borrow()
                .page_data_ref(),
        }
    }
    /// Reconstruct only page metadata/resources, without parsing contents or
    /// copying any existing page object, path, image or text buffer.
    pub fn geometry_page(&self) -> Option<CpdfPage> {
        self.is_alive()
            .then(|| CpdfPage::new_with_page_data(self.pdf_dict()?, self.page_data()))
            .flatten()
    }
    pub fn index(&self) -> i32 {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.xfa_page() {
            return page.page_index();
        }
        let Some(document) = self.document() else {
            return -1;
        };
        let Some(dict) = self.pdf_dict() else {
            return -1;
        };
        let number = dict.borrow().obj_num();
        let index = document
            .borrow_mut()
            .page_index(number)
            .and_then(|index| i32::try_from(index).ok())
            .unwrap_or(-1);
        index
    }
    pub fn width(&self) -> f32 {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.xfa_page() {
            return page.width();
        }
        self.geometry_page().map_or(0.0, |page| page.width())
    }
    pub fn height(&self) -> f32 {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.xfa_page() {
            return page.height();
        }
        self.geometry_page().map_or(0.0, |page| page.height())
    }
    pub fn bbox(&self) -> CfxFloatRect {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.xfa_page().filter(|page| {
            page.context()
                .is_some_and(|ctx| ctx.contains_extension_full_form())
        }) {
            return CfxFloatRect::new(0., 0., page.width(), page.height());
        }
        self.geometry_page()
            .map_or_else(CfxFloatRect::default, |page| page.bbox())
    }
    pub fn page_matrix(&self) -> CfxMatrix {
        self.geometry_page()
            .map_or_else(CfxMatrix::default, |page| page.page_matrix())
    }
    pub fn display_matrix_for_rect(&self, rect: FxRect, rotate: i32) -> CfxMatrix {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.xfa_page() {
            return page.display_matrix_for_rect(rect, rotate);
        }
        self.geometry_page()
            .map_or_else(CfxMatrix::default, |page| {
                page.display_matrix_for_rect(rect, rotate)
            })
    }
}
impl Drop for SdkPage {
    fn drop(&mut self) {
        self.invalidate();
    }
}
pub struct CpdfSdkPageView {
    environment: Weak<CpdfSdkFormFillEnvironment>,
    page: Weak<SdkPage>,
    annots: RefCell<Vec<Rc<CpdfSdkAnnot>>>,
    capture_widget: RefCell<Option<Weak<CpdfSdkAnnot>>>,
    matrix: Cell<CfxMatrix>,
    on_widget: Cell<bool>,
    valid: Cell<bool>,
    locked: Cell<bool>,
    being_destroyed: Cell<bool>,
    alive: Cell<bool>,
}
impl CpdfSdkPageView {
    pub fn new(environment: &Rc<CpdfSdkFormFillEnvironment>, page: &Rc<SdkPage>) -> Rc<Self> {
        page.register_environment(environment);
        Rc::new(Self {
            environment: Rc::downgrade(environment),
            page: Rc::downgrade(page),
            annots: RefCell::new(Vec::new()),
            capture_widget: RefCell::new(None),
            matrix: Cell::new(CfxMatrix::default()),
            on_widget: Cell::new(false),
            valid: Cell::new(false),
            locked: Cell::new(false),
            being_destroyed: Cell::new(false),
            alive: Cell::new(true),
        })
    }
    pub fn id(&self) -> u64 {
        self.page.upgrade().map_or(0, |page| page.id())
    }
    pub fn environment(&self) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.environment.upgrade()
    }
    pub fn page(&self) -> Option<Rc<SdkPage>> {
        self.page.upgrade().filter(|page| page.is_alive())
    }
    pub fn document(&self) -> Option<Rc<RefCell<CpdfDocument>>> {
        self.page()?.document()
    }
    pub fn page_index(&self) -> i32 {
        self.page().map_or(-1, |page| page.index())
    }
    pub fn bbox(&self) -> CfxFloatRect {
        self.page()
            .map_or_else(CfxFloatRect::default, |page| page.bbox())
    }
    pub fn current_matrix(&self) -> CfxMatrix {
        self.matrix.get()
    }
    pub fn is_alive(&self) -> bool {
        self.alive.get() && self.page().is_some()
    }
    pub fn is_valid(&self) -> bool {
        self.is_alive() && self.valid.get()
    }
    pub fn set_valid(&self, valid: bool) {
        self.valid.set(valid);
    }
    pub fn is_locked(&self) -> bool {
        self.locked.get()
    }
    pub fn is_being_destroyed(&self) -> bool {
        self.being_destroyed.get()
    }
    pub fn set_being_destroyed(&self) {
        self.being_destroyed.set(true);
    }
    pub fn annot_list(&self) -> Vec<Rc<CpdfSdkAnnot>> {
        self.annots
            .borrow()
            .iter()
            .filter(|a| a.is_alive())
            .cloned()
            .collect()
    }
    pub fn annot_by_dict(&self, dict: &ObjectRef) -> Option<Rc<CpdfSdkAnnot>> {
        self.annot_list().into_iter().find(|a| {
            a.pdf_dict()
                .is_some_and(|candidate| Rc::ptr_eq(&candidate, dict))
        })
    }
    pub fn widget_by_dict(&self, dict: &ObjectRef) -> Option<Rc<CpdfSdkWidget>> {
        self.annot_by_dict(dict)?.widget()
    }
    pub fn is_valid_sdk_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> bool {
        annot.is_alive() && self.annots.borrow().iter().any(|a| Rc::ptr_eq(a, annot))
    }
    pub fn is_valid_annot(&self, dict: &ObjectRef) -> bool {
        self.annot_by_dict(dict).is_some()
    }
    pub fn focus_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.environment()?
            .get_focus_annot()
            .filter(|annot| self.is_valid_sdk_annot(annot))
    }
    pub fn get_focus_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        self.focus_annot()
    }
    fn focus_subtypes(&self) -> Vec<Subtype> {
        self.environment()
            .map_or_else(Vec::new, |env| env.focusable_annot_subtypes())
    }
    pub fn next_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.full_xfa_page() {
            return page.get_next_xfa_annot(annot);
        }
        CpdfSdkAnnotIterator::new(self, &self.focus_subtypes()).next_annot(annot)
    }
    pub fn prev_annot(&self, annot: &Rc<CpdfSdkAnnot>) -> Option<Rc<CpdfSdkAnnot>> {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.full_xfa_page() {
            return page.get_prev_xfa_annot(annot);
        }
        CpdfSdkAnnotIterator::new(self, &self.focus_subtypes()).prev_annot(annot)
    }
    pub fn first_focusable_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.full_xfa_page() {
            return page.get_first_xfa_annot(self);
        }
        CpdfSdkAnnotIterator::new(self, &self.focus_subtypes()).first_annot()
    }
    pub fn last_focusable_annot(&self) -> Option<Rc<CpdfSdkAnnot>> {
        #[cfg(feature = "xfa")]
        if let Some(page) = self.full_xfa_page() {
            return page.get_last_xfa_annot(self);
        }
        CpdfSdkAnnotIterator::new(self, &self.focus_subtypes()).last_annot()
    }
    #[cfg(feature = "xfa")]
    fn full_xfa_page(&self) -> Option<Rc<CpdfXfaPage>> {
        self.page()?.xfa_page().filter(|page| {
            page.context()
                .is_some_and(|ctx| ctx.contains_extension_full_form())
        })
    }
    #[cfg(feature = "xfa")]
    pub fn get_annot_for_ff_widget(&self, widget: &dyn FfWidget) -> Option<Rc<CpdfSdkAnnot>> {
        self.annot_list().into_iter().find(|annot| {
            annot.xfa_widget().is_some_and(|xfa| {
                std::ptr::eq(
                    xfa.ff_widget().as_ff_widget().unwrap().widget(),
                    widget.widget(),
                )
            })
        })
    }
    #[cfg(feature = "xfa")]
    pub fn add_annot_for_ff_widget(
        self: &Rc<Self>,
        widget: &Rc<dyn LayoutItemWidget>,
    ) -> Option<Rc<CpdfSdkAnnot>> {
        if !self.is_alive() {
            return None;
        }
        let ff = widget.as_ff_widget()?;
        if let Some(existing) = self.get_annot_for_ff_widget(ff) {
            return Some(existing);
        }
        let xfa = CpdfXfaWidget::new(widget.clone(), self);
        let annot = CpdfSdkAnnot::for_xfa(xfa, self);
        self.annots.borrow_mut().push(annot.clone());
        Some(annot)
    }
    #[cfg(feature = "xfa")]
    pub fn delete_annot_for_ff_widget(&self, widget: &dyn FfWidget) {
        let Some(annot) = self.get_annot_for_ff_widget(widget) else {
            return;
        };
        if let Some(env) = self.environment() {
            if env
                .get_focus_annot()
                .as_ref()
                .is_some_and(|focus| Rc::ptr_eq(focus, &annot))
            {
                env.kill_focus_annot(0);
            }
        }
        self.annots
            .borrow_mut()
            .retain(|entry| !Rc::ptr_eq(entry, &annot));
        annot.invalidate();
        let captured = self
            .capture_widget
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade)
            .is_some_and(|entry| Rc::ptr_eq(&entry, &annot));
        if captured {
            self.capture_widget.borrow_mut().take();
            self.on_widget.set(false);
        }
    }
    pub fn load_fx_annots(self: &Rc<Self>) {
        if !self.is_alive() || self.locked.replace(true) {
            return;
        }
        struct Restore<'a>(&'a Cell<bool>);
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                self.0.set(false);
            }
        }
        let _restore = Restore(&self.locked);
        let (Some(page), Some(env)) = (self.page(), self.environment()) else {
            return;
        };
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.full_xfa_page() {
            let Some(owner) = xfa.get_xfa_page_view().and_then(|item| item.page_view()) else {
                return;
            };
            let Some(page) = owner.as_ff_page_view() else {
                return;
            };
            let mut iterator =
                page.create_widget_iterator(XfaWidgetStatus::kVisible | XfaWidgetStatus::kViewable);
            while let Some(widget) = iterator.move_to_next() {
                if !self.is_alive() {
                    break;
                }
                if let Some(annot) = self.add_annot_for_ff_widget(&widget) {
                    annot.on_load();
                }
            }
            return;
        }
        let Some(geometry) = page.geometry_page() else {
            return;
        };
        let form = env.get_interactive_form();
        let core_form = form.core_form();
        // The environment registers this view before callbacks can reenter loading.
        core_form.fix_page_fields(&geometry);
        for annot in CpdfAnnotList::new_for_sdk(&geometry).into_annots() {
            super::cpdfsdk_unsupported::notify_unsupported_annot(annot.subtype(), annot.dict());
            if !self.is_alive() {
                break;
            }
            let wrapper = if annot.subtype() == Subtype::Widget {
                let Some(control) = core_form.get_control_by_dict(annot.dict()) else {
                    continue;
                };
                let Some(document) = self.document() else {
                    break;
                };
                let widget = Rc::new(CpdfSdkWidget::new(&document, control.clone()));
                let wrapper = CpdfSdkAnnot::for_widget(annot, widget.clone(), self);
                form.add_map(&control, &widget);
                if core_form.need_construct_ap() {
                    widget.reset_appearance(None, false);
                }
                wrapper
            } else if annot.subtype() == Subtype::XfaWidget {
                continue;
            } else {
                CpdfSdkAnnot::basic(annot, self)
            };
            self.annots.borrow_mut().push(wrapper.clone());
            wrapper.on_load();
        }
    }
    pub fn destroy(&self) {
        if !self.alive.replace(false) {
            return;
        }
        self.being_destroyed.set(true);
        self.valid.set(false);
        self.capture_widget.borrow_mut().take();
        self.on_widget.set(false);
        let annots = std::mem::take(&mut *self.annots.borrow_mut());
        let env = self.environment();
        for annot in annots {
            annot.invalidate();
            if let (Some(env), Some(widget)) = (&env, annot.widget()) {
                env.get_interactive_form_filler().on_delete(&widget);
                env.get_interactive_form().remove_map(&widget.control());
            }
        }
    }
    pub fn clear_page(&self) {
        if !self.being_destroyed.get() {
            if let (Some(env), Some(page)) = (self.environment(), self.page()) {
                env.remove_page_view(&page);
            }
        }
    }
    pub fn update_rects(&self, rects: &[CfxFloatRect]) {
        let (Some(env), Some(page)) = (self.environment(), self.page()) else {
            return;
        };
        for rect in rects {
            if !self.is_alive() {
                break;
            }
            env.invalidate_page(&page, rect.get_outer_rect());
        }
    }
    pub fn update_view(&self, annot: &Rc<CpdfSdkAnnot>) {
        if self.is_valid_sdk_annot(annot) {
            self.update_rects(&[annot.rect()]);
        }
    }
    pub fn page_view_on_draw(
        &self,
        device: &mut dyn PwlDraw,
        matrix: CfxMatrix,
        draw_annots: bool,
        clip: FxRect,
    ) {
        if !self.is_alive() {
            return;
        }
        self.matrix.set(matrix);
        #[cfg(feature = "xfa")]
        if let Some(page) = self.full_xfa_page() {
            device.draw_xfa_page(&page, self.focus_annot().as_ref(), matrix, clip);
            return;
        }
        for observed in CpdfSdkAnnotIteration::for_drawing(self).iter() {
            if !self.is_alive() {
                break;
            }
            if let Some(annot) = observed.upgrade().filter(|a| a.is_alive()) {
                // A clipped form patch must not parse/draw unrelated widget
                // appearances. Use live bounds, including choice popups and
                // focus borders, rather than the saved annotation rectangle.
                // NoZoom/NoRotate appearances have a different device mapping.
                if let Some(widget) = annot.widget().filter(|w| w.annot().flags() & 24 == 0) {
                    let mut bounds = matrix.transform_float_rect(widget.view_bbox());
                    bounds.inflate_xy(1., 1.); // Conservative device AA coverage.
                    let mut intersection = bounds.get_outer_rect();
                    intersection.intersect(clip);
                    if intersection.is_empty() {
                        continue;
                    }
                }
                annot.on_draw(device, matrix, draw_annots);
            }
        }
    }
    fn fx_annot_at_point(&self, point: CfxPointF) -> Option<Rc<CpdfSdkAnnot>> {
        for observed in CpdfSdkAnnotIteration::new(self).iter() {
            let Some(annot) = observed.upgrade().filter(|a| a.is_alive()) else {
                continue;
            };
            if annot.optional_content_visible()
                && annot.subtype() != Subtype::Popup
                && annot.view_bbox().contains_point(point)
            {
                return Some(annot);
            }
        }
        None
    }
    fn fx_widget_at_point(&self, point: CfxPointF) -> Option<Rc<CpdfSdkAnnot>> {
        for observed in CpdfSdkAnnotIteration::new(self).iter() {
            let Some(annot) = observed.upgrade().filter(|a| a.is_alive()) else {
                continue;
            };
            if matches!(annot.subtype(), Subtype::Widget | Subtype::XfaWidget)
                && annot.do_hit_test(point)
            {
                return Some(annot);
            }
        }
        None
    }
    pub fn on_focus(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let Some(env) = self.environment() else {
            return false;
        };
        let Some(annot) = self.fx_widget_at_point(point) else {
            env.kill_focus_annot(flags);
            return false;
        };
        env.set_focus_annot(&annot);
        true
    }
    pub fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let Some(env) = self.environment() else {
            return false;
        };
        let Some(annot) = self.fx_widget_at_point(point) else {
            env.kill_focus_annot(flags);
            return false;
        };
        if !annot.on_l_button_down(flags, point) || !annot.is_alive() || !self.is_alive() {
            return false;
        }
        env.set_focus_annot(&annot);
        true
    }
    pub fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let hit = self.fx_widget_at_point(point);
        if let Some(focus) = self.focus_annot() {
            if hit.as_ref().is_none_or(|hit| !Rc::ptr_eq(hit, &focus))
                && focus.on_l_button_up(flags, point)
            {
                return true;
            }
        }
        hit.filter(|a| a.is_alive())
            .is_some_and(|annot| annot.on_l_button_up(flags, point))
    }
    pub fn on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let Some(env) = self.environment() else {
            return false;
        };
        let Some(annot) = self.fx_widget_at_point(point) else {
            env.kill_focus_annot(flags);
            return false;
        };
        if !annot.on_l_button_dbl_clk(flags, point) || !annot.is_alive() || !self.is_alive() {
            return false;
        }
        env.set_focus_annot(&annot);
        true
    }
    pub fn on_r_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.right_button(flags, point, false)
    }
    pub fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.right_button(flags, point, true)
    }
    fn right_button(&self, flags: u32, point: CfxPointF, up: bool) -> bool {
        let Some(annot) = self.fx_widget_at_point(point) else {
            return false;
        };
        let ok = if up {
            annot.on_r_button_up(flags, point)
        } else {
            annot.on_r_button_down(flags, point)
        };
        if !annot.is_alive() || !self.is_alive() {
            return false;
        }
        if ok {
            if let Some(env) = self.environment() {
                env.set_focus_annot(&annot);
            }
        }
        true
    }
    pub fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let annot = self.fx_annot_at_point(point);
        let captured = self
            .capture_widget
            .borrow()
            .as_ref()
            .and_then(Weak::upgrade);
        let same = match (&annot, &captured) {
            (Some(a), Some(b)) => Rc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if self.on_widget.get() && !same {
            self.exit_widget(true, flags);
        }
        if !self.is_alive() {
            return false;
        }
        let Some(annot) = annot.filter(|a| a.is_alive()) else {
            return false;
        };
        if !self.on_widget.get() {
            self.enter_widget(&annot, flags);
            if !self.is_alive() {
                return false;
            }
            if !annot.is_alive() {
                self.exit_widget(false, flags);
                return true;
            }
        }
        annot.on_mouse_move(flags, point);
        true
    }
    fn enter_widget(&self, annot: &Rc<CpdfSdkAnnot>, flags: u32) {
        self.on_widget.set(true);
        *self.capture_widget.borrow_mut() = Some(Rc::downgrade(annot));
        annot.on_mouse_enter(flags);
    }
    fn exit_widget(&self, callback: bool, flags: u32) {
        self.on_widget.set(false);
        let captured = self
            .capture_widget
            .borrow_mut()
            .take()
            .and_then(|a| a.upgrade());
        if callback {
            if let Some(annot) = captured.filter(|a| a.is_alive()) {
                annot.on_mouse_exit(flags);
            }
        }
    }
    pub fn on_mouse_wheel(&self, flags: u32, point: CfxPointF, delta: CfxPointF) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.fx_widget_at_point(point)
            .is_some_and(|annot| annot.on_mouse_wheel(flags, point, delta))
    }
    pub fn on_char(&self, word: u32, flags: u32) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.focus_annot()
            .is_some_and(|annot| annot.on_char(word, flags))
    }
    pub fn on_key_down(&self, key: i32, flags: u32) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        let focused = self.focus_annot();
        if key == FWL_VKEY_Tab {
            if flags & (FWL_EVENTFLAG_ControlKey | FWL_EVENTFLAG_AltKey | FWL_EVENTFLAG_MetaKey)
                != 0
            {
                return false;
            }
            let Some(env) = self.environment() else {
                return false;
            };
            let Some(annot) = focused.as_ref() else {
                let end = if flags & FWL_EVENTFLAG_ShiftKey != 0 {
                    self.last_focusable_annot()
                } else {
                    self.first_focusable_annot()
                };
                return end.is_some_and(|a| env.set_focus_annot(&a));
            };
            let next = if flags & FWL_EVENTFLAG_ShiftKey != 0 {
                self.prev_annot(annot)
            } else {
                self.next_annot(annot)
            };
            let Some(next) = next else { return false };
            if !Rc::ptr_eq(&next, annot) {
                env.set_focus_annot(&next);
                return true;
            }
        }
        focused
            .filter(|a| a.is_alive())
            .is_some_and(|annot| annot.on_key_down(key, flags))
    }
    pub fn focused_form_text(&self) -> Vec<u32> {
        self.focus_annot().map_or_else(Vec::new, |a| a.text())
    }
    pub fn selected_text(&self) -> Vec<u32> {
        self.focus_annot()
            .map_or_else(Vec::new, |a| a.selected_text())
    }
    pub fn replace_selection(&self, text: &[u32]) {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        if let Some(a) = self.focus_annot() {
            a.replace_selection(text);
        }
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        if let Some(a) = self.focus_annot() {
            a.replace_and_keep_selection(text);
        }
    }
    pub fn select_all_text(&self) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.focus_annot().is_some_and(|a| a.select_all_text())
    }
    pub fn can_undo(&self) -> bool {
        self.focus_annot().is_some_and(|a| a.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        self.focus_annot().is_some_and(|a| a.can_redo())
    }
    pub fn undo(&self) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.focus_annot().is_some_and(|a| a.undo())
    }
    pub fn redo(&self) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.focus_annot().is_some_and(|a| a.redo())
    }
    pub fn set_index_selected(&self, index: i32, selected: bool) -> bool {
        let environment = self.environment();
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        self.focus_annot()
            .is_some_and(|a| a.set_index_selected(index, selected))
    }
    pub fn is_index_selected(&self, index: i32) -> bool {
        self.focus_annot()
            .is_some_and(|a| a.is_index_selected(index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fpdfapi::parser::cpdf_array::CpdfArray;
    use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
    use crate::fpdfapi::parser::cpdf_object::retain_object;
    use crate::fpdfapi::parser::cpdf_string::CpdfString;
    use crate::fpdfsdk::cpdfsdk_formfillenvironment::FormFillHost;

    fn fixture(
        host: Option<&Rc<dyn FormFillHost>>,
        tabs: &[u8],
        rects: &[CfxFloatRect],
    ) -> (
        Rc<RefCell<CpdfDocument>>,
        Rc<SdkPage>,
        Rc<CpdfSdkFormFillEnvironment>,
        Rc<CpdfSdkPageView>,
    ) {
        let doc = Rc::new(RefCell::new(CpdfDocument::new()));
        doc.borrow_mut().create_new_doc();
        let dict = doc.borrow_mut().create_new_page(0).unwrap();
        let mut array = CpdfArray::new();
        for rect in rects {
            let mut annot = CpdfDictionary::new();
            annot.set_new_name_for(b"Subtype", b"Text");
            annot.set_rect_for(b"Rect", *rect);
            annot.set_new_for(b"Contents", CpdfString::from_bytes(None, b"Popup text"));
            array.append(retain_object(annot));
        }
        {
            let mut borrowed = dict.borrow_mut();
            let dict = borrowed
                .as_any_mut()
                .downcast_mut::<CpdfDictionary>()
                .unwrap();
            dict.set_rect_for(b"MediaBox", CfxFloatRect::new(-100.0, -100.0, 500.0, 500.0));
            dict.set_new_name_for(b"Tabs", tabs);
            dict.set_new_for(b"Annots", array);
        }
        let page = CpdfPage::from_document(&mut doc.borrow_mut(), 0).unwrap();
        let page = SdkPage::new(&doc, &page);
        let env = CpdfSdkFormFillEnvironment::new(&doc, host);
        let view = env.get_or_create_page_view(&page).unwrap();
        view.set_valid(true);
        env.set_focusable_annot_subtypes(&[Subtype::Text]);
        (doc, page, env, view)
    }
    fn traversal(view: &CpdfSdkPageView) -> Vec<CfxFloatRect> {
        let iterator = CpdfSdkAnnotIterator::new(view, &[Subtype::Text]);
        let mut next = iterator.first_annot();
        let mut out = Vec::new();
        while let Some(annot) = next {
            out.push(annot.rect());
            next = iterator.next_annot(&annot);
            assert!(out.len() <= 4);
        }
        out
    }
    #[test]
    fn structure_row_and_column_order_accept_negative_coordinates() {
        let a = CfxFloatRect::new(-90.0, -30.0, -70.0, -10.0);
        let b = CfxFloatRect::new(-40.0, -31.0, -20.0, -11.0);
        let c = CfxFloatRect::new(-89.0, -80.0, -69.0, -60.0);
        let d = CfxFloatRect::new(-39.0, -81.0, -19.0, -61.0);
        let input = [d, c, b, a];
        for (tabs, expected) in [
            (b"S", vec![d, c, b, a]),
            (b"R", vec![a, b, c, d]),
            (b"C", vec![a, c, b, d]),
        ] {
            let (_doc, _page, _env, view) = fixture(None, tabs, &input);
            assert_eq!(traversal(&view), expected);
            let iterator = CpdfSdkAnnotIterator::new(&view, &[Subtype::Text]);
            assert!(iterator
                .prev_annot(&iterator.first_annot().unwrap())
                .is_none());
            assert!(iterator
                .next_annot(&iterator.last_annot().unwrap())
                .is_none());
        }
    }
    #[test]
    fn focused_annotation_is_first_for_input_and_last_for_drawing() {
        let (_doc, _page, env, view) = fixture(
            None,
            b"S",
            &[
                CfxFloatRect::new(0.0, 0.0, 10.0, 10.0),
                CfxFloatRect::new(20.0, 0.0, 30.0, 10.0),
            ],
        );
        let annots = view.annot_list();
        let focused = annots[1].clone();
        assert!(env.set_focus_annot(&focused));
        assert!(Rc::ptr_eq(
            &CpdfSdkAnnotIteration::new(&view)
                .iter()
                .next()
                .unwrap()
                .upgrade()
                .unwrap(),
            &focused
        ));
        assert!(Rc::ptr_eq(
            &CpdfSdkAnnotIteration::for_drawing(&view)
                .iter()
                .last()
                .unwrap()
                .upgrade()
                .unwrap(),
            &focused
        ));
    }
    #[derive(Default)]
    struct RemovingHost {
        view: RefCell<Weak<CpdfSdkPageView>>,
        calls: Cell<usize>,
    }
    impl FormFillHost for RemovingHost {
        fn invalidate(&self, _page: &SdkPage, _rect: FxRect) -> bool {
            self.calls.set(self.calls.get() + 1);
            if let Some(view) = self.view.borrow().upgrade() {
                view.clear_page();
            }
            true
        }
    }
    #[test]
    fn hover_callback_can_remove_page_and_invalidate_snapshot_handles() {
        let host = Rc::new(RemovingHost::default());
        let interface: Rc<dyn FormFillHost> = host.clone();
        let (_doc, page, env, view) = fixture(
            Some(&interface),
            b"S",
            &[CfxFloatRect::new(0.0, 0.0, 10.0, 10.0)],
        );
        *host.view.borrow_mut() = Rc::downgrade(&view);
        let snapshot = CpdfSdkAnnotIteration::new(&view);
        let retained = view.annot_list()[0].clone();
        assert!(!view.on_mouse_move(0, CfxPointF::new(5.0, 5.0)));
        assert_eq!(host.calls.get(), 1);
        assert!(env.get_page_view(&page).is_none());
        assert!(!view.is_alive());
        assert!(!retained.is_alive());
        assert!(snapshot
            .iter()
            .all(|annot| annot.upgrade().is_none_or(|annot| !annot.is_alive())));
    }
    #[test]
    fn closing_page_invalidates_retained_annotations_even_while_loading() {
        let (_doc, page, env, view) =
            fixture(None, b"S", &[CfxFloatRect::new(0.0, 0.0, 10.0, 10.0)]);
        let annot = view.annot_list()[0].clone();
        view.locked.set(true);
        page.invalidate();
        assert!(!annot.is_alive());
        assert!(!view.is_alive());
        assert!(view.annots.borrow().is_empty());
        assert!(env.get_page_view(&page).is_none());
        page.invalidate();
    }
    #[test]
    fn page_bridge_retains_identity_without_retaining_document() {
        let (doc, page, env, view) = fixture(None, b"S", &[]);
        let dict = page.dict();
        assert!(Rc::ptr_eq(&dict, &view.page().unwrap().dict()));
        assert_eq!(page.index(), 0);
        let geometry = page.geometry_page().unwrap();
        assert!(Rc::ptr_eq(&geometry.page_data_ref(), &page.page_data()));
        drop(doc);
        assert!(!page.is_alive());
        assert!(!view.is_alive());
        assert!(env.document().is_none());
    }
    #[test]
    #[cfg(feature = "js")]
    fn action_error_is_scoped_to_outer_input_and_preserved_during_nested_events() {
        use crate::fpdfdoc::cpdf_action::CpdfAction;
        use crate::fpdfsdk::cpdfsdk_formfillenvironment::ActionError;
        let (_doc, _page, env, view) =
            fixture(None, b"S", &[CfxFloatRect::new(0.0, 0.0, 10.0, 10.0)]);
        let mut action = CpdfDictionary::new();
        action.set_new_name_for(b"S", b"JavaScript");
        action.set_new_for(
            b"JS",
            CpdfString::from_bytes(None, b"throw Error('script');"),
        );
        let action = CpdfAction::new(Some(retain_object(action)));
        let execute_script = || {
            let error = env.do_action_javascript(&action, &[]).unwrap_err();
            env.record_action_error(error);
        };
        // A caller dispatching document JavaScript uses the same outer scope
        // as a page input operation. Link Enter only handles GoTo/URI actions.
        env.run_input_event(execute_script);
        assert_eq!(env.action_error(), Some(ActionError::JavaScriptException));
        assert!(view.on_mouse_move(0, CfxPointF::new(5.0, 5.0)));
        assert_eq!(env.action_error(), None);
        env.run_input_event(|| {
            execute_script();
            assert!(view.on_mouse_move(0, CfxPointF::new(5.0, 5.0)));
            assert_eq!(env.action_error(), Some(ActionError::JavaScriptException));
        });
        assert_eq!(env.action_error(), Some(ActionError::JavaScriptException));
        assert!(!view.on_mouse_move(0, CfxPointF::new(50.0, 50.0)));
        assert_eq!(env.action_error(), None);
    }
}
