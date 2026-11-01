// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source query contracts using an actual FFDoc, native layout graph/fragments,
//! real parser nodes, persistent lists and Boa coercion/reentry.
use super::*;
use crate::{
    fxcrt::fx_coordinates::{CfxPointF, CfxSizeF},
    xfa::fxfa::{cxfa_ffdoc::CxfaFfDoc, fxfa_basic::XfaHashCode, test_support::new_document},
};
use boa_engine::{js_string, property::Attribute};
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
struct Fixture {
    doc: Rc<CxfaFfDoc>,
    runtime: Rc<CfxJseEngine>,
    field: CxfaNode,
    first: CxfaContentLayoutItem,
    second: CxfaContentLayoutItem,
}
fn fixture() -> Fixture {
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="record"><pageSet><pageArea name="one" numbered="1"><contentArea name="body" h="200pt"/></pageArea><pageArea name="two" numbered="0"><contentArea name="body" h="200pt"/></pageArea></pageSet><field name="amount"><ui><textEdit/></ui></field><field name="unlaid"><ui><textEdit/></ui></field><draw name="label"><value><text>caption</text></value></draw><subform name="section"><field name="nested"><ui><textEdit/></ui></field></subform><field name="master"><ui><textEdit/></ui></field></subform></template></xdp:xdp>"#,
    );
    doc.create_doc_view();
    let runtime = doc.script_context();
    let document = doc.model_document();
    document
        .do_data_merge_with_resolver(runtime.resolver())
        .unwrap();
    let form = document
        .get_xfa_node(XfaHashCode::Form as u32)
        .unwrap()
        .get_first_child_by_class(E::Subform)
        .unwrap();
    let template = form.get_template_node_if_exists().unwrap();
    let pages = template.get_first_child_by_class(E::PageSet).unwrap();
    let field = form.get_first_child_by_name(&wide("amount")).unwrap();
    let unlaid = form.get_first_child_by_name(&wide("unlaid")).unwrap();
    let label = form.get_first_child_by_name(&wide("label")).unwrap();
    let section = form.get_first_child_by_name(&wide("section")).unwrap();
    let nested = section.get_first_child_by_name(&wide("nested")).unwrap();
    let master = form.get_first_child_by_name(&wide("master")).unwrap();
    let layout = doc.layout_processor();
    let view = layout.get_layout_page_mgr();
    let store = layout.store();
    assert!(view.initialize_page_set(&pages));
    let mut contents = Vec::new();
    let mut page_items = Vec::new();
    for page in [
        pages.get_first_child_by_name(&wide("one")).unwrap(),
        pages.get_first_child_by_name(&wide("two")).unwrap(),
    ] {
        let record = view.create_view_record_simple();
        page_items.push(view.add_page_area_layout_item(record, &page).unwrap());
        contents.push(
            view.add_content_area_layout_item(
                record,
                page.get_first_child_by_class(E::ContentArea).as_ref(),
            )
            .unwrap(),
        );
        view.set_current_view_record(record);
    }
    let first = store.create_content(&field, None);
    first.set_position(CfxPointF::new(10.0014, 20.));
    first.set_size(CfxSizeF::new(72.1234, 36.5));
    contents[0].append_last_child(&first);
    field.set_layout_item(Some((*first).clone()));
    let second = store.create_content(&field, None);
    second.set_position(CfxPointF::new(3., 4.));
    second.set_size(CfxSizeF::new(144., 18.));
    contents[1].append_last_child(&second);
    first.insert_after(&second);
    let label_item = store.create_content(&label, None);
    contents[0].append_last_child(&label_item);
    let group = store.create_content(&section, None);
    contents[0].append_last_child(&group);
    let nested_item = store.create_content(&nested, None);
    group.append_last_child(&nested_item);
    let duplicate = store.create_content(&field, None);
    group.append_last_child(&duplicate);
    let master_item = store.create_content(&master, None);
    page_items[0].append_last_child(&master_item);
    runtime
        .with_script_context(|context| {
            let object = layout_object(&runtime, context)?;
            context.register_global_property(js_string!("layout"), object, Attribute::all())?;
            for (name, node) in [("field", &field), ("unlaid", &unlaid)] {
                let object = runtime.get_or_create_binding(node, context)?;
                context.register_global_property(
                    boa_engine::JsString::from(name),
                    object,
                    Attribute::all(),
                )?;
            }
            Ok(())
        })
        .unwrap();
    Fixture {
        doc,
        runtime,
        field,
        first,
        second,
    }
}
#[test]
fn dimensions_units_fragment_indices_and_actual_page_ranges_follow_source() {
    let f = fixture();
    assert!(f.runtime.eval(r#"
        function near(a,b){return Math.abs(a-b)<0.00001}
        if(!near(layout.w(field),72.123)||layout.h(field)!==36.5||!near(layout.x(field),10.001)||layout.y(field)!==20)throw 'geometry';
        if(!near(layout.w(field,'in'),1.002)||layout.h(field,'',1)!==18||layout.w(field,'pt',1)!==144||!near(layout.x(field,'pt',-2),10.001))throw 'fragment/unit';
        if(layout.h(field,'nonsense',9)!==0||layout.h(unlaid,'nonsense')!==undefined||layout.h({})!==undefined)throw 'no fragment';
        if(layout.page(field)!==1||layout.absPage(field)!==0||layout.sheet(field)!==0||layout.pageSpan(field)!==2||layout.absPageSpan(field)!==2)throw 'range';
        if(layout.page(unlaid)!==-1||layout.pageSpan(unlaid)!==-1||layout.page(null)!==0||layout.pageSpan(null)!==undefined)throw 'no page';
        if(layout.pageCount()!==1||layout.absPageCount()!==2||layout.sheetCount()!==2)throw 'page counts';
        let errors=0;for(const invoke of [()=>layout.h(),()=>layout.w(field,'bogus'),()=>layout.page(),()=>layout.pageSpan(field,0),()=>layout.pageContent()]){try{invoke()}catch(e){errors++}}
        errors===5
    "#).unwrap().to_boolean());
    assert_eq!(f.first.get_page().unwrap().get_page_index(), 0);
    assert_eq!(f.second.get_page().unwrap().get_page_index(), 1);
}
#[test]
fn page_content_lists_preserve_order_filter_master_nodes_and_deduplicate_fragments() {
    let f = fixture();
    assert!(f.runtime.eval(r#"
        function names(list){let a=[];for(let i=0;i<list.length;i++)a.push(list.item(i).name);return a.join(',')}
        if(names(layout.pageContent(0))!=='one,body,amount,label,section,nested')throw 'all content roots';
        if(names(layout.pageContent(0,'field'))!=='amount,nested')throw 'fields/dedup';
        if(names(layout.pageContent(0,'draw'))!=='label'||names(layout.pageContent(0,'subform'))!=='section')throw 'filter';
        if(names(layout.pageContent(0,'pageArea'))!=='one'||names(layout.pageContent(0,'contentArea'))!=='body')throw 'masters';
        if(names(layout.pageContent(0,'',true))!=='one,body,master'||names(layout.pageContent(0,'field',true))!=='master')throw 'master content';
        if(layout.pageContent(0,'invalid').length!==0||layout.pageContent(-1).length!==0||layout.pageContent(99).length!==0)throw 'empty';
        globalThis.saved=layout.pageContent(0,'field');saved.item(0)===field
    "#).unwrap().to_boolean());
    f.first.remove_self_if_parented();
    assert!(f
        .runtime
        .eval(
            "saved.length===2 && saved.item(0)===field && layout.pageContent(0,'field').length===2"
        )
        .unwrap()
        .to_boolean());
}
#[test]
fn ready_relayout_native_batch_noops_and_coercions_use_real_ff_services() {
    let f = fixture();
    assert_eq!(
        f.doc.get_doc_view().unwrap().layout_status(),
        LayoutStatus::None
    );
    assert!(f.runtime.eval(r#"
        if(layout.ready!==false)throw 'not ready';
        let coerced=false;let ignored={valueOf(){coerced=true;throw 'unexpected'}};
        if(layout.absPageCountInBatch(ignored)!==0||layout.sheetCountInBatch(ignored)!==0||layout.absPageInBatch(ignored)!==0||layout.sheetInBatch(ignored)!==0)throw 'native batches';
        if(layout.relayoutPageArea(ignored)!==undefined||coerced)throw 'native no-op';
        let rejected=false;try{layout.ready=ignored}catch(e){rejected=true}if(!rejected||coerced)throw 'readonly ready';
        let unit={toString(){layout.relayout();return 'in'}};
        layout.w(field,unit,1)===2
    "#).unwrap().to_boolean());
    assert!(f.doc.layout_processor().need_layout());
    f.runtime.set_layout_host(None);
    assert!(f
        .runtime
        .eval("layout.ready===undefined && layout.pageContent(0)===undefined")
        .unwrap()
        .to_boolean());
    assert!(f.runtime.eval("layout.absPageCount()").is_err());
    assert!(f
        .runtime
        .eval("layout.ready={toString(){throw 'coerced'}};true")
        .unwrap()
        .to_boolean());
    f.field.set_layout_item(None);
}
