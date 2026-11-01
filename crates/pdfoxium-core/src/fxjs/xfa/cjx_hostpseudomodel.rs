// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Complete `cjx_hostpseudomodel.{h,cpp}` and non-node host object facade.
//! All UI/document effects use a real optional owner; missing service preserves
//! native absent-notify behavior. Host calls lease the existing Boa context.
use super::{
    cfxjse_engine::{resolve_error, CfxJseEngine},
    cfxjse_resolveprocessor::XfaResolveFlags,
    cjx_subform::{parameter_error, string, to_wide},
};
use crate::{
    fxcrt::fx_extension::is_wide_space,
    fxjs::cfxjs_engine::CfxJsEngine,
    xfa::fxfa::{fxfa::XfaPrintOpt, parser::cxfa_node::CxfaNode},
};
use boa_engine::{
    object::{FunctionObjectBuilder, ObjectInitializer},
    property::PropertyDescriptor,
    Context, JsData, JsNativeError, JsObject, JsResult, JsString, JsValue, NativeFunction,
};
use boa_gc::{Finalize, Trace};
use std::rc::{Rc, Weak};
/// Actual embedding services used by xfa.host. No default UI, navigation,
/// exporter, event scheduler or substitute document is installed by the model.
pub trait XfaHost {
    fn calculations_enabled(&self) -> JsResult<bool>;
    fn set_calculations_enabled(&self, value: bool) -> JsResult<()>;
    fn validations_enabled(&self) -> JsResult<bool>;
    fn set_validations_enabled(&self, value: bool) -> JsResult<()>;
    fn current_page(&self) -> JsResult<i32>;
    fn set_current_page(&self, page: i32) -> JsResult<()>;
    fn num_pages(&self) -> JsResult<i32>;
    fn language(&self) -> JsResult<Vec<u32>>;
    fn platform(&self) -> JsResult<Vec<u32>>;
    fn app_name(&self) -> JsResult<Vec<u32>>;
    fn title(&self) -> JsResult<Vec<u32>>;
    fn set_title(&self, title: &[u32]) -> JsResult<()>;
    fn goto_url(&self, url: &[u32]) -> JsResult<()>;
    fn open_list(&self, node: &CxfaNode) -> JsResult<()>;
    fn get_focus(&self) -> JsResult<Option<CxfaNode>>;
    fn set_focus(&self, node: Option<&CxfaNode>) -> JsResult<()>;
    fn reset_data(&self, node: Option<&CxfaNode>) -> JsResult<()>;
    fn beep(&self, kind: u32) -> JsResult<()>;
    fn message_box(&self, message: &[u32], title: &[u32], icon: u32, buttons: u32)
        -> JsResult<i32>;
    fn response(
        &self,
        question: &[u32],
        title: &[u32],
        default_answer: &[u32],
        masked: bool,
    ) -> JsResult<Vec<u32>>;
    fn print(&self, start: i32, end: i32, options: XfaPrintOpt) -> JsResult<()>;
    fn export_data(&self, file_path: &[u32], xdp: bool) -> JsResult<()>;
}
#[derive(Trace, Finalize, JsData)]
struct HostBinding {
    #[unsafe_ignore_trace]
    runtime: Weak<CfxJseEngine>,
}
fn runtime(this: &JsValue, inherited: bool) -> JsResult<Rc<CfxJseEngine>> {
    let mut object = this.as_object();
    while let Some(candidate) = object {
        if let Some(link) = candidate
            .downcast_ref::<HostBinding>()
            .map(|data| data.runtime.clone())
        {
            return link.upgrade().ok_or_else(|| {
                JsNativeError::reference()
                    .with_message("XFA host script context has expired")
                    .into()
            });
        }
        if !inherited {
            break;
        }
        object = candidate.prototype();
    }
    Err(JsNativeError::typ()
        .with_message("Invalid XFA host receiver")
        .into())
}
#[derive(Clone, Copy)]
enum Property {
    AppType,
    CalculationsEnabled,
    CurrentPage,
    Language,
    NumPages,
    Platform,
    Title,
    ValidationsEnabled,
    Variation,
    Version,
    Name,
}
#[derive(Clone, Copy)]
enum Method {
    Beep,
    DocumentCountInBatch,
    DocumentInBatch,
    ExportData,
    GetFocus,
    GotoUrl,
    ImportData,
    MessageBox,
    OpenList,
    PageDown,
    PageUp,
    Print,
    ResetData,
    Response,
    SetFocus,
}
impl Method {
    fn name(self) -> &'static str {
        match self {
            Self::Beep => "beep",
            Self::DocumentCountInBatch => "documentCountInBatch",
            Self::DocumentInBatch => "documentInBatch",
            Self::ExportData => "exportData",
            Self::GetFocus => "getFocus",
            Self::GotoUrl => "gotoURL",
            Self::ImportData => "importData",
            Self::MessageBox => "messageBox",
            Self::OpenList => "openList",
            Self::PageDown => "pageDown",
            Self::PageUp => "pageUp",
            Self::Print => "print",
            Self::ResetData => "resetData",
            Self::Response => "response",
            Self::SetFocus => "setFocus",
        }
    }
    fn client_only(self) -> bool {
        matches!(
            self,
            Self::Beep
                | Self::GotoUrl
                | Self::MessageBox
                | Self::OpenList
                | Self::Print
                | Self::SetFocus
        )
    }
    fn valid_arity(self, count: usize) -> bool {
        match self {
            Self::Beep | Self::ResetData => count <= 1,
            Self::GotoUrl | Self::OpenList | Self::SetFocus | Self::ImportData => count == 1,
            Self::ExportData => (1..=2).contains(&count),
            Self::MessageBox | Self::Response => (1..=4).contains(&count),
            Self::Print => count == 8,
            _ => true,
        }
    }
}
pub(crate) fn host_object(runtime: &Rc<CfxJseEngine>, context: &mut Context) -> JsResult<JsObject> {
    let object = ObjectInitializer::with_native_data(
        HostBinding {
            runtime: Rc::downgrade(runtime),
        },
        context,
    )
    .build();
    for (name, field) in [
        ("appType", Property::AppType),
        ("calculationsEnabled", Property::CalculationsEnabled),
        ("currentPage", Property::CurrentPage),
        ("language", Property::Language),
        ("numPages", Property::NumPages),
        ("platform", Property::Platform),
        ("title", Property::Title),
        ("validationsEnabled", Property::ValidationsEnabled),
        ("variation", Property::Variation),
        ("version", Property::Version),
        ("name", Property::Name),
    ] {
        let getter = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure(move |this, _, context| {
                property(this, &[], context, field, false)
            }),
        )
        .name(JsString::from(format!("get {name}")))
        .constructor(false)
        .build();
        let setter = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure(move |this, args, context| {
                property(this, args, context, field, true)
            }),
        )
        .name(JsString::from(format!("set {name}")))
        .length(1)
        .constructor(false)
        .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .get(getter)
                .set(setter)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    for method in [
        Method::Beep,
        Method::DocumentCountInBatch,
        Method::DocumentInBatch,
        Method::ExportData,
        Method::GetFocus,
        Method::GotoUrl,
        Method::ImportData,
        Method::MessageBox,
        Method::OpenList,
        Method::PageDown,
        Method::PageUp,
        Method::Print,
        Method::ResetData,
        Method::Response,
        Method::SetFocus,
    ] {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            NativeFunction::from_copy_closure(move |this, args, context| {
                call(this, args, context, method)
            }),
        )
        .name(JsString::from(method.name()))
        .constructor(false)
        .build();
        object.define_property_or_throw(
            JsString::from(method.name()),
            PropertyDescriptor::builder()
                .value(function)
                .writable(false)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    // Base CJX_Object className is a real readonly non-node type name.
    let getter =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(class_name))
            .name(JsString::from("get className"))
            .constructor(false)
            .build();
    let setter =
        FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_class_name))
            .name(JsString::from("set className"))
            .constructor(false)
            .build();
    object.define_property_or_throw(
        JsString::from("className"),
        PropertyDescriptor::builder()
            .get(getter)
            .set(setter)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn class_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this, true)?;
    Ok(JsString::from("hostPseudoModel").into())
}
fn set_class_name(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this, true)?;
    Err(invalid_property())
}
fn invalid_property() -> boa_engine::JsError {
    JsNativeError::typ()
        .with_message("Invalid property set operation.")
        .into()
}
fn readonly(name: &str) -> boa_engine::JsError {
    JsNativeError::typ()
        .with_message(format!("Unable to set {name} value."))
        .into()
}
fn property(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    field: Property,
    setting: bool,
) -> JsResult<JsValue> {
    let runtime = runtime(this, true)?;
    if matches!(field, Property::Title | Property::Variation) && !runtime.is_run_at_client() {
        return Ok(JsValue::undefined());
    }
    match field {
        Property::Version => {
            return if setting {
                Err(readonly("version"))
            } else {
                Ok(JsString::from("11").into())
            }
        }
        Property::Variation => {
            return if setting {
                Err(readonly("variation"))
            } else {
                Ok(JsString::from("Full").into())
            }
        }
        _ => {}
    }
    let Some(host) = runtime.host() else {
        return Ok(JsValue::undefined());
    };
    if setting {
        let value = args.first().cloned().unwrap_or_default();
        match field {
            Property::AppType | Property::Name => return Err(invalid_property()),
            Property::Language => return Err(readonly("language")),
            Property::NumPages => return Err(readonly("numPages")),
            Property::Platform => return Err(readonly("platform")),
            Property::CalculationsEnabled => {
                let value = value.to_boolean();
                CfxJsEngine::with_host_callback(context, || host.set_calculations_enabled(value))?;
            }
            Property::ValidationsEnabled => {
                let value = value.to_boolean();
                CfxJsEngine::with_host_callback(context, || host.set_validations_enabled(value))?;
            }
            Property::CurrentPage => {
                let value = value.to_i32(context)?;
                CfxJsEngine::with_host_callback(context, || host.set_current_page(value))?;
            }
            Property::Title => {
                let value = to_wide(&value, context)?;
                CfxJsEngine::with_host_callback(context, || host.set_title(&value))?;
            }
            _ => unreachable!(),
        }
        return Ok(JsValue::undefined());
    }
    CfxJsEngine::with_host_callback(context, || match field {
        Property::AppType => Ok(JsString::from("Exchange").into()),
        Property::CalculationsEnabled => Ok(host.calculations_enabled()?.into()),
        Property::ValidationsEnabled => Ok(host.validations_enabled()?.into()),
        Property::CurrentPage => Ok(host.current_page()?.into()),
        Property::NumPages => Ok(host.num_pages()?.into()),
        Property::Language => Ok(string(&host.language()?)),
        Property::Platform => Ok(string(&host.platform()?)),
        Property::Name => Ok(string(&host.app_name()?)),
        Property::Title => Ok(string(&host.title()?)),
        _ => unreachable!(),
    })
}
fn resolve_node(
    runtime: &CfxJseEngine,
    expression: &JsValue,
    context: &mut Context,
) -> JsResult<Option<CxfaNode>> {
    let Some(reference) = runtime.current_this_node() else {
        return Ok(None);
    };
    let expression = to_wide(expression, context)?;
    let result = CfxJsEngine::with_host_callback(context, || {
        runtime.resolve_objects(
            Some(&reference),
            &expression,
            XfaResolveFlags::CHILDREN | XfaResolveFlags::PARENT | XfaResolveFlags::SIBLINGS,
        )
    })
    .map_err(resolve_error)?;
    Ok(result.and_then(|result| result.objects.into_iter().next()))
}
fn call(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    method: Method,
) -> JsResult<JsValue> {
    let runtime = runtime(this, false)?;
    if method.client_only() && !runtime.is_run_at_client() {
        return Ok(JsValue::undefined());
    }
    if !method.valid_arity(args.len()) {
        return Err(parameter_error("HostPseudoModel", method.name()));
    }
    if matches!(
        method,
        Method::DocumentCountInBatch | Method::DocumentInBatch
    ) {
        return Ok(0.into());
    }
    // Native ImportData is deliberately an arity-only operation. No importer
    // or success response is fabricated in the Rust port.
    if matches!(method, Method::ImportData) {
        return Ok(JsValue::undefined());
    }
    let Some(host) = runtime.host() else {
        return Ok(JsValue::undefined());
    };
    match method {
        Method::Beep => {
            let kind = if args.is_empty() {
                4
            } else {
                args[0].to_i32(context)? as u32
            };
            CfxJsEngine::with_host_callback(context, || host.beep(kind))?;
        }
        Method::GotoUrl => {
            let url = to_wide(&args[0], context)?;
            CfxJsEngine::with_host_callback(context, || host.goto_url(&url))?;
        }
        Method::ExportData => {
            let path = to_wide(&args[0], context)?;
            let xdp = args.get(1).is_none_or(JsValue::to_boolean);
            CfxJsEngine::with_host_callback(context, || host.export_data(&path, xdp))?;
        }
        Method::MessageBox => {
            let message = to_wide(&args[0], context)?;
            let title = if let Some(value) = args.get(1) {
                to_wide(value, context)?
            } else {
                Vec::new()
            };
            let icon = if let Some(value) = args.get(2) {
                value.to_i32(context)? as u32
            } else {
                0
            };
            let icon = if icon > 3 { 0 } else { icon };
            let buttons = if let Some(value) = args.get(3) {
                value.to_i32(context)? as u32
            } else {
                0
            };
            let buttons = if buttons > 3 { 0 } else { buttons };
            return CfxJsEngine::with_host_callback(context, || {
                host.message_box(&message, &title, icon, buttons)
            })
            .map(JsValue::from);
        }
        Method::Response => {
            let question = to_wide(&args[0], context)?;
            let title = if let Some(value) = args.get(1) {
                to_wide(value, context)?
            } else {
                Vec::new()
            };
            let default = if let Some(value) = args.get(2) {
                to_wide(value, context)?
            } else {
                Vec::new()
            };
            let masked = if let Some(value) = args.get(3) {
                value.to_i32(context)? != 0
            } else {
                false
            };
            return CfxJsEngine::with_host_callback(context, || {
                host.response(&question, &title, &default, masked)
            })
            .map(|value| string(&value));
        }
        Method::Print => {
            let mut options = XfaPrintOpt::default();
            for (index, flag) in [
                (0, XfaPrintOpt::kShowDialog),
                (3, XfaPrintOpt::kCanCancel),
                (4, XfaPrintOpt::kShrinkPage),
                (5, XfaPrintOpt::kAsImage),
                (6, XfaPrintOpt::kReverseOrder),
                (7, XfaPrintOpt::kPrintAnnot),
            ] {
                if args[index].to_boolean() {
                    options |= flag;
                }
            }
            let start = args[1].to_i32(context)?;
            let end = args[2].to_i32(context)?;
            CfxJsEngine::with_host_callback(context, || host.print(start, end, options))?;
        }
        Method::GetFocus => {
            let node = CfxJsEngine::with_host_callback(context, || host.get_focus())?;
            return node.map_or_else(
                || Ok(JsValue::undefined()),
                |node| {
                    runtime
                        .get_or_create_binding(&node, context)
                        .map(JsValue::from)
                },
            );
        }
        Method::OpenList | Method::SetFocus => {
            let node = if args[0].is_object() {
                runtime.node_from_value(&args[0])
            } else if args[0].is_string() {
                let Some(node) = resolve_node(&runtime, &args[0], context)? else {
                    return Ok(JsValue::undefined());
                };
                Some(node)
            } else {
                None
            };
            if matches!(method, Method::OpenList) {
                if let Some(node) = node {
                    CfxJsEngine::with_host_callback(context, || host.open_list(&node))?;
                }
            } else {
                CfxJsEngine::with_host_callback(context, || host.set_focus(node.as_ref()))?;
            }
        }
        Method::ResetData => {
            let expression = if let Some(value) = args.first() {
                to_wide(value, context)?
            } else {
                Vec::new()
            };
            if expression.is_empty() {
                CfxJsEngine::with_host_callback(context, || host.reset_data(None))?;
            } else {
                if runtime.current_this_node().is_none() {
                    return Ok(JsValue::undefined());
                }
                let mut cursor = 0;
                let mut found = false;
                while cursor < expression.len() {
                    let start = cursor;
                    while cursor < expression.len() && expression[cursor] != 44 {
                        cursor += 1;
                    }
                    let mut filter = &expression[start..cursor];
                    if cursor < expression.len() {
                        cursor += 1;
                    }
                    while filter.first().is_some_and(|&ch| is_wide_space(ch)) {
                        filter = &filter[1..];
                    }
                    while filter.last().is_some_and(|&ch| is_wide_space(ch)) {
                        filter = &filter[..filter.len() - 1];
                    }
                    if let Some(node) = resolve_node(&runtime, &string(filter), context)? {
                        found = true;
                        CfxJsEngine::with_host_callback(context, || {
                            host.reset_data(node.is_widget_ready().then_some(&node))
                        })?;
                    }
                }
                if !found {
                    CfxJsEngine::with_host_callback(context, || host.reset_data(None))?;
                }
            }
        }
        Method::PageUp => {
            let page = CfxJsEngine::with_host_callback(context, || host.current_page())?;
            if page > 1 {
                CfxJsEngine::with_host_callback(context, || host.set_current_page(page - 1))?;
            }
        }
        Method::PageDown => {
            let (page, count) = CfxJsEngine::with_host_callback(context, || {
                Ok::<_, boa_engine::JsError>((host.current_page()?, host.num_pages()?))
            })?;
            if count != 0 && page != count {
                let next = if page >= count {
                    count.saturating_sub(1)
                } else {
                    page.saturating_add(1)
                };
                CfxJsEngine::with_host_callback(context, || host.set_current_page(next))?;
            }
        }
        _ => unreachable!(),
    }
    Ok(JsValue::undefined())
}
#[cfg(test)]
#[path = "cjx_hostpseudomodel_tests.rs"]
mod tests;
