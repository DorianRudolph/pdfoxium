// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Method installation follows actual CJX constructor inheritance. The XML
//! schema's element-parent table has a different purpose and is not a JS class.
use super::{cfxjse_engine::node_data, cjx_list::new_list_binding};
use crate::{
    fxjs::{cjs_field::binding_error, js_resources::JsMessage},
    xfa::fxfa::{
        fxfa_basic::XfaElement,
        parser::{cxfa_arraynodelist::CxfaArrayNodeList, cxfa_node::CxfaNode},
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsObject, JsResult, JsString, JsValue,
};

#[path = "cjx_method.rs"]
mod method;
pub(super) use method::{
    native_method, validate as validate_method_receiver, Class as MethodClass,
};

// These native constructors use CJX_Object (or its Boolean specialization),
// rather than CJX_TextNode. XML content type does not define JS inheritance.
pub(super) fn is_node_class(element: XfaElement) -> bool {
    !matches!(
        element,
        XfaElement::Boolean
            | XfaElement::Decimal
            | XfaElement::ExData
            | XfaElement::Float
            | XfaElement::Integer
            | XfaElement::Text
            | XfaElement::Time
            | XfaElement::Delta
    )
}
fn is_container_class(element: XfaElement) -> bool {
    matches!(
        element,
        XfaElement::Field
            | XfaElement::Area
            | XfaElement::ContentArea
            | XfaElement::PageArea
            | XfaElement::PageSet
            | XfaElement::Variables
            | XfaElement::Draw
            | XfaElement::Subform
            | XfaElement::SubformSet
    )
}
fn is_model_class(element: XfaElement) -> bool {
    matches!(
        element,
        XfaElement::Xfa
            | XfaElement::Form
            | XfaElement::Template
            | XfaElement::DataModel
            | XfaElement::ConnectionSet
            | XfaElement::SourceSet
    )
}
fn method(
    object: &JsObject,
    name: &str,
    callback: NativeFunctionPointer,
    context: &mut Context,
) -> JsResult<()> {
    let class = if name == "getDelta" || name == "getDeltas" {
        MethodClass::Container
    } else {
        MethodClass::Element(XfaElement::Desc)
    };
    let function = FunctionObjectBuilder::new(context.realm(), native_method(callback, class))
        .name(JsString::from(name))
        .length(0)
        .constructor(false)
        .build();
    object.define_property_or_throw(
        JsString::from(name),
        PropertyDescriptor::builder()
            .value(function)
            .writable(false)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(())
}
pub(crate) fn define_methods(
    node: &CxfaNode,
    object: &JsObject,
    context: &mut Context,
) -> JsResult<()> {
    let element = node.get_element_type();
    if element == XfaElement::Xfa {
        let runtime =
            super::cfxjse_engine::CfxJseEngine::from_context(context).ok_or_else(|| {
                boa_engine::JsNativeError::reference().with_message("XFA context has expired")
            })?;
        use super::cfxjse_resolveprocessor::PseudoModel;
        for kind in [
            PseudoModel::Event,
            PseudoModel::Layout,
            PseudoModel::Host,
            PseudoModel::DataWindow,
            PseudoModel::Log,
            PseudoModel::Signature,
        ] {
            let pseudo = runtime.get_or_create_pseudo_binding(kind, context)?;
            object.define_property_or_throw(
                JsString::from(kind.name()),
                PropertyDescriptor::builder()
                    .value(pseudo)
                    .writable(false)
                    .enumerable(true)
                    .configurable(false),
                context,
            )?;
        }
    }
    if is_node_class(element) {
        super::cjx_tree::define_tree_methods(object, context)?;
        super::cjx_node::define_node_methods(object, context)?;
    }
    if is_container_class(element) {
        method(object, "getDelta", get_delta, context)?;
        method(object, "getDeltas", get_deltas, context)?;
    }
    if is_model_class(element) {
        super::cjx_model::define_model_methods(object, context)?;
    }
    match element {
        XfaElement::Form => super::cjx_form::define_form_methods(object, context)?,
        XfaElement::Template => super::cjx_template::define_template_methods(object, context)?,
        XfaElement::Manifest => super::cjx_manifest::define_manifest_methods(object, context)?,
        XfaElement::Source => super::cjx_source::define_source_methods(object, context)?,
        XfaElement::WsdlConnection => {
            super::cjx_wsdlconnection::define_wsdlconnection_methods(object, context)?
        }
        XfaElement::Delta => super::cjx_delta::define_delta_methods(object, context)?,
        XfaElement::Field => super::cjx_field::define_field_methods(object, context)?,
        XfaElement::ExclGroup => super::cjx_exclgroup::define_exclgroup_methods(object, context)?,
        XfaElement::Subform => super::cjx_subform::define_subform_methods(object, context)?,
        XfaElement::InstanceManager => {
            super::cjx_instancemanager::define_instance_manager_methods(object, context)?
        }
        XfaElement::Packet => super::cjx_packet::define_packet_methods(object, context)?,
        XfaElement::Desc => method(object, "metadata", metadata, context)?,
        _ => {}
    }
    Ok(())
}
fn get_delta(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    // This is the explicit native CJX_Container implementation.
    Ok(JsValue::undefined())
}
fn get_deltas(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let data = node_data(this, context)?;
    let runtime = data.runtime()?;
    let list = CxfaArrayNodeList::new(runtime.document());
    runtime
        .document()
        .get_node_owner()
        .persist_list(list.clone());
    new_list_binding(&runtime, list, context).map(Into::into)
}
fn metadata(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let _ = node_data(this, context)?;
    if args.len() > 1 {
        return Err(binding_error("Desc", "metadata", JsMessage::ParamError));
    }
    // The source metadata method deliberately returns the empty string.
    Ok(JsString::from("").into())
}

#[cfg(test)]
#[path = "cjx_method_tests.rs"]
mod method_tests;
