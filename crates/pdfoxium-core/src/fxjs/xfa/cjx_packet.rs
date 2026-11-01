// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! CJX_Packet overrides operate on its mapped XML element. Appended content is
//! allocated in the document's retained XML owner, without an FF host facade.
use super::cfxjse_engine::node_data;
use crate::{
    fxcrt::{
        fx_string::utf16_encode,
        xml::cfx_xmlnode::{XmlNodeRef, XmlNodeType},
    },
    fxjs::{cjs_event_context::wide_from_utf16, cjs_field::binding_error, js_resources::JsMessage},
    xfa::fxfa::parser::cxfa_node::CxfaNode,
};
use boa_engine::{
    native_function::NativeFunctionPointer, object::FunctionObjectBuilder,
    property::PropertyDescriptor, Context, JsNativeError, JsObject, JsResult, JsString, JsValue,
    NativeFunction,
};

pub fn define_packet_methods(object: &JsObject, context: &mut Context) -> JsResult<()> {
    for (name, length, callback) in [
        ("getAttribute", 1, get_attribute as NativeFunctionPointer),
        ("setAttribute", 2, set_attribute),
        ("removeAttribute", 1, remove_attribute),
    ] {
        let function = FunctionObjectBuilder::new(
            context.realm(),
            super::cjx_bindings::native_method(
                callback,
                super::cjx_bindings::MethodClass::Element(
                    crate::xfa::fxfa::fxfa_basic::XfaElement::Packet,
                ),
            ),
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
    let get = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(get_content))
        .name(JsString::from("get content"))
        .length(0)
        .constructor(false)
        .build();
    let set = FunctionObjectBuilder::new(context.realm(), NativeFunction::from_fn_ptr(set_content))
        .name(JsString::from("set content"))
        .length(1)
        .constructor(false)
        .build();
    object.define_property_or_throw(
        JsString::from("content"),
        PropertyDescriptor::builder()
            .get(get)
            .set(set)
            .enumerable(true)
            .configurable(true),
        context,
    )?;
    Ok(())
}
fn element(node: &CxfaNode) -> Option<XmlNodeRef> {
    node.get_xml_mapping_node()
        .filter(|xml| xml.node_type() == XmlNodeType::Element)
}
fn string(value: &[u32]) -> JsValue {
    JsString::from(utf16_encode(value).as_slice()).into()
}
fn check_count(args: &[JsValue], count: usize, method: &str) -> JsResult<()> {
    if args.len() == count {
        Ok(())
    } else {
        Err(binding_error("Packet", method, JsMessage::ParamError))
    }
}
pub fn get_attribute(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    check_count(args, 1, "getAttribute")?;
    let Some(element) = element(&binding.node) else {
        return Ok(string(&[]));
    };
    let name = wide_from_utf16(&args[0].to_string(context)?.to_vec());
    Ok(string(&element.get_attribute(&name)))
}
pub fn set_attribute(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    check_count(args, 2, "setAttribute")?;
    if let Some(element) = element(&binding.node) {
        // API order is value, then attribute name, as on CJX_Node. The native
        // nested SetAttribute call leaves argument evaluation unsequenced.
        let value = wide_from_utf16(&args[0].to_string(context)?.to_vec());
        let name = wide_from_utf16(&args[1].to_string(context)?.to_vec());
        element.set_attribute(&name, &value);
    }
    Ok(JsValue::null())
}
pub fn remove_attribute(
    this: &JsValue,
    args: &[JsValue],
    context: &mut Context,
) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    check_count(args, 1, "removeAttribute")?;
    if let Some(element) = element(&binding.node) {
        let name = wide_from_utf16(&args[0].to_string(context)?.to_vec());
        element.remove_attribute(&name);
    }
    Ok(JsValue::null())
}
pub fn read_content(node: &CxfaNode) -> JsValue {
    string(&element(node).map_or_else(Vec::new, |xml| xml.text_data()))
}
pub fn write_content(node: &CxfaNode, value: &JsValue, context: &mut Context) -> JsResult<()> {
    let Some(element) = element(node) else {
        return Ok(());
    };
    // Capture the real owner before coercion, retaining both allocations if
    // script changes the packet mapping or document's active XML owner.
    let owner = node
        .document()
        .and_then(|doc| doc.xml_document())
        .ok_or_else(|| {
            JsNativeError::reference().with_message("Packet content requires its XML document")
        })?;
    let text = wide_from_utf16(&value.to_string(context)?.to_vec());
    let text = owner.borrow_mut().create_text_owned(text);
    if element.is_alive() {
        element.append_last_child(&text);
    }
    Ok(())
}
pub fn get_content(this: &JsValue, _: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    Ok(read_content(&binding.node))
}
pub fn set_content(this: &JsValue, args: &[JsValue], context: &mut Context) -> JsResult<JsValue> {
    let binding = node_data(this, context)?;
    write_content(
        &binding.node,
        args.first().unwrap_or(&JsValue::undefined()),
        context,
    )?;
    Ok(JsValue::undefined())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxcrt::{
            cfx_memorystream::CfxMemoryStream, cfx_read_only_span_stream::CfxReadOnlySpanStream,
            xml::cfx_xmlparser::CfxXmlParser,
        },
        fxjs::xfa::cfxjse_engine::CfxJseEngine,
        xfa::fxfa::{
            fxfa_basic::{XfaElement, XfaPacketType},
            parser::{cxfa_document::CxfaDocument, cxfa_document_builder::CxfaDocumentBuilder},
        },
    };
    use boa_engine::property::Attribute;
    use std::{cell::RefCell, rc::Rc};
    fn wide(s: &str) -> Vec<u32> {
        s.chars().map(u32::from).collect()
    }
    fn build(input: &[u8]) -> (Rc<CxfaDocument>, CxfaNode) {
        let stream = Rc::new(CfxReadOnlySpanStream::new(input));
        let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
        let doc = CxfaDocument::new();
        let mut builder = CxfaDocumentBuilder::new(doc.clone());
        assert!(builder.build_document(xml, XfaPacketType::Xdp));
        (doc, builder.get_root_node().unwrap())
    }
    fn setup() -> (Rc<CfxJseEngine>, CxfaNode, CxfaNode) {
        let (doc,root)=build(br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"/><custom xmlns="urn:custom" old="original">A<child>B</child><![CDATA[C]]><?pi keep?></custom></xdp:xdp>"#);
        let packet = root.get_first_child_by_class(XfaElement::Packet).unwrap();
        let runtime = CfxJseEngine::new(doc).unwrap();
        runtime
            .javascript()
            .with_context(|context| {
                let binding = runtime.get_or_create_binding(&packet, context)?;
                context.register_global_property(
                    JsString::from("subject"),
                    binding,
                    Attribute::all(),
                )?;
                Ok(())
            })
            .unwrap();
        (runtime, root, packet)
    }
    #[test]
    fn parsed_packet_attribute_overrides_content_append_and_xml_reparse() {
        let (runtime, root, packet) = setup();
        assert!(runtime.eval(r#"if(subject.getAttribute('old')!=='original'||subject.content!=='AC')throw Error('original');if(subject.setAttribute('value & <tag>','new')!==null)throw Error('set return');if(subject.getAttribute('new')!=='value & <tag>'||subject.getAttribute('missing')!=='')throw Error('xml attributes');if(subject.removeAttribute('old')!==null||subject.getAttribute('old')!=='')throw Error('remove');subject.content=' D & < >';subject.content=' E';subject.content==='AC D & < > E'"#).unwrap().to_boolean());
        let xml = packet.get_xml_mapping_node().unwrap();
        assert_eq!(
            xml.get_attribute(&wide("new")).as_ref(),
            wide("value & <tag>")
        );
        assert!(xml.first_child_named(&wide("child")).is_some());
        let mut output = CfxMemoryStream::new();
        root.get_xml_mapping_node().unwrap().save(&mut output);
        let (_reloaded_doc, reloaded) = build(output.get_span());
        let reloaded = reloaded
            .get_first_child_by_class(XfaElement::Packet)
            .unwrap()
            .get_xml_mapping_node()
            .unwrap();
        assert_eq!(reloaded.text_data(), wide("AC D & < > E"));
        assert_eq!(
            reloaded.get_attribute(&wide("new")).as_ref(),
            wide("value & <tag>")
        );
        assert_eq!(
            reloaded
                .first_child_named(&wide("child"))
                .unwrap()
                .text_data(),
            wide("B")
        );
    }
    #[test]
    fn packet_argument_counts_coercion_reentry_and_opaque_errors() {
        let (runtime, _, packet) = setup();
        assert!(runtime.eval(r#"let count=0;for(const f of [()=>subject.getAttribute(),()=>subject.getAttribute('x','y'),()=>subject.setAttribute('x'),()=>subject.removeAttribute()]){try{f()}catch(e){count++}}if(count!==4)throw Error('counts');let order=[];subject.setAttribute({toString(){order.push('value');subject.setAttribute('nested','inner');return 'outer'}},{toString(){order.push('name');return 'new'}});if(order.join(',')!=='value,name'||subject.getAttribute('inner')!=='nested')throw Error('reentry');const marker={};let caught=false;try{subject.setAttribute({toString(){throw marker}},'never')}catch(e){caught=e===marker}if(!caught||subject.getAttribute('never')!=='')throw Error('opaque');subject.getAttribute({toString(){subject.setAttribute('changed','new');return 'new'}})==='changed'"#).unwrap().to_boolean());
        assert_eq!(
            packet
                .get_xml_mapping_node()
                .unwrap()
                .get_attribute(&wide("new"))
                .as_ref(),
            wide("changed")
        );
    }
    #[test]
    fn absent_or_non_element_mapping_skips_all_argument_coercion() {
        let (runtime, _, packet) = setup();
        packet.clear_xml_mapping_node();
        assert!(runtime.eval(r#"let coerced=false;const bad={toString(){coerced=true;throw 1}};if(subject.getAttribute(bad)!==''||subject.setAttribute(bad,bad)!==null||subject.removeAttribute(bad)!==null)throw Error('empty mapping');subject.content=bad;!coerced && subject.content===''"#).unwrap().to_boolean());
        let text = runtime
            .document()
            .xml_document()
            .unwrap()
            .borrow_mut()
            .create_text(&wide("not an element"));
        packet.set_xml_mapping_node(text);
        assert!(runtime
            .eval(
                "subject.content='ignored';subject.content==='' && subject.getAttribute('x')===''"
            )
            .unwrap()
            .to_boolean());
    }
    #[test]
    fn content_coercion_can_reenter_and_allocates_in_retained_document_owner() {
        let (runtime, _, packet) = setup();
        let owner = runtime.document().xml_document().unwrap();
        let original = packet.get_xml_mapping_node().unwrap();
        assert!(runtime.eval(r#"subject.content={toString(){subject.content='inner';subject.setAttribute('yes','reentered');return 'outer'}};subject.content==='ACinnerouter' && subject.getAttribute('reentered')==='yes'"#).unwrap().to_boolean());
        let last = original.last_child().unwrap();
        // Replacing the active XML owner retains the previous mapped element.
        let replacement = Rc::new(RefCell::new(
            crate::fxcrt::xml::cfx_xmldocument::CfxXmlDocument::new(),
        ));
        runtime.document().set_xml_document(replacement);
        runtime.eval("subject.content='tail'").unwrap();
        assert_eq!(last.next_sibling(), original.last_child());
        assert!(last.next_sibling().is_some());
        assert_eq!(original.text_data(), wide("ACinneroutertail"));
        drop(owner);
        assert!(original.is_alive());
        assert_eq!(packet.get_xml_mapping_node(), Some(original));
    }
}
