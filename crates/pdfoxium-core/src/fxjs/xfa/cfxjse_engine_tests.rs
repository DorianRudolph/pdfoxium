// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Connected CFXJSE_Engine/CJX contracts through actual Boa execution. These
//! parser/model fixtures do not claim public PDF/XFA embedder coverage.
use super::cfxjse_engine::CfxJseEngine;
use crate::{
    fxcrt::{cfx_read_only_span_stream::CfxReadOnlySpanStream, xml::cfx_xmlparser::CfxXmlParser},
    xfa::fxfa::{
        fxfa_basic::{XfaAttribute, XfaElement, XfaPacketType},
        parser::{
            cxfa_dataexporter::CxfaDataExporter,
            cxfa_document::{CxfaDocument, XfaDocumentNotify},
            cxfa_document_builder::CxfaDocumentBuilder,
            cxfa_model_properties::CxfaScriptType,
            cxfa_node::CxfaNode,
        },
    },
};
use std::{
    cell::{Cell, RefCell},
    rc::{Rc, Weak},
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn build(input: &[u8], packet: XfaPacketType) -> (Rc<CxfaDocument>, CxfaNode) {
    let mut bytes = input.to_vec();
    bytes.push(0);
    let stream = Rc::new(CfxReadOnlySpanStream::new(&bytes));
    let xml = Rc::new(RefCell::new(CfxXmlParser::new(stream).parse().unwrap()));
    let document = CxfaDocument::new();
    let mut builder = CxfaDocumentBuilder::new(document.clone());
    assert!(builder.build_document(xml, packet));
    let root = builder.get_root_node().unwrap();
    assert!(document.set_root(Some(root.clone())));
    (document, root)
}

fn named(parent: &CxfaNode, name: &str) -> CxfaNode {
    parent.get_first_child_by_name(&wide(name)).unwrap()
}

fn merged_document() -> (Rc<CxfaDocument>, CxfaNode, Rc<CfxJseEngine>) {
    let input = br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/2.8/">
        <subform name="record">
          <field name="amount"><ui><textEdit/></ui>
            <bind match="dataRef" ref="$record.amount"/>
          </field>
          <field name="mirror"><ui><textEdit/></ui>
            <bind match="dataRef" ref="$record.amount"/>
          </field>
        </subform>
      </template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
        <xfa:data><record><amount>original</amount></record></xfa:data>
      </xfa:datasets>
    </xdp:xdp>"#;
    let (document, root) = build(input, XfaPacketType::Xdp);
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    (document, root, runtime)
}

fn assert_script(runtime: &CfxJseEngine, source: &str) {
    assert_eq!(
        runtime.eval(source).unwrap().as_boolean(),
        Some(true),
        "{source}"
    );
}

#[test]
fn merged_xdp_boa_values_attributes_lists_and_xml_roundtrip() {
    let (_document, root, runtime) = merged_document();
    let form = root.get_first_child_by_class(XfaElement::Form).unwrap();
    let record = named(&form, "record");
    let amount = named(&record, "amount");
    let mirror = named(&record, "mirror");
    let datasets = root
        .get_first_child_by_class(XfaElement::DataModel)
        .unwrap();
    let data_amount = named(&named(&named(&datasets, "data"), "record"), "amount");
    assert_eq!(amount.get_bind_data(), Some(data_amount.clone()));
    assert_eq!(mirror.get_bind_data(), Some(data_amount.clone()));
    let data_mapping = data_amount.get_xml_mapping_node().unwrap();
    assert_script(
        &runtime,
        r#"
        var record = xfa.form.resolveNode('record');
        var amount = record.resolveNode('amount');
        var mirror = record.resolveNode('mirror');
        var data = xfa.datasets.resolveNode('data.record.amount');
        amount === xfa.form.resolveNode('record.amount') &&
        amount.parent === record && amount.rawValue === 'original' &&
        mirror.rawValue === 'original' && data.value === 'original'
    "#,
    );
    assert_script(
        &runtime,
        r#"
        amount.rawValue = 'edited & <value> 😀';
        amount.setAttribute('23pt', 'w');
        mirror.rawValue === amount.rawValue && data.value === amount.rawValue &&
        amount.getAttribute('w') === '23pt'
    "#,
    );
    let edited = wide("edited & <value> 😀");
    assert_eq!(amount.get_raw_value(), edited);
    assert_eq!(mirror.get_raw_value(), edited);
    assert_eq!(data_amount.get_raw_value(), edited);
    assert_eq!(data_amount.get_xml_mapping_node(), Some(data_mapping));
    assert_script(
        &runtime,
        r#"
        var live = record.nodes;
        var initialLength = live.length;
        var snapshot = record.resolveNodes('#field[*]');
        var classSnapshot = amount.classAll;
        var namedSnapshot = amount.all;
        var temporary = amount.clone(true);
        temporary.setAttribute('temporary', 'name');
        live.append(temporary);
        snapshot.length === 2 && snapshot.item(0) === amount &&
        snapshot.item(1) === mirror && classSnapshot.length === 2 &&
        namedSnapshot.length === 1 && live.length === initialLength + 1 &&
        live.item(initialLength) === temporary && temporary.parent === record
    "#,
    );
    assert_script(
        &runtime,
        r#"
        var changedIndexError = false;
        try {
            live.item({ valueOf() { live.remove(temporary); return initialLength; } });
        } catch (e) { changedIndexError = true; }
        var invalidInsert = false;
        try { live.insert(record, amount); } catch (e) { invalidInsert = true; }
        var foreignAnchor = false;
        try { live.insert(temporary, xfa.form); } catch (e) { foreignAnchor = true; }
        var nonNode = false;
        try { live.append({}); } catch (e) { nonNode = true; }
        var coercedLength = false;
        var readOnly = false;
        try { live.length = { valueOf() { coercedLength = true; return 0; } }; }
        catch (e) { readOnly = true; }
        invalidInsert && foreignAnchor && nonNode && readOnly && !coercedLength &&
        changedIndexError && temporary.parent === null && live.item(0) === amount &&
        live.length === initialLength && snapshot.length === 2 &&
        classSnapshot.length === 2 && namedSnapshot.length === 1 &&
        live.namedItem('missing') === undefined && record.resolveNode('missing') === null &&
        amount === record.resolveNode('amount')
    "#,
    );
    assert_eq!(
        runtime
            .run_script(
                CxfaScriptType::Javascript,
                &wide("this === amount && this.rawValue === mirror.rawValue"),
                Some(&amount),
            )
            .unwrap()
            .as_boolean(),
        Some(true)
    );
    assert_eq!(
        runtime
            .run_script(CxfaScriptType::Formcalc, &wide("Sum(2, 3)"), Some(&amount),)
            .unwrap()
            .as_number(),
        Some(5.0)
    );

    let exporter = CxfaDataExporter::new();
    let form_xml = exporter.export_to_vec(&form).unwrap();
    let (_document, reloaded_form) = build(&form_xml, XfaPacketType::Form);
    let reloaded_record = named(&reloaded_form, "record");
    let reloaded_amount = named(&reloaded_record, "amount");
    assert_eq!(reloaded_amount.get_raw_value(), edited);
    assert_eq!(named(&reloaded_record, "mirror").get_raw_value(), edited);
    assert_eq!(
        reloaded_amount.get_attribute_by_enum(XfaAttribute::W),
        wide("23pt")
    );
    let data_xml = exporter.export_to_vec(&datasets).unwrap();
    let (_document, reloaded_datasets) = build(&data_xml, XfaPacketType::Datasets);
    assert_eq!(
        named(
            &named(&named(&reloaded_datasets, "data"), "record"),
            "amount"
        )
        .get_raw_value(),
        edited
    );
    assert_eq!(amount.get_bind_data(), Some(data_amount.clone()));
    assert_eq!(mirror.get_bind_data(), Some(data_amount));
}

struct ReenterValueChange {
    runtime: Weak<CfxJseEngine>,
    active: Cell<bool>,
    callbacks: Cell<usize>,
}
impl XfaDocumentNotify for ReenterValueChange {
    fn on_child_added(&self, _parent: &CxfaNode, _child: &CxfaNode) {}
    fn on_child_removed(&self, _parent: &CxfaNode, _child: &CxfaNode) {}
    fn on_attribute_changed(&self, _node: &CxfaNode, attribute: XfaAttribute) {
        if attribute != XfaAttribute::Value || self.active.replace(true) {
            return;
        }
        let runtime = self.runtime.upgrade().unwrap();
        assert_script(
            &runtime,
            r#"
            globalThis.callbackReads = (globalThis.callbackReads || 0) + 1;
            xfa.form.resolveNode('record.amount') === amount
        "#,
        );
        self.callbacks.set(self.callbacks.get() + 1);
        self.active.set(false);
    }
    fn on_node_ready(&self, _node: &CxfaNode) {}
}

#[test]
fn script_mutation_reentry_opaque_errors_and_runtime_lifetime() {
    let (document, root, runtime) = merged_document();
    let amount = named(
        &named(
            &root.get_first_child_by_class(XfaElement::Form).unwrap(),
            "record",
        ),
        "amount",
    );
    assert_script(
        &runtime,
        "var amount = xfa.form.resolveNode('record.amount'); amount.rawValue === 'original'",
    );
    let observer = Rc::new(ReenterValueChange {
        runtime: Rc::downgrade(&runtime),
        active: Cell::new(false),
        callbacks: Cell::new(0),
    });
    document.set_notify(Some(observer.clone()));
    assert_script(
        &runtime,
        "amount.rawValue = 'nested change'; amount.rawValue === 'nested change'",
    );
    assert!(observer.callbacks.get() > 0);
    assert_script(&runtime, "callbackReads > 0");
    assert_script(
        &runtime,
        r#"
        var sentinel = { kind: 'opaque' };
        var caughtSame = false;
        try { amount.rawValue = { toString() { throw sentinel; } }; }
        catch (e) { caughtSame = e === sentinel; }
        caughtSame && amount.rawValue === 'nested change'
    "#,
    );
    assert!(runtime.eval("throw sentinel").is_err());
    assert_script(
        &runtime,
        "amount === xfa.form.resolveNode('record.amount') && amount.rawValue === 'nested change'",
    );
    document.set_notify(None);

    let retained = runtime
        .eval("globalThis.retained = amount; retained")
        .unwrap();
    assert!(retained.as_object().is_some());
    let javascript = runtime.javascript().clone();
    let weak_document = Rc::downgrade(&document);
    drop(document);
    assert!(weak_document.upgrade().is_some()); // Engine retains its document.
    drop(runtime);
    assert!(weak_document.upgrade().is_none());
    assert!(amount.is_alive()); // Script wrappers retain the allocation store.
    assert!(javascript
        .eval(&"retained.rawValue".encode_utf16().collect::<Vec<_>>())
        .is_err());
    assert_eq!(
        javascript
            .eval(&"6 * 7".encode_utf16().collect::<Vec<_>>())
            .unwrap()
            .as_number(),
        Some(42.0)
    );
    drop(retained);
    drop(javascript);
    boa_gc::force_collect();
    assert!(!amount.is_alive());
}

#[test]
fn production_som_predicates_object_attributes_and_this_scope() {
    let (_document, root, runtime) = merged_document();
    let amount = named(
        &named(
            &root.get_first_child_by_class(XfaElement::Form).unwrap(),
            "record",
        ),
        "amount",
    );
    assert_script(
        &runtime,
        r#"
        var record = xfa.form.resolveNode('record');
        var amount = record.resolveNode('amount');
        var mirror = record.resolveNode('mirror');
        amount.resolveNode('parent') === record &&
        amount.resolveNodes('parent').item(0) === record &&
        amount.resolveNode('model') === xfa.form &&
        amount.resolveNode('parent.mirror') === mirror && xfa.this === null
    "#,
    );
    let result = runtime.run_script(
        CxfaScriptType::Javascript,
        &wide(r#"
            var selected = record.resolveNodes('#field.(this.classIndex === 1 && xfa.this === this && this.resolveNode("parent") === record)');
            var scalarTrue = record.resolveNodes('#field.[1]');
            var scalarFalse = record.resolveNodes('#field.[0]');
            var failed = record.resolveNodes('#field.(throw new Error("predicate"))');
            selected.length === 1 && selected.item(0) === mirror &&
            scalarTrue.length === 2 && scalarFalse.length === 0 && failed.length === 0 &&
            this === amount && xfa.this === amount
        "#),
        Some(&amount),
    ).unwrap();
    assert_eq!(result.as_boolean(), Some(true));
    assert!(runtime.current_this_node().is_none());
    assert_eq!(runtime.current_script_type(), CxfaScriptType::Javascript);
    assert!(runtime
        .run_script(CxfaScriptType::Javascript, &wide("throw 17"), Some(&amount))
        .is_err());
    assert!(runtime.current_this_node().is_none());
    assert_script(&runtime, "xfa.this === null && amount.parent === record");
}

#[test]
fn typed_integer_decimal_boolean_default_and_raw_values() {
    let input = br#"<form xmlns="http://www.xfa.org/schema/xfa-form/3.3/">
      <subform name="record">
        <field name="integer"><ui><numericEdit/></ui><value><integer>7</integer></value></field>
        <field name="decimal"><ui><numericEdit/></ui><value><decimal fracDigits="2">1234.5678</decimal></value></field>
        <field name="unlimited"><ui><numericEdit/></ui><value><decimal fracDigits="-1">123.4500</decimal></value></field>
        <field name="boolean"><ui><checkButton/></ui><value><boolean>1</boolean></value></field>
      </subform>
    </form>"#;
    let (document, form) = build(input, XfaPacketType::Form);
    let runtime = CfxJseEngine::new(document).unwrap();
    assert_script(
        &runtime,
        r#"
        var record = xfa.resolveNode('record');
        var integer = record.resolveNode('integer');
        var decimal = record.resolveNode('decimal');
        var unlimited = record.resolveNode('unlimited');
        var boolean = record.resolveNode('boolean');
        var booleanValue = boolean.getElement('value').getElement('boolean');
        integer.rawValue === 7 && integer['{default}'] === 7 &&
        integer.getElement('value').getElement('integer').value === 7 &&
        typeof decimal.rawValue === 'number' && unlimited.rawValue === '123.4500' &&
        typeof unlimited.getElement('value').getElement('decimal').value === 'number' &&
        boolean.rawValue === true && booleanValue.value === true
    "#,
    );
    let decimal = runtime
        .eval("decimal.rawValue")
        .unwrap()
        .as_number()
        .unwrap();
    crate::test_support::assert_float_eq(1234.5678, decimal as f32);
    assert_script(
        &runtime,
        r#"
        integer['{default}'] = 12;
        unlimited.rawValue = '999.00100';
        booleanValue.value = 0;
        integer.rawValue === 12 && unlimited.rawValue === '999.00100' &&
        booleanValue.value === false && boolean.rawValue === false
    "#,
    );
    assert_script(
        &runtime,
        "booleanValue.value = 1; boolean.rawValue === true && booleanValue['{default}'] === true",
    );
    let record = named(&form, "record");
    assert_eq!(named(&record, "integer").get_raw_value(), wide("12"));
    assert_eq!(
        named(&record, "unlimited").get_raw_value(),
        wide("999.00100")
    );
    assert_eq!(named(&record, "boolean").get_raw_value(), wide("1"));
}

#[test]
fn generic_descriptor_coercions_and_selected_model_properties() {
    let input = br#"<template xmlns="http://www.xfa.org/schema/xfa-template/3.3/" lock="0">
      <subform name="record">
        <area name="position" level="2" lock="0"/>
        <event activity="initialize"><script contentType="application/x-javascript">1</script></event>
      </subform>
    </template>"#;
    let (document, template) = build(input, XfaPacketType::Template);
    let runtime = CfxJseEngine::new(document).unwrap();
    assert_script(
        &runtime,
        r#"
        var record = xfa.resolveNode('record');
        var area = record.resolveNode('position');
        var order = [];
        area.name = { toString() { order.push('string'); return 'renamed'; } };
        area.level = { valueOf() { order.push('integer'); return 9.8; } };
        var boolCoerced = false;
        area.lock = { valueOf() { boolCoerced = true; return 0; } };
        order.join(',') === 'string,integer' && !boolCoerced && area.lock === '1' &&
        area.level === 9 && area.name === 'renamed' && record.resolveNode('renamed') === area
    "#,
    );
    assert_script(
        &runtime,
        r#"
        area.lock = 0;
        xfa.lock = 1;
        var occur = record.getElement('occur');
        occur.min = 3;
        var raisedMaximum = occur.max === 3;
        occur.max = 1;
        var script = record.resolveNode('#event').getElement('script');
        var readonly = false;
        try { script.stateless = 1; } catch (e) { readonly = true; }
        area.lock === '0' && xfa.lock === '1' && raisedMaximum &&
        occur.min === 1 && occur.max === 1 && script.stateless === '0' && readonly
    "#,
    );
    let area = named(&named(&template, "record"), "renamed");
    assert_eq!(area.get_integer(XfaAttribute::Level), 9);
    assert!(!area.get_boolean(XfaAttribute::Lock));
    assert!(template.get_boolean(XfaAttribute::Lock));
}

#[test]
fn dataset_default_write_formats_xml_using_bound_numeric_picture() {
    use crate::xfa::fxfa::parser::cxfa_localemgr::CxfaLocaleMgr;
    let input = br#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/">
      <template xmlns="http://www.xfa.org/schema/xfa-template/3.3/">
        <subform name="record" locale="en_US">
          <field name="amount"><ui><numericEdit/></ui>
            <bind match="dataRef" ref="$record.amount"><picture>num{zz9.99}</picture></bind>
          </field>
        </subform>
      </template>
      <xfa:datasets xmlns:xfa="http://www.xfa.org/schema/xfa-data/1.0/">
        <xfa:data><record><amount>1.00</amount></record></xfa:data>
      </xfa:datasets>
    </xdp:xdp>"#;
    let (document, root) = build(input, XfaPacketType::Xdp);
    document.set_locale_mgr(Some(Rc::new(CxfaLocaleMgr::new(
        &wide("en_US"),
        Rc::new(|| 0),
    ))));
    let runtime = CfxJseEngine::new(document.clone()).unwrap();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let form = root.get_first_child_by_class(XfaElement::Form).unwrap();
    let field = named(&named(&form, "record"), "amount");
    let data = field.get_bind_data().unwrap();
    assert_script(
        &runtime,
        r#"
        var boundAmount = xfa.datasets.resolveNode('data.record.amount');
        boundAmount.value = '7.5';
        boundAmount.value === '7.5' && xfa.form.resolveNode('record.amount').rawValue === 7.5
    "#,
    );
    assert_eq!(data.get_raw_value(), wide("7.5"));
    assert_eq!(field.get_raw_value(), wide("7.5"));
    assert_eq!(
        data.get_xml_mapping_node().unwrap().text_data(),
        wide("7.50")
    );
}
