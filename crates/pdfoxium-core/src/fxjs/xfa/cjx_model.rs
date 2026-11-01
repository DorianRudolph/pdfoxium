// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CJX_Model methods. The two empty native property callbacks remain no-ops;
//! createNode deliberately coerces its unused namespace argument without applying it.
use super::cfxjse_engine::node_data;
use crate::{
    fxjs::{
        cfxjs_engine::CfxJsEngine, cjs_event_context::wide_from_utf16, cjs_field::binding_error,
        js_resources::JsMessage,
    },
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaPacketType},
        parser::xfa_basic_data::get_element_by_name,
    },
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsObject, JsResult, JsString, JsValue, NativeFunction,
};

pub fn define_model_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    for (name, length, callback) in [
        (
            "clearErrorList",
            0,
            clear_error_list as NativeFunctionPointer,
        ),
        ("createNode", 1, create_node),
        ("isCompatibleNS", 1, is_compatible_ns),
    ] {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(callback, super::cjx_bindings::MethodClass::Model),
        )
        .name(JsString::from(name))
        .length(length)
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
    }
    for (name, get, set) in [
        (
            "context",
            get_context as NativeFunctionPointer,
            set_context as NativeFunctionPointer,
        ),
        ("aliasNode", get_alias_node, set_alias_node),
    ] {
        let get = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(get))
            .name(JsString::from(format!("get {name}")))
            .length(0)
            .constructor(false)
            .build();
        let set = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set))
            .name(JsString::from(format!("set {name}")))
            .length(1)
            .constructor(false)
            .build();
        object.define_property_or_throw(
            JsString::from(name),
            PropertyDescriptor::builder()
                .get(get)
                .set(set)
                .enumerable(true)
                .configurable(true),
            context,
        )?;
    }
    Ok(())
}
pub fn clear_error_list(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    node_data(this, context)?;
    // The pinned method succeeds without checking, coercing, or using arguments.
    Ok(JsValue::undefined())
}
pub fn create_node(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.is_empty() || args.len() > 3 {
        return Err(binding_error("Model", "createNode", JsMessage::ParamError));
    }
    let name = if args.len() > 1 {
        wide_from_utf16(&args[1].to_string(context)?.to_vec())
    } else {
        Vec::new()
    };
    if args.len() == 3 {
        let _namespace = args[2].to_string(context)?;
    }
    let tag = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    let runtime = binding.runtime()?;
    let element = get_element_by_name(&tag);
    let Some(node) =
        CfxJsEngine::with_host_callback(context, || binding.node.create_same_packet_node(element))
    else {
        return Ok(JsValue::null());
    };
    if !name.is_empty() {
        if !node.has_attribute(XfaAttribute::Name) {
            return Err(binding_error("Model", "createNode", JsMessage::ParamError));
        }
        CfxJsEngine::with_host_callback(context, || {
            node.set_attribute_by_enum(XfaAttribute::Name, &name, true);
            if node.get_packet_type() == XfaPacketType::Datasets {
                node.create_xml_mapping_node();
            }
        });
    }
    runtime
        .get_or_create_binding(&node, context)
        .map(JsValue::from)
}
pub fn is_compatible_ns(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    if args.is_empty() {
        return Err(binding_error(
            "Model",
            "isCompatibleNS",
            JsMessage::ParamError,
        ));
    }
    let namespace = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    binding.runtime()?;
    Ok((binding.node.try_namespace().unwrap_or_default() == namespace).into())
}
pub fn get_context(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    node_data(this, context)?;
    Ok(JsValue::undefined())
}
pub fn set_context(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    node_data(this, context)?;
    Ok(JsValue::undefined())
}
pub fn get_alias_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    get_context(this, args, context)
}
pub fn set_alias_node(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    set_context(this, args, context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser,
        },
        fxjs::xfa::cfxjse_engine::CfxJseEngine,
        xfa::fxfa::{
            fxfa_basic::XfaElement,
            parser::{
                cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder,
                cxfa_node::CxfaNode,
            },
        },
    };
    use boa_engine::property::Attribute;
    use std::{cell::RefCell, rc::Rc};
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn setup() -> (Rc<CfxJseEngine>, CxfaNode) {
        let input=br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/><xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/"><xfa:data/></xfa:datasets></xdp:xdp>"#;
        let stream = Rc::new(CfxReadOnlySpanStream::new(input));
        let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
        let document = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(document.clone());
        assert!(builder.build_document(xml, XfaPacketType::Xdp));
        let root = builder.get_root_node().unwrap();
        let model = root
            .get_first_child_by_class(XfaElement::DataModel)
            .unwrap();
        let template = root.get_first_child_by_class(XfaElement::Template).unwrap();
        let runtime = CfxJseEngine::new(document).unwrap();
        runtime
            .javascript()
            .with_context(|context| {
                for (name, node) in [("subject", &model), ("template", &template)] {
                    let object = runtime.get_or_create_binding(node, context)?;
                    context.register_global_property(
                        JsString::from(name),
                        object,
                        Attribute::all(),
                    )?;
                }
                Ok(())
            })
            .unwrap();
        (runtime, model)
    }
    #[test]
    fn create_node_preserves_name_namespace_tag_coercion_and_dataset_mapping() {
        let (runtime, model) = setup();
        assert_eq!(runtime.eval(r#"let order=[];const made=subject.createNode({toString(){order.push('tag');return 'dataGroup'}},{toString(){order.push('name');return 'person'}},{toString(){order.push('namespace');return 'urn:ignored'}});if(order.join(',')!=='name,namespace,tag')throw Error(order);if(made.getAttribute('name')!=='person'||made.parent!==null)throw Error('created');subject.nodes.append(made);if(subject.nodes.namedItem('person')!==made)throw Error('identity');made"#).unwrap().as_object().is_some(),true);
        let node = model.get_first_child_by_name(&wide("person")).unwrap();
        let mapping = node.get_xml_mapping_node().unwrap();
        assert_eq!(mapping.name().as_ref(), wide("person"));
        assert!(!mapping.has_attribute(&wide("xmlns")));
        assert!(runtime.eval("subject.createNode('field')===null && subject.createNode('nonesuch')===null && subject.createNode('dataValue').parent===null").unwrap().to_boolean());
    }
    #[test]
    fn model_argument_failures_opaque_exceptions_and_native_noops() {
        let (runtime, _) = setup();
        assert!(runtime.eval(r#"let caught=0;for(const f of [()=>subject.createNode(),()=>subject.createNode(1,2,3,4),()=>subject.isCompatibleNS(),()=>template.createNode('ui','bad')]){try{f()}catch(e){caught++}}if(caught!==4)throw Error('parameter checks');const marker={};let tag=false,ok=false;try{subject.createNode({toString(){tag=true;return 'dataGroup'}},'',{toString(){throw marker}})}catch(e){ok=e===marker}if(!ok||tag)throw Error('coercion exception');let coerced=false;const unused={toString(){coerced=true;throw marker}};subject.clearErrorList(unused,unused);subject.context=unused;subject.aliasNode=unused;!coerced && subject.context===undefined && subject.aliasNode===undefined"#).unwrap().to_boolean());
    }
    #[test]
    fn namespace_and_nested_creation_reenter_the_real_runtime() {
        let (runtime, model) = setup();
        assert!(runtime.eval(r#"let ignored=false;const unused={toString(){ignored=true;return ''}};if(!subject.isCompatibleNS('http://www.xfa.org/schema/xfa-data/1.0/',unused)||ignored)throw Error('namespace');let inner;const outer=subject.createNode('dataGroup',{toString(){inner=subject.createNode('dataValue','inner');subject.nodes.append(inner);return 'outer'}});subject.nodes.append(outer);subject.nodes.namedItem('inner')===inner && subject.nodes.namedItem('outer')===outer && !subject.isCompatibleNS('wrong')"#).unwrap().to_boolean());
        assert_eq!(
            model
                .get_first_child_by_name(&wide("inner"))
                .unwrap()
                .get_cdata(XfaAttribute::Name),
            wide("inner")
        );
    }
    struct ReadyObserver {
        runtime: RefCell<std::rc::Weak<CfxJseEngine>>,
        calls: std::cell::Cell<usize>,
    }
    impl crate::xfa::fxfa::parser::cxfa_document::XfaDocumentNotify for ReadyObserver {
        fn on_child_added(&self, _: &CxfaNode, _: &CxfaNode) {}
        fn on_child_removed(&self, _: &CxfaNode, _: &CxfaNode) {}
        fn on_attribute_changed(&self, _: &CxfaNode, _: XfaAttribute) {}
        fn on_node_ready(&self, node: &CxfaNode) {
            assert!(!node.is_initialized());
            self.calls.set(self.calls.get() + 1);
            let runtime = self.runtime.borrow().upgrade().unwrap();
            assert!(runtime
                .eval("typeof subject.clearErrorList==='function'")
                .unwrap()
                .to_boolean());
            node.set_cdata(XfaAttribute::Name, &wide("from-ready"));
        }
    }
    #[test]
    fn node_ready_notification_reenters_boa_before_final_name_assignment() {
        let (runtime, model) = setup();
        let observer = Rc::new(ReadyObserver {
            runtime: RefCell::new(Rc::downgrade(&runtime)),
            calls: std::cell::Cell::new(0),
        });
        runtime.document().set_notify(Some(observer.clone()));
        assert!(runtime.eval("const ready=subject.createNode('dataGroup','final');subject.nodes.append(ready);ready.getAttribute('name')==='final'").unwrap().to_boolean());
        assert_eq!(observer.calls.get(), 1);
        assert_eq!(
            model
                .get_first_child_by_name(&wide("final"))
                .unwrap()
                .get_cdata(XfaAttribute::Name),
            wide("final")
        );
    }
}
