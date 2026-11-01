// Copyright 2026 The pdfoxium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Source coordinator transitions, actual model/page synchronization and
//! callback-safe root processor ownership.
use super::*;
use crate::xfa::fxfa::test_support::new_document;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn missing_form_returns_native_failure_and_weak_document_link_does_not_own_controller() {
    let document = CxfaDocument::new();
    let processor = CxfaLayoutProcessor::new(&document);
    assert!(Rc::ptr_eq(
        &CxfaLayoutProcessor::from_document(&document).unwrap(),
        &processor
    ));
    assert!(processor.need_layout());
    assert_eq!(processor.do_layout().unwrap(), -1);
    assert_eq!(processor.start_layout().unwrap(), -1);
    assert!(!processor.increment_layout().unwrap());
    assert_eq!(processor.count_pages(), 0);
    let weak = Rc::downgrade(&processor);
    drop(processor);
    assert!(weak.upgrade().is_none());
    assert!(CxfaLayoutProcessor::from_document(&document).is_none());
}
fn document() -> Rc<crate::xfa::fxfa::cxfa_ffdoc::CxfaFfDoc> {
    let (doc, _) = new_document(
        r#"<xdp:xdp xmlns:xdp="http://ns.adobe.com/xdp/"><template xmlns="http://www.xfa.org/schema/xfa-template/3.3/"><subform name="root" layout="tb" x="7pt" y="9pt"><pageSet><pageArea name="page"><occur min="1" max="-1"/><contentArea w="100pt" h="50pt"/></pageArea></pageSet><subform name="first" w="20pt" h="30pt"/><subform name="second" w="25pt" h="30pt"/></subform></template></xdp:xdp>"#,
    );
    doc.create_doc_view();
    doc.model_document()
        .do_data_merge_with_resolver(doc.script_context().resolver())
        .unwrap();
    doc.layout_processor().set_content_host(Some(doc.notify()));
    doc
}
#[test]
fn actual_model_pagination_submits_all_fragments_and_increment_preserves_native_state_rules() {
    let doc = document();
    let processor = doc.layout_processor();
    assert_eq!(processor.start_layout().unwrap(), 0);
    let root = processor
        .with_root_content_layout_processor(|content| content.unwrap().form_node())
        .unwrap();
    assert_eq!(processor.do_layout().unwrap(), 100);
    assert_eq!(processor.count_pages(), 2);
    let first = root
        .get_layout_item()
        .unwrap()
        .as_content_layout_item()
        .unwrap();
    let second = first.next().unwrap();
    assert_eq!(first.position(), CfxPointF::new(7., 9.));
    assert_eq!(second.position(), CfxPointF::new(7., 9.));
    assert_eq!(
        first
            .first_child()
            .unwrap()
            .form_node()
            .get_cdata(XfaAttribute::Name),
        "first".chars().map(u32::from).collect::<Vec<_>>()
    );
    assert_eq!(
        second
            .first_child()
            .unwrap()
            .form_node()
            .get_cdata(XfaAttribute::Name),
        "second".chars().map(u32::from).collect::<Vec<_>>()
    );
    assert!(second.next().is_none());
    assert!(!processor.need_layout());
    assert_eq!(processor.start_layout().unwrap(), 100);
    assert!(processor.increment_layout().unwrap());
    processor.set_has_changed_container();
    assert!(processor.need_layout());
    assert!(!processor.increment_layout().unwrap());
    processor.set_force_relayout();
    assert!(processor.increment_layout().unwrap());
    assert!(!processor.need_layout());
    assert_eq!(processor.count_pages(), 2);
}
#[test]
fn scoped_root_content_rejects_reentry_and_restores_owned_processor_after_unwinding() {
    let doc = document();
    let processor = doc.layout_processor();
    assert_eq!(processor.start_layout().unwrap(), 0);
    let root = processor
        .with_root_content_layout_processor(|content| {
            let root = content.unwrap().form_node();
            assert!(matches!(
                processor.do_layout(),
                Err(LayoutProcessorError::Reentrant)
            ));
            assert!(matches!(
                processor.restart_layout(),
                Err(LayoutProcessorError::Reentrant)
            ));
            root
        })
        .unwrap();
    assert!(catch_unwind(AssertUnwindSafe(|| {
        processor
            .with_root_content_layout_processor(|_| panic!("host callback unwind"))
            .unwrap();
    }))
    .is_err());
    assert_eq!(
        processor
            .with_root_content_layout_processor(|content| content.unwrap().form_node())
            .unwrap(),
        root
    );
    assert_eq!(processor.do_layout().unwrap(), 100);
}
