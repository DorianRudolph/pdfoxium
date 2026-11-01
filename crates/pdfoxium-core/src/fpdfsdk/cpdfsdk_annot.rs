// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `fpdfsdk/cpdfsdk_annot.{h,cpp}`. Safe observed annotations compose their
//! native basic/widget subclasses. Page views own annotations; backedges and
//! iteration snapshots are weak. Logical removal invalidates retained handles.
use super::cpdfsdk_baannot::CpdfSdkBaAnnot;
use super::cpdfsdk_pageview::{CpdfSdkPageView, SdkPage};
use super::cpdfsdk_widget::CpdfSdkWidget;
use super::fpdf_fwlevent::FWL_VKEY_Return;
#[cfg(feature = "xfa")]
use super::fpdfxfa::cpdfxfa_widget::CpdfXfaWidget;
use super::pwl::pwl_draw::PwlDraw;
use crate::fpdfapi::parser::cpdf_object::ObjectRef;
use crate::fpdfdoc::cpdf_aaction::AActionType;
use crate::fpdfdoc::cpdf_annot::{AppearanceMode, CpdfAnnot, Subtype};
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxge::cfx_fillrenderoptions::CfxFillRenderOptions;
use crate::fxge::cfx_graphstatedata::CfxGraphStateData;
use crate::fxge::cfx_path::CfxPath;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

#[derive(Clone, Copy, Debug)]
pub enum SdkInputEvent {
    MouseEnter(u32),
    MouseExit(u32),
    LButtonDown(u32, CfxPointF),
    LButtonUp(u32, CfxPointF),
    LButtonDblClk(u32, CfxPointF),
    MouseMove(u32, CfxPointF),
    MouseWheel(u32, CfxPointF, CfxPointF),
    RButtonDown(u32, CfxPointF),
    RButtonUp(u32, CfxPointF),
    Char(u32, u32),
    KeyDown(i32, u32),
    SetFocus(u32),
    KillFocus(u32),
}
enum Kind {
    Basic(CpdfSdkBaAnnot),
    Widget(Rc<CpdfSdkWidget>),
    #[cfg(feature = "xfa")]
    Xfa(Rc<CpdfXfaWidget>),
}
pub struct CpdfSdkAnnot {
    page_view: Weak<CpdfSdkPageView>,
    kind: Kind,
    pdf_annot: Option<RefCell<CpdfAnnot>>,
    alive: Cell<bool>,
    focused: Cell<bool>,
}
impl CpdfSdkAnnot {
    pub fn basic(annot: CpdfAnnot, view: &Rc<CpdfSdkPageView>) -> Rc<Self> {
        let basic = CpdfSdkBaAnnot::new(Rc::clone(annot.dict()));
        Rc::new(Self {
            page_view: Rc::downgrade(view),
            kind: Kind::Basic(basic),
            pdf_annot: Some(RefCell::new(annot)),
            alive: Cell::new(true),
            focused: Cell::new(false),
        })
    }
    pub fn for_widget(
        annot: CpdfAnnot,
        widget: Rc<CpdfSdkWidget>,
        view: &Rc<CpdfSdkPageView>,
    ) -> Rc<Self> {
        let result = Rc::new(Self {
            page_view: Rc::downgrade(view),
            kind: Kind::Widget(widget.clone()),
            pdf_annot: Some(RefCell::new(annot)),
            alive: Cell::new(true),
            focused: Cell::new(false),
        });
        widget.bind_annotation(&result);
        result
    }
    #[cfg(feature = "xfa")]
    pub fn for_xfa(widget: Rc<CpdfXfaWidget>, view: &Rc<CpdfSdkPageView>) -> Rc<Self> {
        let result = Rc::new(Self {
            page_view: Rc::downgrade(view),
            kind: Kind::Xfa(widget.clone()),
            pdf_annot: None,
            alive: Cell::new(true),
            focused: Cell::new(false),
        });
        widget.bind_annotation(&result);
        result
    }
    #[cfg(feature = "xfa")]
    pub fn xfa_widget(&self) -> Option<Rc<CpdfXfaWidget>> {
        match &self.kind {
            Kind::Xfa(widget) => Some(widget.clone()),
            _ => None,
        }
    }
    pub fn pdf_dict(&self) -> Option<ObjectRef> {
        Some(Rc::clone(self.pdf_annot.as_ref()?.borrow().dict()))
    }
    pub fn page_view(&self) -> Option<Rc<CpdfSdkPageView>> {
        self.page_view.upgrade().filter(|view| view.is_alive())
    }
    pub fn page(&self) -> Option<Rc<SdkPage>> {
        self.page_view()?.page()
    }
    pub fn is_alive(&self) -> bool {
        self.alive.get() && self.page_view.upgrade().is_some_and(|view| view.is_alive())
    }
    pub fn invalidate(&self) {
        self.alive.set(false);
    }
    pub fn widget(&self) -> Option<Rc<CpdfSdkWidget>> {
        match &self.kind {
            Kind::Widget(widget) => Some(widget.clone()),
            _ => None,
        }
    }
    pub fn as_ba_annot(&self) -> &CpdfSdkBaAnnot {
        match &self.kind {
            Kind::Basic(annot) => annot,
            Kind::Widget(widget) => widget.annot(),
            #[cfg(feature = "xfa")]
            Kind::Xfa(_) => panic!("XFA widget has no PDF basic annotation"),
        }
    }
    pub fn dict(&self) -> ObjectRef {
        self.pdf_dict()
            .expect("XFA widgets have no PDF annotation dictionary")
    }
    pub fn subtype(&self) -> Subtype {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.subtype();
        }
        self.pdf_annot
            .as_ref()
            .expect("PDF annotation branch")
            .borrow()
            .subtype()
    }
    pub fn rect(&self) -> CfxFloatRect {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.rect();
        }
        self.pdf_annot
            .as_ref()
            .expect("PDF annotation branch")
            .borrow()
            .rect()
    }
    pub fn view_bbox(&self) -> CfxFloatRect {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.view_bbox();
        }
        self.widget()
            .map_or_else(|| self.rect(), |widget| widget.view_bbox())
    }
    pub fn layout_order(&self) -> i32 {
        #[cfg(feature = "xfa")]
        if self.xfa_widget().is_some() {
            return 5;
        }
        self.widget().map_or_else(
            || self.as_ba_annot().layout_order(),
            |widget| widget.layout_order(),
        )
    }
    pub fn optional_content_visible(&self) -> bool {
        #[cfg(feature = "xfa")]
        if self.xfa_widget().is_some() {
            return true;
        }
        let oc = self
            .dict()
            .borrow()
            .as_dictionary()
            .and_then(|d| d.get_dict_for(b"OC"));
        let Some(oc) = oc else { return true };
        let Some(doc) = self.page_view().and_then(|v| v.document()) else {
            return true;
        };
        let visible = crate::fpdfapi::page::cpdf_occontext::CpdfOcContext::new(
            &doc.borrow(),
            crate::fpdfapi::page::cpdf_occontext::UsageType::View,
        )
        .check_ocg_dict_visible(Some(&oc));
        visible
    }
    pub fn do_hit_test(&self, point: CfxPointF) -> bool {
        if !self.optional_content_visible() {
            return false;
        }
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.do_hit_test(point);
        }
        self.widget()
            .is_some_and(|widget| widget.do_hit_test(point))
    }
    pub fn on_load(&self) {
        if let Some(widget) = self.widget() {
            widget.on_load();
        }
    }
    pub fn on_draw(&self, device: &mut dyn PwlDraw, matrix: CfxMatrix, draw_annots: bool) {
        if !self.is_alive() || !self.optional_content_visible() {
            return;
        }
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            device.draw_xfa_widget(&xfa, matrix);
            return;
        }
        if let Some(widget) = self.widget() {
            widget.on_draw(device, matrix, draw_annots);
            return;
        }
        if !self.as_ba_annot().is_visible() {
            return;
        }
        if draw_annots && self.subtype() == Subtype::Popup {
            let mut annot = self
                .pdf_annot
                .as_ref()
                .expect("PDF annotation branch")
                .borrow_mut();
            if !annot.is_open() {
                return;
            }
            annot.generate_ap_if_needed();
            drop(annot);
            if let Some(page) = self.page() {
                device.draw_annotation_ap(&page, &self.dict(), AppearanceMode::Normal, matrix);
            }
            return;
        }
        let Some(view) = self.page_view() else { return };
        let Some(env) = view.environment() else {
            return;
        };
        if !self.focused.get()
            || !env.focusable_annot_subtypes().contains(&self.subtype())
            || !env
                .get_focus_annot()
                .is_some_and(|focused| std::ptr::eq(focused.as_ref(), self))
        {
            return;
        }
        let mut rect = self.view_bbox();
        if rect.is_empty() {
            return;
        }
        rect.normalize();
        let mut path = CfxPath::new();
        path.append_rect(rect.left, rect.bottom, rect.right, rect.top);
        let mut graph = CfxGraphStateData::default();
        graph.set_dash_array(vec![1.0]);
        device.draw_path(
            &path,
            matrix,
            Some(&graph),
            0,
            0xff000000,
            CfxFillRenderOptions::even_odd_options(),
        );
    }
    fn input(self: &Rc<Self>, event: SdkInputEvent) -> bool {
        let environment = self.page_view().and_then(|view| view.environment());
        let _event = environment
            .as_ref()
            .map(|environment| environment.begin_input_event());
        if !self.is_alive()
            || (!matches!(
                event,
                SdkInputEvent::KillFocus(_) | SdkInputEvent::MouseExit(_)
            ) && !self.optional_content_visible())
        {
            return false;
        }
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.dispatch(event);
        }
        if let Some(widget) = self.widget() {
            let Some(env) = self.page_view().and_then(|view| view.environment()) else {
                return false;
            };
            if matches!(
                event,
                SdkInputEvent::SetFocus(_) | SdkInputEvent::KillFocus(_)
            ) {
                if !env.focusable_annot_subtypes().contains(&self.subtype()) {
                    return false;
                }
                if widget.is_signature_widget() {
                    return true;
                }
            }
            return env.dispatch_annot_event(self, event);
        }
        match event {
            SdkInputEvent::MouseEnter(_) => {
                self.pdf_annot
                    .as_ref()
                    .expect("PDF annotation branch")
                    .borrow()
                    .set_popup_annot_open_state(true);
                self.update_annot_rects();
                false
            }
            SdkInputEvent::MouseExit(_) => {
                self.pdf_annot
                    .as_ref()
                    .expect("PDF annotation branch")
                    .borrow()
                    .set_popup_annot_open_state(false);
                self.update_annot_rects();
                false
            }
            SdkInputEvent::SetFocus(_) | SdkInputEvent::KillFocus(_) => {
                let Some(view) = self.page_view() else {
                    return false;
                };
                let Some(env) = view.environment() else {
                    return false;
                };
                if !env.focusable_annot_subtypes().contains(&self.subtype()) {
                    return false;
                }
                self.focused
                    .set(matches!(event, SdkInputEvent::SetFocus(_)));
                self.invalidate_rect();
                true
            }
            SdkInputEvent::KeyDown(key, flags)
                if key == FWL_VKEY_Return && self.subtype() == Subtype::Link =>
            {
                let Some(view) = self.page_view() else {
                    return false;
                };
                let Some(env) = view.environment() else {
                    return false;
                };
                let action = self.as_ba_annot().additional_action(AActionType::KeyStroke);
                if action.has_dict() {
                    return match env.do_action_link(&action, AActionType::KeyStroke, flags) {
                        Ok(result) => result,
                        Err(error) => {
                            env.record_action_error(error);
                            false
                        }
                    };
                }
                let Some(document) = view.document() else {
                    return false;
                };
                let dest = self.as_ba_annot().destination(&document.borrow());
                match env.do_action_destination(&dest) {
                    Ok(result) => result,
                    Err(error) => {
                        env.record_action_error(error);
                        false
                    }
                }
            }
            _ => false, // Native BAAnnot default handlers do not consume input.
        }
    }
    fn update_annot_rects(&self) {
        let mut rects = vec![self.rect()];
        if let Some(popup) = self
            .pdf_annot
            .as_ref()
            .expect("PDF annotation branch")
            .borrow()
            .popup_annot_rect()
        {
            rects.push(popup);
        }
        for rect in &mut rects {
            rect.inflate_xy(1.0, 1.0);
        }
        if let Some(view) = self.page_view() {
            view.update_rects(&rects);
        }
    }
    fn invalidate_rect(&self) {
        let mut rect = self.view_bbox();
        if rect.is_empty() {
            return;
        }
        rect.inflate_xy(1.0, 1.0);
        rect.normalize();
        if let Some(view) = self.page_view() {
            view.update_rects(&[rect]);
        }
    }
    pub fn on_mouse_enter(self: &Rc<Self>, flags: u32) {
        self.input(SdkInputEvent::MouseEnter(flags));
    }
    pub fn on_mouse_exit(self: &Rc<Self>, flags: u32) {
        self.input(SdkInputEvent::MouseExit(flags));
    }
    pub fn on_l_button_down(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::LButtonDown(flags, point))
    }
    pub fn on_l_button_up(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::LButtonUp(flags, point))
    }
    pub fn on_l_button_dbl_clk(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::LButtonDblClk(flags, point))
    }
    pub fn on_r_button_down(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::RButtonDown(flags, point))
    }
    pub fn on_r_button_up(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::RButtonUp(flags, point))
    }
    pub fn on_mouse_move(self: &Rc<Self>, flags: u32, point: CfxPointF) -> bool {
        self.input(SdkInputEvent::MouseMove(flags, point))
    }
    pub fn on_mouse_wheel(self: &Rc<Self>, flags: u32, point: CfxPointF, delta: CfxPointF) -> bool {
        self.input(SdkInputEvent::MouseWheel(flags, point, delta))
    }
    pub fn on_char(self: &Rc<Self>, word: u32, flags: u32) -> bool {
        self.input(SdkInputEvent::Char(word, flags))
    }
    pub fn on_key_down(self: &Rc<Self>, key: i32, flags: u32) -> bool {
        self.input(SdkInputEvent::KeyDown(key, flags))
    }
    pub fn on_set_focus(self: &Rc<Self>, flags: u32) -> bool {
        self.input(SdkInputEvent::SetFocus(flags))
    }
    pub fn on_kill_focus(self: &Rc<Self>, flags: u32) -> bool {
        self.input(SdkInputEvent::KillFocus(flags))
    }
    pub fn text(&self) -> Vec<u32> {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.get_text();
        }
        self.widget().map_or_else(Vec::new, |w| w.get_text())
    }
    pub fn selected_text(&self) -> Vec<u32> {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.get_selected_text();
        }
        self.widget()
            .map_or_else(Vec::new, |w| w.get_selected_text())
    }
    pub fn replace_selection(&self, text: &[u32]) {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            xfa.replace_selection(text);
            return;
        }
        if let Some(w) = self.widget() {
            w.replace_selection(text);
        }
    }
    pub fn replace_and_keep_selection(&self, text: &[u32]) {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            xfa.replace_and_keep_selection(text);
            return;
        }
        if let Some(w) = self.widget() {
            w.replace_and_keep_selection(text);
        }
    }
    pub fn select_all_text(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.select_all_text();
        }
        self.widget().is_some_and(|w| w.select_all_text())
    }
    pub fn can_undo(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.can_undo();
        }
        self.widget().is_some_and(|w| w.can_undo())
    }
    pub fn can_redo(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.can_redo();
        }
        self.widget().is_some_and(|w| w.can_redo())
    }
    pub fn undo(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.undo();
        }
        self.widget().is_some_and(|w| w.undo())
    }
    pub fn redo(&self) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.redo();
        }
        self.widget().is_some_and(|w| w.redo())
    }
    pub fn set_index_selected(&self, index: i32, selected: bool) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.set_index_selected(index, selected);
        }
        self.widget()
            .is_some_and(|w| w.set_index_selected(index, selected))
    }
    pub fn is_index_selected(&self, index: i32) -> bool {
        #[cfg(feature = "xfa")]
        if let Some(xfa) = self.xfa_widget() {
            return xfa.is_index_selected(index);
        }
        self.widget().is_some_and(|w| w.is_index_selected(index))
    }
}
