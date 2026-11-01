// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cpdfxfa_docenvironment.{h,cpp}`. Optional embedding capabilities are real
//! SDK callbacks. All callback arguments/owners are acquired before dispatch;
//! no PDF, layout, annotation collection or context slot stays borrowed.
use super::{
    cpdfxfa_context::{xfa_object, CpdfXfaContext, LoadStatus},
    cpdfxfa_page::CpdfXfaPage,
};
use crate::{
    fpdfapi::parser::{
        cpdf_array::CpdfArray, cpdf_dictionary::CpdfDictionary, cpdf_object::retain_object,
        cpdf_scalar_object::ObjectType, cpdf_stream_acc::CpdfRetainedStreamAcc,
        cpdf_string::CpdfString,
    },
    fpdfdoc::cpdf_formfield::FormFieldType,
    fpdfsdk::{cpdfsdk_formfillenvironment::CpdfSdkFormFillEnvironment, cpdfsdk_pageview::SdkPage},
    fxcrt::{
        fx_coordinates::{CfxFloatRect, CfxPointF, CfxRectF},
        fx_stream::{SeekableReadStream, SeekableWriteStream, WriteStream},
        xml::cfx_xmldocument::CfxXmlDocument,
    },
    xfa::fxfa::{
        cxfa_ffdoc::{CxfaFfDoc, PageViewEvent, XfaDocumentEnvironment},
        cxfa_ffwidget::FfWidget,
        fxfa::XfaPrintOpt,
        fxfa_basic::XfaHashCode,
        layout::{cxfa_layoutitem::LayoutItemWidget, cxfa_viewlayoutitem::CxfaViewLayoutItem},
    },
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};
/// Extra FPDF_FORMFILLINFO XFA callbacks. None/false means the function pointer
/// was absent, matching the SDK optional-service boundary. File handlers own
/// real streams; the library never substitutes memory-only export success.
pub trait XfaSdkHost {
    fn app_name(&self) -> Vec<u32> {
        Vec::new()
    }
    fn download_url(&self, _url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        None
    }
    fn post_request_url(
        &self,
        _url: &[u32],
        _data: &[u32],
        _content_type: &[u32],
        _encoding: &[u32],
        _header: &[u32],
    ) -> Option<Vec<u32>> {
        None
    }
    fn put_request_url(&self, _url: &[u32], _data: &[u32], _encoding: &[u32]) -> bool {
        false
    }
    fn display_caret(&self, _page: &SdkPage, _visible: bool, _rect: CfxFloatRect) {}
    fn page_view_rect(&self, _page: &SdkPage) -> Option<CfxFloatRect> {
        None
    }
    fn popup_menu(&self, _page: &SdkPage, _flags: u32, _point: CfxPointF) -> bool {
        false
    }
    fn page_event(&self, _count: i32, _event: PageViewEvent) {}
    fn current_page_index(&self) -> i32 {
        -1
    }
    fn set_current_page(&self, _index: i32) {}
    fn goto_url(&self, _url: &[u32]) {}
    fn open_file_read(
        &self,
        _kind: i32,
        _path: &[u32],
        _mode: &str,
    ) -> Option<Rc<dyn SeekableReadStream>> {
        None
    }
    fn open_file_write(
        &self,
        _kind: i32,
        _path: &[u32],
        _mode: &str,
    ) -> Option<Box<dyn SeekableWriteStream>> {
        None
    }
}
pub struct CpdfXfaDocEnvironment {
    context: Weak<CpdfXfaContext>,
}
impl CpdfXfaDocEnvironment {
    pub fn new(context: Weak<CpdfXfaContext>) -> Self {
        Self { context }
    }
    fn context(&self) -> Option<Rc<CpdfXfaContext>> {
        self.context.upgrade()
    }
    fn matches(&self, doc: &CxfaFfDoc) -> Option<Rc<CpdfXfaContext>> {
        self.context().filter(|ctx| {
            ctx.xfa_doc()
                .is_some_and(|active| std::ptr::eq(active.as_ref(), doc))
        })
    }
    fn env_for(&self, doc: &CxfaFfDoc) -> Option<Rc<CpdfSdkFormFillEnvironment>> {
        self.matches(doc)?.form_fill_env()
    }
    fn xfa_host(&self) -> Option<Rc<dyn XfaSdkHost>> {
        self.context()?.form_fill_env()?.host()?.xfa_host()
    }
    fn page(
        &self,
        item: &CxfaViewLayoutItem,
    ) -> Option<(Rc<CpdfXfaContext>, Rc<CpdfXfaPage>, Rc<SdkPage>)> {
        let ctx = self.context()?;
        let page = ctx.get_xfa_page_for_ffview(item)?;
        let sdk = ctx.sdk_page(&page)?;
        Some((ctx, page, sdk))
    }
}
impl XfaDocumentEnvironment for CpdfXfaDocEnvironment {
    fn set_change_mark(&self, doc: &CxfaFfDoc) {
        if let Some(env) = self.env_for(doc) {
            env.set_change_mark();
        }
    }
    fn invalidate_rect(&self, page: Option<&CxfaViewLayoutItem>, rect: CfxRectF) {
        let Some((ctx, _, sdk)) = page.and_then(|item| self.page(item)) else {
            return;
        };
        if let Some(env) = ctx.form_fill_env() {
            env.invalidate_page(&sdk, rect.to_float_rect().to_fx_rect());
        }
    }
    fn display_caret(&self, widget: &dyn FfWidget, visible: bool, anchor: Option<CfxRectF>) {
        let Some(anchor) = anchor else {
            return;
        };
        let Some((_, _, sdk)) = widget
            .widget()
            .page_view()
            .and_then(|item| self.page(&item))
        else {
            return;
        };
        if let Some(host) = self.xfa_host() {
            host.display_caret(&sdk, visible, anchor.to_float_rect());
        }
    }
    fn get_popup_pos(
        &self,
        widget: &dyn FfWidget,
        min: f32,
        max: f32,
        anchor: CfxRectF,
        popup: &mut CfxRectF,
    ) -> bool {
        let Some((_, _, sdk)) = widget
            .widget()
            .page_view()
            .and_then(|item| self.page(&item))
        else {
            return false;
        };
        let Some(rect) = self.xfa_host().and_then(|host| host.page_view_rect(&sdk)) else {
            return false;
        };
        popup_position(
            rect,
            widget.widget().node().get_rotate(),
            min,
            max,
            anchor,
            popup,
        )
    }
    fn popup_menu(&self, widget: &dyn FfWidget, point: CfxPointF) -> bool {
        let Some((_, _, sdk)) = widget
            .widget()
            .page_view()
            .and_then(|item| self.page(&item))
        else {
            return false;
        };
        let flags = (if widget.can_undo() { 8 } else { 0 })
            | (if widget.can_redo() { 16 } else { 0 })
            | (if widget.can_paste() { 32 } else { 0 })
            | (if widget.can_copy() { 1 } else { 0 })
            | (if widget.can_cut() { 2 } else { 0 })
            | (if widget.can_select_all() { 4 } else { 0 });
        self.xfa_host()
            .is_some_and(|host| host.popup_menu(&sdk, flags, point))
    }
    fn on_page_view_event(&self, _page: Option<&CxfaViewLayoutItem>, event: PageViewEvent) {
        let Some(ctx) = self.context() else {
            return;
        };
        if matches!(ctx.load_status(), LoadStatus::Loading | LoadStatus::Closing)
            || event != PageViewEvent::StopLayout
        {
            return;
        }
        let Some(env) = ctx.form_fill_env() else {
            return;
        };
        let count = ctx.get_page_count();
        let old = ctx.original_page_count();
        if count == old || ctx.doc_view().is_none() {
            return;
        }
        for index in 0..old {
            if let Some(page) = ctx.get_xfa_page(index) {
                if let Some(sdk) = ctx.sdk_page(&page) {
                    env.remove_page_view(&sdk);
                }
                page.set_xfa_page_view_index(index);
                if ctx
                    .form_fill_env()
                    .as_ref()
                    .is_none_or(|active| !Rc::ptr_eq(active, &env))
                {
                    return;
                }
            }
        }
        ctx.set_original_page_count(count);
        if let Some(host) = self.xfa_host() {
            host.page_event(
                (count - old).abs(),
                if count < old {
                    PageViewEvent::PostRemoved
                } else {
                    PageViewEvent::PostAdded
                },
            );
        }
    }
    fn widget_post_add(&self, widget: &dyn FfWidget) {
        let Some((ctx, _, sdk)) = widget
            .widget()
            .page_view()
            .and_then(|item| self.page(&item))
        else {
            return;
        };
        let Some(owner) = widget.widget().layout_item().and_then(|item| item.widget()) else {
            return;
        };
        if let Some(view) = ctx
            .form_fill_env()
            .and_then(|env| env.get_or_create_page_view(&sdk))
        {
            view.add_annot_for_ff_widget(&owner);
        }
    }
    fn widget_pre_remove(&self, widget: &dyn FfWidget) {
        let Some((ctx, _, sdk)) = widget
            .widget()
            .page_view()
            .and_then(|item| self.page(&item))
        else {
            return;
        };
        if let Some(view) = ctx.form_fill_env().and_then(|env| env.get_page_view(&sdk)) {
            view.delete_annot_for_ff_widget(widget);
        }
    }
    fn count_pages(&self, doc: &CxfaFfDoc) -> i32 {
        self.matches(doc)
            .filter(|ctx| ctx.form_fill_env().is_some())
            .map_or(0, |ctx| ctx.get_page_count())
    }
    fn get_current_page(&self, doc: &CxfaFfDoc) -> i32 {
        if self
            .matches(doc)
            .is_none_or(|ctx| !ctx.contains_extension_full_form())
        {
            return -1;
        }
        self.xfa_host().map_or(-1, |host| host.current_page_index())
    }
    fn set_current_page(&self, doc: &CxfaFfDoc, page: i32) {
        let Some(ctx) = self.matches(doc) else {
            return;
        };
        if !ctx.contains_extension_form() || page < 0 || page >= ctx.get_page_count() {
            return;
        }
        if let Some(host) = self.xfa_host() {
            host.set_current_page(page);
        }
    }
    fn is_calculations_enabled(&self, doc: &CxfaFfDoc) -> bool {
        self.env_for(doc)
            .is_some_and(|env| env.interactive_form().is_xfa_calculate_enabled())
    }
    fn set_calculations_enabled(&self, doc: &CxfaFfDoc, enabled: bool) {
        if let Some(env) = self.env_for(doc) {
            env.interactive_form().xfa_enable_calculate(enabled);
        }
    }
    fn get_title(&self, doc: &CxfaFfDoc) -> Vec<u32> {
        let Some(pdf) = self.matches(doc).and_then(|ctx| ctx.pdf_document()) else {
            return Vec::new();
        };
        let Some(info) = pdf.borrow_mut().info() else {
            return Vec::new();
        };
        let value = info
            .borrow()
            .as_any()
            .downcast_ref::<CpdfDictionary>()
            .map_or_else(Vec::new, |dict| dict.get_unicode_text_for(b"Title"));
        value
    }
    fn set_title(&self, doc: &CxfaFfDoc, title: &[u32]) {
        let Some(pdf) = self.matches(doc).and_then(|ctx| ctx.pdf_document()) else {
            return;
        };
        let Some(info) = pdf.borrow_mut().info() else {
            return;
        };
        if let Some(dict) = info
            .borrow_mut()
            .as_any_mut()
            .downcast_mut::<CpdfDictionary>()
        {
            dict.set_for(
                b"Title",
                Some(retain_object(CpdfString::from_unicode(None, title))),
            );
        };
    }
    fn export_data(&self, doc: &CxfaFfDoc, path: &[u32], xdp: bool) {
        let Some(ctx) = self
            .matches(doc)
            .filter(|ctx| ctx.contains_extension_form())
        else {
            return;
        };
        let Some(env) = ctx.form_fill_env() else {
            return;
        };
        let path = if path.is_empty() {
            let Some(host) = env.host().filter(|host| host.javascript_enabled()) else {
                return;
            };
            host.js_field_browse()
        } else {
            path.to_vec()
        };
        let Some(host) = env.host().and_then(|host| host.xfa_host()) else {
            return;
        };
        let Some(mut stream) = host.open_file_write(if xdp { 2 } else { 1 }, &path, "wb") else {
            return;
        };
        if xdp {
            if !export_xdp(&ctx, doc, &path, stream.as_mut()) {
                return;
            }
        } else {
            if !stream.write_string(b"<?xml version=\"1.0\" encoding=\"UTF-8\"?>\r\n") {
                return;
            }
            let node = doc.model_document().get_xfa_node(XfaHashCode::Data as u32);
            if !doc.save_package(node.as_ref(), stream.as_mut()) {
                return;
            }
        }
        stream.flush();
    }
    fn goto_url(&self, doc: &CxfaFfDoc, url: &[u32]) {
        if self
            .matches(doc)
            .is_some_and(|ctx| ctx.contains_extension_full_form())
        {
            if let Some(host) = self.xfa_host() {
                host.goto_url(url);
            }
        }
    }
    fn is_validations_enabled(&self, doc: &CxfaFfDoc) -> bool {
        self.env_for(doc)
            .is_some_and(|env| env.interactive_form().is_xfa_validations_enabled())
    }
    fn set_validations_enabled(&self, doc: &CxfaFfDoc, enabled: bool) {
        if let Some(env) = self.env_for(doc) {
            env.interactive_form().xfa_set_validations_enabled(enabled);
        }
    }
    fn set_focus_widget(&self, doc: &CxfaFfDoc, owner: Option<Rc<dyn LayoutItemWidget>>) {
        let Some(env) = self.env_for(doc) else {
            return;
        };
        let Some(owner) = owner else {
            env.kill_focus_annot(0);
            return;
        };
        let Some(widget) = owner.as_ff_widget() else {
            return;
        };
        for view in env.page_views() {
            if let Some(annot) = view.get_annot_for_ff_widget(widget) {
                env.set_focus_annot(&annot);
                break;
            }
        }
    }
    fn print(&self, doc: &CxfaFfDoc, start: i32, end: i32, options: XfaPrintOpt) {
        if let Some(host) = self
            .env_for(doc)
            .and_then(|env| env.host())
            .filter(|host| host.javascript_enabled())
        {
            host.js_doc_print(
                options.contains(XfaPrintOpt::kShowDialog),
                start,
                end,
                options.contains(XfaPrintOpt::kCanCancel),
                options.contains(XfaPrintOpt::kShrinkPage),
                options.contains(XfaPrintOpt::kAsImage),
                options.contains(XfaPrintOpt::kReverseOrder),
                options.contains(XfaPrintOpt::kPrintAnnot),
            );
        }
    }
    fn get_highlight_color(&self, doc: &CxfaFfDoc) -> u32 {
        let Some(env) = self.env_for(doc) else {
            return 0;
        };
        let form = env.interactive_form();
        let color = form.highlight_color(FormFieldType::Xfa);
        u32::from(form.highlight_alpha()) << 24
            | (color & 255) << 16
            | (color & 0xff00)
            | (color >> 16 & 255)
    }
    fn xml_document(&self) -> Option<Rc<RefCell<CfxXmlDocument>>> {
        self.context()?.xml_document()
    }
    fn open_linked_file(&self, doc: &CxfaFfDoc, url: &[u32]) -> Option<Rc<dyn SeekableReadStream>> {
        self.matches(doc)?;
        self.xfa_host()?.open_file_read(0, url, "rb")
    }
}
/// Native rotated popup-space calculation. Reject nonfinite dimensions before
/// numeric conversion; otherwise retain its integer available-space rounding.
pub fn popup_position(
    view: CfxFloatRect,
    rotation: i32,
    min: f32,
    max: f32,
    anchor: CfxRectF,
    popup: &mut CfxRectF,
) -> bool {
    if [
        view.left,
        view.top,
        view.right,
        view.bottom,
        min,
        max,
        anchor.left,
        anchor.top,
        anchor.width,
        anchor.height,
        popup.left,
    ]
    .iter()
    .any(|v| !v.is_finite())
        || min < 0.
        || max < min
    {
        return false;
    }
    let (below, above) = match rotation {
        90 => {
            if anchor.bottom() > view.bottom {
                popup.left += anchor.bottom() - view.bottom;
            }
            if anchor.top < view.top {
                popup.left -= view.top - anchor.top;
            }
            (
                (view.right - anchor.right()) as i32,
                (anchor.left - view.left) as i32,
            )
        }
        180 => {
            if anchor.right() > view.right {
                popup.left += anchor.right() - view.right;
            }
            if anchor.left < view.left {
                popup.left -= view.left - anchor.left;
            }
            (
                (anchor.top - view.top) as i32,
                (view.bottom - anchor.bottom()) as i32,
            )
        }
        270 => {
            if anchor.top < view.top {
                popup.left += view.top - anchor.top;
            }
            if anchor.bottom() > view.bottom {
                popup.left -= anchor.bottom() - view.bottom;
            }
            (
                (anchor.left - view.left) as i32,
                (view.right - anchor.right()) as i32,
            )
        }
        _ => {
            if anchor.left < view.left {
                popup.left += view.left - anchor.left;
            }
            if anchor.right() > view.right {
                popup.left -= anchor.right() - view.right;
            }
            (
                (view.bottom - anchor.bottom()) as i32,
                (anchor.top - view.top) as i32,
            )
        }
    };
    if below <= 0 && above <= 0 {
        return false;
    }
    let draw_below = below > 0 && (above <= 0 || below > above);
    let height = (if draw_below { below } else { above }) as f32;
    let height = height.max(min).min(max);
    popup.top = if draw_below {
        if matches!(rotation, 90 | 270) {
            anchor.width
        } else {
            anchor.height
        }
    } else {
        -height
    };
    popup.height = height;
    true
}
fn export_xdp(
    ctx: &CpdfXfaContext,
    doc: &CxfaFfDoc,
    path: &[u32],
    stream: &mut dyn WriteStream,
) -> bool {
    let Some(pdf) = ctx.pdf_document() else {
        return false;
    };
    let Some(xfa) = xfa_object(&pdf.borrow()) else {
        return false;
    };
    let packets = {
        let object = xfa.borrow();
        let Some(array) = object.as_any().downcast_ref::<CpdfArray>() else {
            return false;
        };
        (1..array.size())
            .step_by(2)
            .filter_map(|i| {
                let name = array.get_object_at(i - 1)?;
                if name.borrow().object_type() != ObjectType::String {
                    return None;
                }
                let raw = array.get_object_at(i)?;
                if raw.borrow().object_type() != ObjectType::Reference {
                    return None;
                }
                let name = name.borrow().get_string();
                Some((name, array.get_stream_at(i)?, i == array.size() - 1))
            })
            .collect::<Vec<_>>()
    };
    for (name, packet, last) in packets {
        let code = match name.as_slice() {
            b"form" => Some(XfaHashCode::Form),
            b"datasets" => Some(XfaHashCode::Datasets),
            _ => None,
        };
        if let Some(code) = code {
            let node = doc.model_document().get_xfa_node(code as u32);
            if !doc.save_package(node.as_ref(), stream) {
                return false;
            }
            continue;
        }
        if last {
            let path: String = path.iter().filter_map(|&ch| char::from_u32(ch)).collect();
            let path = path
                .replace('&', "&amp;")
                .replace('"', "&quot;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            if !stream.write_string(
                format!("\n<pdf href=\"{path}\" xmlns=\"http://ns.adobe.com/xdp/pdf/\"/>")
                    .as_bytes(),
            ) {
                return false;
            }
        }
        let Some(acc) = CpdfRetainedStreamAcc::load_filtered(packet) else {
            return false;
        };
        if !stream.write_block(&acc.get_span()) {
            return false;
        }
    }
    true
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn popup_rotations_choose_larger_side_and_preserve_minimum_height() {
        let view = CfxFloatRect::new(0., 200., 300., 0.);
        let anchor = CfxRectF::new(20., 30., 40., 10.);
        for (rotation, top, height) in [
            (0, 10., 80.),
            (90, 40., 80.),
            (180, -80., 80.),
            (270, -80., 80.),
        ] {
            let mut popup = CfxRectF::new(0., 0., 40., 0.);
            assert!(popup_position(view, rotation, 10., 80., anchor, &mut popup));
            assert_eq!((popup.top, popup.height), (top, height));
        }
        let mut popup = CfxRectF::default();
        assert!(popup_position(
            CfxFloatRect::new(0., 45., 100., 0.),
            0,
            100.,
            150.,
            anchor,
            &mut popup
        ));
        assert_eq!(popup.height, 100.);
    }
    #[test]
    fn popup_no_space_and_nonfinite_inputs_are_rejected() {
        let anchor = CfxRectF::new(0., 0., 10., 10.);
        let mut popup = CfxRectF::default();
        assert!(!popup_position(
            CfxFloatRect::new(0., 0., 10., 10.),
            0,
            10.,
            20.,
            anchor,
            &mut popup
        ));
        assert!(!popup_position(
            CfxFloatRect::default(),
            0,
            f32::NAN,
            20.,
            anchor,
            &mut popup
        ));
    }
}
