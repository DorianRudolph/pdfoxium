// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cxfa_fffield.{h,cpp}` on the actual FWL controls, model and caption layout.
use super::{
    cxfa_eventparam::{CxfaEventParam, XfaEventType},
    cxfa_ffwidget::{
        rect_without_margin, CxfaFfWidget, FfResult, FfWidget, FfWidgetAdapter, FocusOption,
        HighlightOption,
    },
    fxfa::{AlertButton, AlertIcon, AlertReturn, XfaWidgetStatus as S},
    fxfa_basic::{XfaAttributeValue as V, XfaElement as E},
    layout::{cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::LayoutItemWidget},
    parser::cxfa_node::{CxfaNode, XfaFfWidgetType as Kind, XfaNodeFlag},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxSizeF, CfxVector},
    fxge::cfx_fillrenderoptions::FillType,
    xfa::{
        fgas::graphics::{
            cfgas_gecolor::CfgasGeColor, cfgas_gegraphics::CfgasGeGraphics,
            cfgas_gepath::CfgasGePath,
        },
        fwl::{
            cfwl_edit::FWL_STYLEEXT_EDT_READ_ONLY,
            cfwl_event::{EventData, FwlEvent},
            cfwl_message::{FwlMessage, KeyCommand, MouseCommand},
            cfwl_widget::{WidgetAdapter, WidgetRef},
            fwl_widgetdef::*,
            fwl_widgethit::FwlWidgetHit,
            ifwl_widgetdelegate::WidgetDelegate,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};
const MIN_UI_HEIGHT: f32 = 4.32;
const DEFAULT_UI_HEIGHT: f32 = 2.;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ShapeOption {
    Square,
    Round,
}
pub struct CxfaFfField {
    pub base: CxfaFfWidget,
    normal_widget: RefCell<Option<WidgetRef>>,
    ui_rect: Cell<CfxRectF>,
    caption_rect: Cell<CfxRectF>,
}
impl std::ops::Deref for CxfaFfField {
    type Target = CxfaFfWidget;
    fn deref(&self) -> &Self::Target {
        &self.base
    }
}
impl CxfaFfField {
    pub fn new(node: &CxfaNode) -> Rc<Self> {
        Rc::new(Self::new_base(node))
    }
    pub fn new_base(node: &CxfaNode) -> Self {
        Self {
            base: CxfaFfWidget::new_base(node),
            normal_widget: RefCell::new(None),
            ui_rect: Cell::new(CfxRectF::default()),
            caption_rect: Cell::new(CfxRectF::default()),
        }
    }
    pub fn normal_widget(&self) -> Option<WidgetRef> {
        self.normal_widget.borrow().clone()
    }
    pub fn set_normal_widget(&self, widget: Option<WidgetRef>) {
        let old = self.normal_widget.replace(widget);
        drop(old);
    }
    /// Concrete FF widgets own FWL controls; the FWL adapter/delegate keeps
    /// only a weak edge back to the concrete dynamic FF widget.
    pub fn attach_normal_widget(&self, owner: &Rc<dyn FfWidget>, widget: WidgetRef) {
        let weak = Rc::downgrade(owner);
        widget
            .base()
            .set_adapter_iface(Some(Rc::new(FfWidgetAdapter(weak.clone()))));
        widget
            .base()
            .set_delegate(Some(Rc::new(FfFieldDelegate(weak))));
        self.set_normal_widget(Some(widget));
    }
    pub fn ui_rect(&self) -> CfxRectF {
        self.ui_rect.get()
    }
    pub fn set_ui_rect(&self, rect: CfxRectF) {
        self.ui_rect.set(rect);
    }
    pub fn caption_rect(&self) -> CfxRectF {
        self.caption_rect.get()
    }
    pub fn set_caption_rect(&self, rect: CfxRectF) {
        self.caption_rect.set(rect);
    }
    pub fn get_bbox(&self, focus: FocusOption) -> CfxRectF {
        if focus == FocusOption::DoNotDrawFocus {
            return self.base.get_bbox(focus);
        }
        if matches!(
            self.base.node().get_ff_widget_type(),
            Kind::Button | Kind::CheckButton | Kind::ImageEdit | Kind::Signature | Kind::ChoiceList
        ) {
            self.get_rotate_matrix().transform_rect(self.ui_rect.get())
        } else {
            CfxRectF::default()
        }
    }
    pub fn render_widget(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
    ) -> FfResult<bool> {
        if !self.base.has_visible_status() {
            return Ok(true);
        }
        let mut rotate = self.get_rotate_matrix();
        rotate.concat(matrix);
        let mut success = self.base.render_widget(graphics, rotate, highlight)?;
        success &= self.base.draw_border(
            graphics,
            self.base.node().get_ui_border().as_ref(),
            self.ui_rect.get(),
            rotate,
            false,
        );
        success &= self.render_caption(graphics, rotate)?;
        self.draw_highlight(graphics, rotate, highlight, ShapeOption::Square);
        if let Some(widget) = self.normal_widget() {
            let rect = widget.get_widget_rect();
            let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
            translated.concat(rotate);
            if let Some(app) = self.base.fwl_app() {
                app.widget_mgr()
                    .on_draw_widget(&widget, graphics, translated);
            }
        }
        Ok(success)
    }
    pub fn draw_highlight(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
        highlight: HighlightOption,
        shape: ShapeOption,
    ) {
        if highlight == HighlightOption::NoHighlight || self.ui_rect.get().is_empty() {
            return;
        }
        let Some(host) = self.base.doc_view() else {
            return;
        };
        if !host.is_interactive() || !self.base.node().is_open_access() {
            return;
        }
        graphics.set_fill_color(&CfgasGeColor::Solid(host.highlight_color()));
        let mut path = CfgasGePath::new();
        let rect = self.ui_rect.get();
        if shape == ShapeOption::Round {
            path.add_ellipse(rect);
        } else {
            path.add_rectangle(rect.left, rect.top, rect.width, rect.height);
        }
        graphics.fill_path(&path, FillType::Winding, matrix);
    }
    pub fn is_loaded(&self) -> bool {
        self.normal_widget().is_some() && self.base.is_loaded()
    }
    pub fn load_widget(&self) -> FfResult<bool> {
        self.load_widget_with_layout(|| self.perform_layout())
    }
    pub fn load_widget_with_layout(
        &self,
        perform: impl FnOnce() -> FfResult<()>,
    ) -> FfResult<bool> {
        self.base
            .require_doc_view()?
            .load_caption(self.base.node())?;
        perform()?;
        Ok(true)
    }
    pub fn load_widget_with_rect(&self, set_rect: impl FnOnce()) -> FfResult<bool> {
        self.load_widget_with_layout(|| self.perform_layout_with_rect(set_rect))
    }
    pub fn perform_layout(&self) -> FfResult<()> {
        self.perform_layout_with_rect(|| self.set_fwl_rect())
    }
    pub fn perform_layout_with_rect(&self, set_rect: impl FnOnce()) -> FfResult<()> {
        self.base.perform_layout();
        self.cap_placement()?;
        self.layout_caption()?;
        set_rect();
        self.set_edit_scroll_offset();
        if let Some(widget) = self.normal_widget() {
            widget.update();
        }
        Ok(())
    }
    pub fn set_edit_scroll_offset(&self) {
        let Some(widget) = self.normal_widget() else {
            return;
        };
        if !matches!(
            self.base.node().get_ff_widget_type(),
            Kind::TextEdit | Kind::NumericEdit | Kind::PasswordEdit
        ) {
            return;
        }
        let mut previous = self
            .base
            .layout_item()
            .and_then(|item| item.previous())
            .and_then(|item| CxfaFfWidget::from_layout_item(&item));
        let mut offset = if previous
            .as_ref()
            .and_then(|widget| widget.as_ff_widget())
            .and_then(FfWidget::as_field)
            .is_some()
        {
            -self.base.node().get_ui_margin().top
        } else {
            0.
        };
        while let Some(owner) = previous {
            let Some(field) = owner.as_ff_widget().and_then(FfWidget::as_field) else {
                break;
            };
            offset += field.ui_rect().height;
            previous = field
                .base
                .layout_item()
                .and_then(|item| item.previous())
                .and_then(|item| CxfaFfWidget::from_layout_item(&item));
        }
        if let Some(edit) = widget.as_edit() {
            edit.set_scroll_offset(offset);
        }
    }
    pub fn cap_placement(&self) -> FfResult<()> {
        let node = self.base.node();
        let mut widget = self.base.get_rect_without_rotate();
        let margin = node.get_margin_if_exists();
        let item = self.base.layout_item();
        let previous = item.as_ref().is_some_and(|item| item.previous().is_some());
        let next = item.as_ref().is_some_and(|item| item.next().is_some());
        if let Some(margin) = &margin {
            widget.deflate(
                margin.margin_get_left_inset(),
                if previous {
                    0.
                } else {
                    margin.margin_get_top_inset()
                },
                margin.margin_get_right_inset(),
                if next {
                    0.
                } else {
                    margin.margin_get_bottom_inset()
                },
            );
        }
        let (mut placement, mut reserve) = (V::Unknown, 0f32);
        let caption = node.get_caption_if_exists();
        let mut caption_rect = CfxRectF::default();
        if let Some(caption) = caption
            .as_ref()
            .filter(|caption| !caption.caption_is_hidden())
        {
            placement = caption.caption_get_placement_type();
            if (placement == V::Top && previous) || (placement == V::Bottom && next) {
                caption_rect = CfxRectF::default();
            } else {
                reserve =
                    caption
                        .caption_get_reserve()
                        .min(if matches!(placement, V::Top | V::Bottom) {
                            widget.height
                        } else {
                            widget.width
                        });
                if !previous && !next {
                    caption_rect = widget;
                } else if let Some(item) = item {
                    let first = item.get_first();
                    caption_rect = first.get_absolute_rect();
                    let mut next = first.next();
                    while let Some(item) = next {
                        caption_rect.height += item.get_absolute_rect().height;
                        next = item.next();
                    }
                    rect_without_margin(&mut caption_rect, margin.as_ref());
                }
                if reserve <= 0. {
                    if let Some(layout) = self
                        .base
                        .doc_view()
                        .and_then(|host| host.caption_layout(node))
                    {
                        let size = layout.with_layout(|layout| {
                            layout.calc_size(CfxSizeF::default(), CfxSizeF::default())
                        })?;
                        reserve = if matches!(placement, V::Top | V::Bottom) {
                            size.height
                        } else {
                            size.width
                        };
                    }
                }
            }
        }
        self.ui_rect.set(widget);
        self.caption_rect.set(caption_rect);
        let cap_margin = caption.and_then(|caption| caption.caption_get_margin_if_exists());
        match placement {
            V::Left => {
                caption_rect.width = reserve;
                self.caption_rect.set(caption_rect);
                self.cap_left_right_placement(cap_margin.as_ref(), widget, placement);
                let mut ui = self.ui_rect.get();
                ui.width -= reserve;
                ui.left += reserve;
                self.ui_rect.set(ui);
            }
            V::Top => {
                caption_rect.height = reserve;
                self.caption_rect.set(caption_rect);
                self.cap_top_bottom_placement(cap_margin.as_ref(), widget, placement);
                let mut ui = self.ui_rect.get();
                ui.top += reserve;
                ui.height -= reserve;
                self.ui_rect.set(ui);
            }
            V::Right => {
                caption_rect.left = caption_rect.right() - reserve;
                caption_rect.width = reserve;
                self.caption_rect.set(caption_rect);
                self.cap_left_right_placement(cap_margin.as_ref(), widget, placement);
                let mut ui = self.ui_rect.get();
                ui.width -= reserve;
                self.ui_rect.set(ui);
            }
            V::Bottom => {
                caption_rect.top = caption_rect.bottom() - reserve;
                caption_rect.height = reserve;
                self.caption_rect.set(caption_rect);
                self.cap_top_bottom_placement(cap_margin.as_ref(), widget, placement);
                let mut ui = self.ui_rect.get();
                ui.height -= reserve;
                self.ui_rect.set(ui);
            }
            _ => {}
        }
        let mut ui = self.ui_rect.get();
        if let Some(border) = node.get_ui_border() {
            rect_without_margin(&mut ui, border.get_margin_if_exists().as_ref());
        }
        ui.normalize();
        self.ui_rect.set(ui);
        Ok(())
    }
    pub fn cap_top_bottom_placement(
        &self,
        margin: Option<&CxfaNode>,
        widget: CfxRectF,
        placement: V,
    ) {
        let ui_margin = self.base.node().get_ui_margin();
        let mut caption = self.caption_rect.get();
        let mut ui = self.ui_rect.get();
        caption.left += ui_margin.left;
        if margin.is_some() {
            rect_without_margin(&mut caption, margin);
            if caption.height < 0. {
                caption.top += caption.height;
            }
        }
        let width = ui_margin.left + ui_margin.width;
        let height = caption.height + ui_margin.top + ui_margin.height;
        if width > widget.width {
            ui.width += width - widget.width;
        }
        if height == DEFAULT_UI_HEIGHT && ui.height < MIN_UI_HEIGHT {
            ui.height = MIN_UI_HEIGHT;
            caption.top += ui_margin.top + ui_margin.height;
        } else if height > widget.height {
            ui.height += height - widget.height;
            if placement == V::Bottom {
                caption.top += height - widget.height;
            }
        }
        self.ui_rect.set(ui);
        self.caption_rect.set(caption);
    }
    pub fn cap_left_right_placement(
        &self,
        margin: Option<&CxfaNode>,
        widget: CfxRectF,
        placement: V,
    ) {
        let ui_margin = self.base.node().get_ui_margin();
        let mut caption = self.caption_rect.get();
        let mut ui = self.ui_rect.get();
        caption.top += ui_margin.top;
        caption.height -= ui_margin.top;
        if margin.is_some() {
            rect_without_margin(&mut caption, margin);
            if caption.height < 0. {
                caption.top += caption.height;
            }
        }
        let width = caption.width + ui_margin.left + ui_margin.width;
        let height = ui_margin.top + ui_margin.height;
        if width > widget.width {
            ui.width += width - widget.width;
            if placement == V::Right {
                caption.left += width - widget.width;
            }
        }
        if height == DEFAULT_UI_HEIGHT && ui.height < MIN_UI_HEIGHT {
            ui.height = MIN_UI_HEIGHT;
            caption.top += ui_margin.top + ui_margin.height;
        } else if height > widget.height {
            ui.height += height - widget.height;
        }
        self.ui_rect.set(ui);
        self.caption_rect.set(caption);
    }
    pub fn update_fwl(&self) {
        if let Some(widget) = self.normal_widget() {
            widget.update();
        }
    }
    pub fn update_ui_property(&self) -> u32 {
        if self
            .base
            .node()
            .get_ui_child_node()
            .is_some_and(|node| node.get_element_type() == E::DefaultUi)
        {
            FWL_STYLEEXT_EDT_READ_ONLY
        } else {
            0
        }
    }
    pub fn set_fwl_rect(&self) {
        let Some(widget) = self.normal_widget() else {
            return;
        };
        let mut rect = self.ui_rect.get();
        rect.width = rect.width.max(1.);
        if self
            .base
            .doc_view()
            .is_some_and(|host| !host.is_interactive())
        {
            rect.height = rect.height.max(self.base.node().get_font_size());
        }
        widget.base().set_widget_rect(rect);
    }
    pub fn fwl_to_client(&self, point: CfxPointF) -> CfxPointF {
        self.normal_widget().map_or(point, |widget| {
            let rect = widget.get_widget_rect();
            CfxPointF::new(point.x - rect.left, point.y - rect.top)
        })
    }
    pub fn pt_in_active_rect(&self, point: CfxPointF) -> bool {
        self.normal_widget()
            .is_some_and(|widget| widget.get_widget_rect().contains_point(point))
    }
    pub fn accepts_focus_on_button_down(
        &self,
        _flags: u32,
        point: CfxPointF,
        _command: MouseCommand,
    ) -> bool {
        self.normal_widget().is_some()
            && self.base.node().is_open_access()
            && self
                .base
                .doc_view()
                .is_some_and(|host| host.is_interactive())
            && self.pt_in_active_rect(point)
    }
    pub fn send_message_to_fwl_widget(&self, message: &mut FwlMessage) -> bool {
        let Some(app) = self.base.fwl_app() else {
            return false;
        };
        app.widget_mgr().on_process_message_to_form(message);
        true
    }
    fn mouse_message(&self, command: MouseCommand, flags: u32, point: CfxPointF) -> bool {
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        let mut message =
            FwlMessage::mouse(Some(widget), command, flags, self.fwl_to_client(point));
        self.send_message_to_fwl_widget(&mut message)
    }
    pub fn on_mouse_enter(&self) -> bool {
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        self.send_message_to_fwl_widget(&mut FwlMessage::mouse(
            Some(widget),
            MouseCommand::Enter,
            0,
            CfxPointF::default(),
        ))
    }
    pub fn on_mouse_exit(&self) -> bool {
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        self.send_message_to_fwl_widget(&mut FwlMessage::mouse(
            Some(widget),
            MouseCommand::Leave,
            0,
            CfxPointF::default(),
        ))
    }
    pub fn on_l_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.base.set_button_down(true);
        self.mouse_message(MouseCommand::LeftButtonDown, flags, point)
    }
    pub fn on_l_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if self.normal_widget().is_none() || !self.base.is_button_down() {
            return false;
        }
        self.base.set_button_down(false);
        self.mouse_message(MouseCommand::LeftButtonUp, flags, point)
    }
    pub fn on_l_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse_message(MouseCommand::LeftButtonDblClk, flags, point)
    }
    pub fn on_mouse_move(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse_message(MouseCommand::Move, flags, point)
    }
    pub fn on_mouse_wheel(&self, _flags: u32, point: CfxPointF, delta: CfxVector) -> bool {
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        self.send_message_to_fwl_widget(&mut FwlMessage::mouse_wheel(
            Some(widget),
            self.fwl_to_client(point),
            delta,
        ))
    }
    pub fn on_r_button_down(&self, flags: u32, point: CfxPointF) -> bool {
        self.base.set_button_down(true);
        self.mouse_message(MouseCommand::RightButtonDown, flags, point)
    }
    pub fn on_r_button_up(&self, flags: u32, point: CfxPointF) -> bool {
        if self.normal_widget().is_none() || !self.base.is_button_down() {
            return false;
        }
        self.base.set_button_down(false);
        self.mouse_message(MouseCommand::RightButtonUp, flags, point)
    }
    pub fn on_r_button_dbl_clk(&self, flags: u32, point: CfxPointF) -> bool {
        self.mouse_message(MouseCommand::RightButtonDblClk, flags, point)
    }
    pub fn on_set_focus(&self, old: Option<&dyn FfWidget>) -> FfResult<bool> {
        if !self.base.on_set_focus(old)? {
            return Ok(false);
        }
        let Some(widget) = self.normal_widget() else {
            return Ok(false);
        };
        if !self.send_message_to_fwl_widget(&mut FwlMessage::set_focus(Some(widget))) {
            return Ok(false);
        }
        self.base.modify_status(S::kFocused, S::kNone);
        self.base.invalidate_rect();
        Ok(true)
    }
    pub fn on_kill_focus(&self, new: Option<&dyn FfWidget>) -> FfResult<bool> {
        if let Some(widget) = self.normal_widget() {
            self.send_message_to_fwl_widget(&mut FwlMessage::kill_focus(Some(widget)));
            self.base.modify_status(S::kNone, S::kFocused);
            self.base.invalidate_rect();
        }
        // Clearing focus has no successor. The base still clears status and
        // dispatches Exit; native's extra null gate stranded the view's old
        // focused-widget pointer after its FWL control had already blurred.
        self.base.on_kill_focus(new)
    }
    pub fn on_key_down(&self, key: u32, flags: u32) -> bool {
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        if !self
            .base
            .doc_view()
            .is_some_and(|host| host.is_interactive())
        {
            return false;
        }
        if is_platform_shortcut_key(flags)
            && matches!(key, VKEY_A | VKEY_C | VKEY_V | VKEY_X | VKEY_Z)
        {
            return false;
        }
        self.send_message_to_fwl_widget(&mut FwlMessage::key(
            Some(widget),
            KeyCommand::KeyDown,
            flags,
            key,
        ))
    }
    pub fn on_char(&self, ch: u32, flags: u32) -> bool {
        if !self
            .base
            .doc_view()
            .is_some_and(|host| host.is_interactive())
        {
            return false;
        }
        if ch == 9 {
            return true;
        }
        let Some(widget) = self.normal_widget() else {
            return false;
        };
        if !self.base.node().is_open_access() {
            return false;
        }
        self.send_message_to_fwl_widget(&mut FwlMessage::key(
            Some(widget),
            KeyCommand::Char,
            flags,
            ch,
        ))
    }
    pub fn hit_test(&self, point: CfxPointF) -> FwlWidgetHit {
        if self.normal_widget().is_some_and(|widget| {
            widget.hit_test(self.fwl_to_client(point)) != FwlWidgetHit::Unknown
        }) {
            return FwlWidgetHit::Client;
        }
        if !self.base.get_rect_without_rotate().contains_point(point) {
            return FwlWidgetHit::Unknown;
        }
        if self.caption_rect.get().contains_point(point) {
            FwlWidgetHit::Titlebar
        } else {
            FwlWidgetHit::Border
        }
    }
    pub fn layout_caption(&self) -> FfResult<()> {
        if let Some(layout) = self
            .base
            .doc_view()
            .and_then(|host| host.caption_layout(self.base.node()))
        {
            let mut rect = self.caption_rect.get();
            let height = layout
                .with_layout(|layout| layout.layout(CfxSizeF::new(rect.width, rect.height)))?;
            rect.height = rect.height.max(height);
            self.caption_rect.set(rect);
        }
        Ok(())
    }
    pub fn render_caption(
        &self,
        graphics: &mut CfgasGeGraphics<'_, '_, '_>,
        matrix: CfxMatrix,
    ) -> FfResult<bool> {
        let Some(layout) = self
            .base
            .doc_view()
            .and_then(|host| host.caption_layout(self.base.node()))
        else {
            return Ok(true);
        };
        if !self
            .base
            .node()
            .get_caption_if_exists()
            .is_some_and(|caption| caption.caption_is_visible())
        {
            return Ok(true);
        }
        let rect = self.caption_rect.get();
        if !layout.with_layout(|layout| Ok(layout.is_loaded()))? {
            layout.with_layout(|layout| layout.layout(CfxSizeF::new(rect.width, rect.height)))?;
        }
        let mut clip = rect;
        clip.intersect(self.base.get_rect_without_rotate());
        clip = matrix.transform_rect(clip);
        let mut translated = CfxMatrix::new(1., 0., 0., 1., rect.left, rect.top);
        translated.concat(matrix);
        let rendered = layout.with_layout(|layout| {
            layout.draw_string(graphics.get_render_device(), translated, clip)
        })?;
        Ok(rendered)
    }
    pub fn process_committed_data(&self, owner: &dyn FfWidget) -> FfResult<bool> {
        if !self.base.node().is_open_access() || !owner.is_data_changed() {
            return Ok(false);
        }
        let host = self.base.require_doc_view()?;
        host.set_change_mark();
        host.add_validate_node(self.base.node());
        if self.calculate_override()? != 1 {
            return Ok(false);
        }
        Ok(owner.commit_data())
    }
    pub fn calculate_override(&self) -> FfResult<i32> {
        let node = self.base.node();
        let Some(group) = node
            .get_excl_group_if_exists()
            .filter(|node| node.is_widget_ready())
        else {
            return self.calculate_node(node);
        };
        if self.calculate_node(&group)? == 0 {
            return Ok(0);
        }
        let mut member = group.get_excl_group_first_member();
        while let Some(node) = member {
            if !node.is_widget_ready() {
                return Ok(1);
            }
            if self.calculate_node(&node)? == 0 {
                return Ok(0);
            }
            member = group.get_excl_group_next_member(Some(&node));
        }
        Ok(1)
    }
    pub fn calculate_node(&self, node: &CxfaNode) -> FfResult<i32> {
        let Some(calculate) = node.get_calculate_if_exists() else {
            return Ok(1);
        };
        let version = self
            .base
            .node()
            .document()
            .map_or(0, |doc| doc.get_cur_version_mode());
        let host = self.base.require_doc_view()?;
        let wide = |text: &str| text.chars().map(u32::from).collect::<Vec<_>>();
        match calculate.calculate_get_override() {
            V::Error => {
                if version <= 204 {
                    return Ok(1);
                }
                host.message_box(
                    &wide("You are not allowed to modify this field."),
                    &wide("Calculate Override"),
                    AlertIcon::kWarning,
                    AlertButton::kOK,
                );
                Ok(0)
            }
            V::Warning => {
                if version <= 204
                    && calculate
                        .calculate_get_script_if_exists()
                        .is_none_or(|script| script.script_get_expression().is_empty())
                {
                    return Ok(1);
                }
                if node.is_user_interactive() {
                    return Ok(1);
                }
                let mut message = calculate.calculate_get_message_text();
                if !message.is_empty() {
                    message.extend([13, 10]);
                }
                message.extend(wide("Are you sure you want to modify this field?"));
                if host.message_box(
                    &message,
                    &wide("Calculate Override"),
                    AlertIcon::kWarning,
                    AlertButton::kYesNo,
                ) == Some(AlertReturn::kYes)
                {
                    node.set_flag(XfaNodeFlag::UserInteractive);
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            V::Ignore => Ok(0),
            V::Disabled => {
                node.set_flag(XfaNodeFlag::UserInteractive);
                Ok(1)
            }
            _ => Ok(1),
        }
    }
    pub fn on_process_event(&self, event: &mut FwlEvent) {
        let (kind, activity) = match event.data {
            EventData::Mouse(MouseCommand::Enter) => (XfaEventType::MouseEnter, V::MouseEnter),
            EventData::Mouse(MouseCommand::Leave) => (XfaEventType::MouseExit, V::MouseExit),
            EventData::Mouse(MouseCommand::LeftButtonDown) => {
                (XfaEventType::MouseDown, V::MouseDown)
            }
            EventData::Mouse(MouseCommand::LeftButtonUp) => (XfaEventType::MouseUp, V::MouseUp),
            EventData::Click => (XfaEventType::Click, V::Click),
            _ => return,
        };
        if let Some(host) = self.base.doc_view() {
            host.process_event(self.base.node(), activity, &mut CxfaEventParam::new(kind));
        }
    }
}
impl LayoutItemWidget for CxfaFfField {
    fn set_layout_item(&self, item: Option<CxfaContentLayoutItem>) {
        self.base.set_layout_item(item);
    }
    fn as_ff_widget(&self) -> Option<&dyn FfWidget> {
        Some(self)
    }
}
impl FfWidget for CxfaFfField {
    fn widget(&self) -> &CxfaFfWidget {
        &self.base
    }
    fn as_field(&self) -> Option<&CxfaFfField> {
        Some(self)
    }
}
impl WidgetAdapter for CxfaFfField {
    fn get_rotate_matrix(&self) -> CfxMatrix {
        self.base.get_rotate_matrix()
    }
    fn display_caret(&self, visible: bool, anchor: Option<CfxRectF>) {
        if let Some(host) = self.base.doc_view() {
            host.display_caret(self, visible, anchor);
        }
    }
    fn get_border_color_and_thickness(&self) -> (u32, f32) {
        self.base.get_border_color_and_thickness()
    }
}
struct FfFieldDelegate(Weak<dyn FfWidget>);
impl WidgetDelegate for FfFieldDelegate {
    fn on_process_message(&self, message: &mut FwlMessage) {
        if let Some(widget) = self.0.upgrade() {
            widget.on_process_message(message);
        }
    }
    fn on_process_event(&self, event: &mut FwlEvent) {
        if let Some(widget) = self.0.upgrade() {
            widget.on_process_event(event);
        }
    }
    fn on_draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if let Some(widget) = self.0.upgrade() {
            widget.on_draw_widget(graphics, matrix);
        }
    }
}
