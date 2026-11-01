// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cjx_layoutpseudomodel.{h,cpp}` over actual FF layout items and fragments.
//! The in-batch zero results and relayoutPageArea no-op are native behavior.
use super::{
    cfxjse_engine::CfxJseEngine,
    cjx_list::new_list_binding,
    cjx_pseudomodel::{self, Kind},
    cjx_subform::{define, parameter_error, to_wide},
};
use crate::{
    fxjs::{cfxjs_engine::CfxJsEngine, cjs_field::binding_error, js_resources::JsMessage},
    xfa::fxfa::{
        cxfa_ffdocview::LayoutStatus,
        fxfa_basic::{XfaAttribute as A, XfaElement as E, XfaUnit},
        layout::{
            cxfa_contentlayoutitem::CxfaContentLayoutItem, cxfa_layoutitem::CxfaLayoutItem,
            cxfa_layoutprocessor::CxfaLayoutProcessor,
            cxfa_traversestrategy_layoutitem::CxfaLayoutItemIterator,
        },
        parser::{
            cxfa_arraynodelist::CxfaArrayNodeList, cxfa_measurement::CxfaMeasurement,
            cxfa_node::CxfaNode,
        },
    },
};
use boa_engine::{
    object::FunctionObjectBuilder, property::PropertyDescriptor, Context, JsNativeError, JsObject,
    JsResult, JsValue, NativeFunction,
};
use std::{collections::HashSet, rc::Rc};
/// Actual FF service; a model-only engine has no fabricated layout or bounds.
pub trait XfaLayoutHost {
    fn layout_processor(&self) -> Option<Rc<CxfaLayoutProcessor>>;
    fn layout_status(&self) -> LayoutStatus;
}
fn runtime(this: &JsValue) -> JsResult<Rc<CfxJseEngine>> {
    cjx_pseudomodel::validate(this, Kind::Layout, false)
}
fn require_layout(runtime: &CfxJseEngine) -> JsResult<Rc<CxfaLayoutProcessor>> {
    runtime
        .layout_host()
        .and_then(|host| host.layout_processor())
        .ok_or_else(|| {
            JsNativeError::reference()
                .with_message("XFA layout requires a live document view")
                .into()
        })
}
pub(crate) fn layout_object(
    runtime: &Rc<CfxJseEngine>,
    context: &mut Context,
) -> JsResult<JsObject> {
    let object = cjx_pseudomodel::object(runtime, Kind::Layout, context)?;
    define(
        &object,
        context,
        &[
            ("absPage", 1, abs_page),
            ("absPageCount", 0, all_page_count),
            ("absPageCountInBatch", 0, zero),
            ("absPageInBatch", 1, batch_page),
            ("absPageSpan", 1, page_span),
            ("h", 1, height),
            ("page", 1, page),
            ("pageContent", 1, page_content),
            ("pageCount", 0, numbered_page_count),
            ("pageSpan", 1, page_span),
            ("relayout", 0, relayout),
            ("relayoutPageArea", 0, noop),
            ("sheet", 1, abs_page),
            ("sheetCount", 0, all_page_count),
            ("sheetCountInBatch", 0, zero),
            ("sheetInBatch", 1, batch_page),
            ("w", 1, width),
            ("x", 1, x),
            ("y", 1, y),
        ],
    )?;
    let get = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(ready))
        .name("get ready")
        .constructor(false)
        .build();
    let set = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_ready))
        .name("set ready")
        .length(1)
        .constructor(false)
        .build();
    object.define_property_or_throw(
        boa_engine::js_string!("ready"),
        PropertyDescriptor::builder()
            .get(get)
            .set(set)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    crate::fxjs::boa_adapters::finish_class(&object, context)?;
    Ok(object)
}
fn ready(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    Ok(runtime
        .layout_host()
        .map_or_else(JsValue::undefined, |host| {
            (!matches!(
                host.layout_status(),
                LayoutStatus::None | LayoutStatus::Start
            ))
            .into()
        }))
}
fn set_ready(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    if runtime.layout_host().is_some() {
        return Err(JsNativeError::typ()
            .with_message("Unable to set ready value.")
            .into());
    }
    Ok(JsValue::undefined())
}
#[derive(Clone, Copy)]
enum Dimension {
    Height,
    Width,
    X,
    Y,
}
fn measure(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
    dimension: Dimension,
) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    if args.is_empty() || args.len() > 3 {
        return Err(parameter_error("LayoutPseudoModel", "h/w/x/y"));
    }
    let Some(node) = runtime.node_from_value(&args[0]) else {
        return Ok(JsValue::undefined());
    };
    let mut unit = vec![112, 116];
    if let Some(value) = args.get(1) {
        let converted = to_wide(value, context)?;
        if !converted.is_empty() {
            unit = converted;
        }
    }
    let mut index = args
        .get(2)
        .map(|value| value.to_i32(context))
        .transpose()?
        .unwrap_or(0);
    let layout = require_layout(&runtime)?;
    let Some(mut item) = layout
        .get_layout_item(&node)
        .and_then(|item| item.as_content_layout_item())
    else {
        return Ok(JsValue::undefined());
    };
    while index > 0 {
        let Some(next) = item.next() else {
            return Ok(0.into());
        };
        item = next;
        index -= 1;
    }
    let rect = item.get_relative_rect();
    let value = match dimension {
        Dimension::Height => rect.height,
        Dimension::Width => rect.width,
        Dimension::X => rect.left,
        Dimension::Y => rect.top,
    };
    let unit = CxfaMeasurement::get_unit_from_string(&unit);
    if unit == XfaUnit::Unknown {
        return Err(binding_error(
            "LayoutPseudoModel",
            "h/w/x/y",
            JsMessage::ValueError,
        ));
    }
    let value = CxfaMeasurement::new(value, XfaUnit::Pt).to_unit(unit);
    Ok(((value * 1000.).round() / 1000.).into())
}
fn height(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    measure(this, args, context, Dimension::Height)
}
fn width(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    measure(this, args, context, Dimension::Width)
}
fn x(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    measure(this, args, context, Dimension::X)
}
fn y(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    measure(this, args, context, Dimension::Y)
}
fn all_page_count(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    Ok(require_layout(&runtime)?.count_pages().into())
}
fn numbered_page_count(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    let layout = require_layout(&runtime)?;
    let count = (0..layout.count_pages())
        .filter(|&index| {
            layout
                .get_page(index)
                .is_some_and(|page| page.get_master_page().get_integer(A::Numbered) != 0)
        })
        .count();
    Ok((count as f64).into())
}
fn page_index(item: &CxfaContentLayoutItem) -> JsResult<i32> {
    item.get_page()
        .map(|page| page.get_page_index())
        .filter(|&index| index >= 0)
        .ok_or_else(|| {
            JsNativeError::reference()
                .with_message("XFA layout fragment is not attached to a document page")
                .into()
        })
}
fn page_span(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    if args.len() != 1 {
        return Err(parameter_error("LayoutPseudoModel", "pageSpan"));
    }
    let Some(node) = runtime.node_from_value(&args[0]) else {
        return Ok(JsValue::undefined());
    };
    let layout = require_layout(&runtime)?;
    let Some(item) = layout
        .get_layout_item(&node)
        .and_then(|item| item.as_content_layout_item())
    else {
        return Ok((-1).into());
    };
    let first = page_index(&item.get_first())?;
    let last = page_index(&item.get_last())?;
    Ok((last - first + 1).into())
}
fn page_internal(this: &JsValue, args: &[JsValue], absolute: bool) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    if args.len() != 1 {
        return Err(parameter_error("LayoutPseudoModel", "page/absPage/sheet"));
    }
    let Some(node) = runtime.node_from_value(&args[0]) else {
        return Ok(0.into());
    };
    let layout = require_layout(&runtime)?;
    let Some(item) = layout
        .get_layout_item(&node)
        .and_then(|item| item.as_content_layout_item())
    else {
        return Ok((-1).into());
    };
    Ok((page_index(&item.get_first())? + i32::from(!absolute)).into())
}
fn page(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    page_internal(this, args, false)
}
fn abs_page(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    page_internal(this, args, true)
}
fn zero(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this)?;
    Ok(0.into())
}
fn batch_page(this: &JsValue, args: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this)?;
    if args.len() != 1 {
        return Err(parameter_error(
            "LayoutPseudoModel",
            "absPageInBatch/sheetInBatch",
        ));
    }
    Ok(0.into())
}
fn noop(this: &JsValue, _: &[JsValue], _: &mut Context) -> JsResult<JsValue> {
    runtime(this)?;
    Ok(JsValue::undefined())
}
fn relayout(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    CfxJsEngine::with_host_callback(context, || {
        let layout = require_layout(&runtime)?;
        if runtime
            .document()
            .get_root()
            .and_then(|root| root.get_first_child_by_class(E::Form))
            .is_some_and(|form| form.first_child().is_some())
        {
            layout.set_has_changed_container();
        }
        layout.set_force_relayout();
        Ok::<(), boa_engine::JsError>(())
    })?;
    Ok(JsValue::undefined())
}
fn append_content(
    root: &CxfaLayoutItem,
    filter: Option<E>,
    seen: &mut HashSet<CxfaNode>,
    result: &mut Vec<CxfaNode>,
) {
    let mut iterator = CxfaLayoutItemIterator::new(Some(root.clone()));
    let mut current = iterator.get_current();
    while let Some(item) = current {
        current = iterator.move_to_next();
        if !item.is_content_layout_item() {
            continue;
        }
        let node = item.form_node();
        let element = node.get_element_type();
        if filter.map_or(
            !matches!(element, E::Field | E::Draw | E::Subform | E::Area),
            |filter| element != filter,
        ) {
            continue;
        }
        if seen.insert(node.clone()) {
            result.push(node);
        }
    }
}
fn object_array(
    layout: &CxfaLayoutProcessor,
    index: i32,
    kind: &[u32],
    on_page_area: bool,
) -> Vec<CxfaNode> {
    let Some(page) = layout.get_page(index) else {
        return Vec::new();
    };
    let equals = |name: &str| kind.iter().copied().eq(name.bytes().map(u32::from));
    if equals("pageArea") {
        return vec![page.form_node()];
    }
    if equals("contentArea") {
        return page
            .children()
            .into_iter()
            .filter(|item| item.form_node().get_element_type() == E::ContentArea)
            .map(|item| item.form_node())
            .collect();
    }
    let filter = if kind.is_empty() {
        None
    } else if equals("field") {
        Some(E::Field)
    } else if equals("draw") {
        Some(E::Draw)
    } else if equals("subform") {
        Some(E::Subform)
    } else if equals("area") {
        Some(E::Area)
    } else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    if kind.is_empty() {
        result.push(page.form_node());
    }
    for item in page.children() {
        if item.form_node().get_element_type() == E::ContentArea {
            if kind.is_empty() {
                result.push(item.form_node());
            }
            if !on_page_area {
                // Traverse every content root; the native first-child iterator
                // otherwise omits independent sibling roots in a content area.
                for root in item.children() {
                    append_content(&root, filter, &mut seen, &mut result);
                }
            }
        } else if on_page_area {
            append_content(&item, filter, &mut seen, &mut result);
        }
    }
    result
}
fn page_content(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let runtime = runtime(this)?;
    if args.is_empty() || args.len() > 3 {
        return Err(parameter_error("LayoutPseudoModel", "pageContent"));
    }
    let index = args[0].to_i32(context)?;
    let kind = args
        .get(1)
        .map(|value| to_wide(value, context))
        .transpose()?
        .unwrap_or_default();
    let on_page_area = args.get(2).is_some_and(JsValue::to_boolean);
    let Some(host) = runtime.layout_host() else {
        return Ok(JsValue::undefined());
    };
    let layout = host.layout_processor().ok_or_else(|| {
        JsNativeError::reference().with_message("XFA layout document has expired")
    })?;
    let nodes = object_array(&layout, index, &kind, on_page_area);
    let list = CxfaArrayNodeList::new(runtime.document());
    list.set_array_node_list(&nodes);
    runtime
        .document()
        .get_node_owner()
        .persist_list(list.clone());
    new_list_binding(&runtime, list, context).map(Into::into)
}
#[cfg(test)]
#[path = "cjx_layoutpseudomodel_tests.rs"]
mod tests;
