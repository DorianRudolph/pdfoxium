// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::cfx_xmldocument::CfxXmlDocument;
use super::cfx_xmlnode::XmlNodeType;
use super::test_support::{wide, StringWriteStream};
use crate::fxcrt::fx_stream::WriteStream;

#[test]
fn ownership_transfer_preserves_detached_nodes_and_nonowning_root_handles() {
    let mut destination = CfxXmlDocument::new();
    let mut source = CfxXmlDocument::new();
    let source_root = source.root();
    let parent = source.create_element(&wide("parent"));
    let text = source.create_text(&wide("child"));
    let detached = source.create_instruction(&wide("acrobat"));
    source_root.append_last_child(&parent);
    parent.append_last_child(&text);
    destination.append_nodes_from(&mut source);
    assert_eq!(source.root(), source_root);
    assert_eq!(source.root().first_child(), Some(parent.clone()));
    drop(source);
    assert!(source_root.is_alive());
    assert!(detached.is_alive());
    assert_eq!(text.parent(), Some(parent));
    assert_eq!(text.root(), source_root);
    drop(destination);
    assert!(!source_root.is_alive());
    assert!(!detached.is_alive());
    assert!(!text.is_alive());
}

#[test]
fn moved_text_and_clones_preserve_cow_lifetime_and_original_buffers() {
    let mut doc = CfxXmlDocument::new();
    let mut clones = CfxXmlDocument::new();
    let input = vec![b'a' as u32; 131_072];
    let input_pointer = input.as_ptr();
    let text = doc.create_text_owned(input);
    assert_eq!(text.text().as_ptr(), input_pointer);
    let cloned_text = text.clone_into(&mut clones);
    assert!(text.text().shares_storage_with(&cloned_text.text()));
    cloned_text.set_text(&wide("replacement"));
    assert_eq!(text.text().len(), 131_072);
    assert_eq!(text.text().as_ptr(), input_pointer);
    assert_eq!(cloned_text.text().as_ref(), wide("replacement"));
    let name = wide("long:element");
    let name_pointer = name.as_ptr();
    let element = doc.create_element_owned(name);
    assert_eq!(element.name().as_ptr(), name_pointer);
    let clone = element.clone_into(&mut clones);
    assert!(element.name().shares_storage_with(&clone.name()));
    let data = doc.create_chardata(&wide("CDATA"));
    let data_clone = data.clone_into(&mut clones);
    assert!(data.text().shares_storage_with(&data_clone.text()));
}

#[test]
fn element_clone_keeps_native_text_only_child_behavior_and_shared_attributes() {
    let mut doc = CfxXmlDocument::new();
    let element = doc.create_element(&wide("root"));
    element.set_attribute(&wide("a"), &wide("original"));
    let text = doc.create_text(&wide("text"));
    let data = doc.create_chardata(&wide("not cloned"));
    let child = doc.create_element(&wide("not-cloned"));
    element.append_last_child(&text);
    element.append_last_child(&data);
    element.append_last_child(&child);
    let clone = element.clone_into(&mut doc);
    assert_eq!(clone.first_child().unwrap().node_type(), XmlNodeType::Text);
    assert_eq!(clone.first_child().unwrap().next_sibling(), None);
    assert!(element
        .get_attribute(&wide("a"))
        .shares_storage_with(&clone.get_attribute(&wide("a"))));
    clone.set_attribute(&wide("a"), &wide("replacement"));
    assert_eq!(element.get_attribute(&wide("a")).as_ref(), wide("original"));
    assert_eq!(
        clone.get_attribute(&wide("a")).as_ref(),
        wide("replacement")
    );
}

struct CallbackWriter<F> {
    bytes: Vec<u8>,
    callback: F,
    succeeds: bool,
}
impl<F: FnMut(&[u8])> WriteStream for CallbackWriter<F> {
    fn write_block(&mut self, bytes: &[u8]) -> bool {
        self.bytes.extend_from_slice(bytes);
        (self.callback)(bytes);
        self.succeeds
    }
}

#[test]
fn serialization_releases_borrows_and_observes_later_attribute_and_tree_changes() {
    let mut doc = CfxXmlDocument::new();
    let element = doc.create_element(&wide("root"));
    element.set_attribute(&wide("a"), &wide("first"));
    element.set_attribute(&wide("z"), &wide("old"));
    let text = doc.create_text(&wide("child"));
    element.append_last_child(&text);
    let later = doc.create_text(&wide("later"));
    let mut writer = CallbackWriter {
        bytes: Vec::new(),
        succeeds: true,
        callback: |bytes: &[u8]| {
            if bytes == b" a=\"first\"" {
                element.set_attribute(&wide("m"), &wide("inserted"));
                element.set_attribute(&wide("z"), &wide("updated"));
            }
            if bytes == b"child" {
                element.append_last_child(&later);
            }
        },
    };
    element.save(&mut writer);
    assert_eq!(
        writer.bytes,
        b"<root a=\"first\" m=\"inserted\" z=\"updated\">childlater</root>"
    );
}

#[test]
fn text_cdata_and_instruction_keep_native_callback_evaluation_order() {
    let mut doc = CfxXmlDocument::new();
    let text = doc.create_text(&wide("old &"));
    let mut writer = CallbackWriter {
        bytes: Vec::new(),
        succeeds: false,
        callback: |_bytes: &[u8]| {
            text.set_text(&wide("new"));
        },
    };
    text.save(&mut writer);
    assert_eq!(writer.bytes, b"old &amp;");
    assert_eq!(text.text().as_ref(), wide("new"));
    let data = doc.create_chardata(&wide("old"));
    let mut writer = CallbackWriter {
        bytes: Vec::new(),
        succeeds: false,
        callback: |bytes: &[u8]| {
            if bytes == b"<![CDATA[" {
                data.set_text(&wide("new"));
            }
        },
    };
    data.save(&mut writer);
    assert_eq!(writer.bytes, b"<![CDATA[new]]>");
    let instruction = doc.create_instruction(&wide("acrobat"));
    let view = instruction.target_data();
    instruction.append_data(&wide("first"));
    assert_eq!(view.len(), 1);
    let mut writer = CallbackWriter {
        bytes: Vec::new(),
        succeeds: false,
        callback: |bytes: &[u8]| {
            if bytes == b"<?" {
                instruction.append_data(&wide("second"));
            }
        },
    };
    instruction.save(&mut writer);
    assert_eq!(writer.bytes, b"<?acrobat first second ?>");
    assert_eq!(view.len(), 2);
}

#[test]
fn namespace_lookup_walks_element_ancestors_and_default_undeclaration() {
    let mut doc = CfxXmlDocument::new();
    let parent = doc.create_element(&wide("parent"));
    let child = doc.create_element(&wide("p:child"));
    let default_child = doc.create_element(&wide("child"));
    parent.set_attribute(&wide("xmlns:p"), &wide("prefix"));
    parent.set_attribute(&wide("xmlns"), &wide("default"));
    parent.append_last_child(&child);
    parent.append_last_child(&default_child);
    assert_eq!(child.namespace_uri().as_ref(), wide("prefix"));
    assert_eq!(default_child.namespace_uri().as_ref(), wide("default"));
    default_child.set_attribute(&wide("xmlns"), &[]);
    assert!(default_child.namespace_uri().is_empty());
    let mut output = StringWriteStream::default();
    default_child.save(&mut output);
    assert_eq!(output.0, b"<child xmlns=\"\" />");
}

#[test]
fn raw_wchar32_attribute_names_keep_signed_order_for_lookup_removal_and_save() {
    let mut doc = CfxXmlDocument::new();
    let element = doc.create_element(&wide("root"));
    let negative_min = [0x8000_0000];
    let negative_next = [0x8000_0001];
    let negative_last = [0xffff_ffff];
    element.set_attribute(&wide("A"), &wide("ascii"));
    element.set_attribute(&negative_last, &wide("last-negative"));
    element.set_attribute(&negative_min, &wide("first-negative"));
    assert!(element.has_attribute(&negative_min));
    assert_eq!(
        element.get_attribute(&negative_min).as_ref(),
        wide("first-negative")
    );
    assert_eq!(
        element.get_attribute(&negative_last).as_ref(),
        wide("last-negative")
    );
    element.with_attributes(|attrs| {
        let order: Vec<_> = attrs.keys().map(|key| key[0]).collect();
        assert_eq!(order, [0x8000_0000, 0xffff_ffff, b'A' as u32]);
    });
    element.remove_attribute(&negative_last);
    assert!(!element.has_attribute(&negative_last));
    assert!(element.get_attribute(&negative_last).is_empty());
    let mut writer = CallbackWriter {
        bytes: Vec::new(),
        succeeds: true,
        callback: |bytes: &[u8]| {
            // Source UTF8Encode drops these raw signed wchar32 names. The
            // value still identifies their distinct ordered WriteString calls.
            if bytes == b" =\"first-negative\"" {
                element.set_attribute(&negative_next, &wide("inserted-negative"));
                element.set_attribute(&wide("A"), &wide("updated-ascii"));
            }
        },
    };
    element.save(&mut writer);
    assert_eq!(
        writer.bytes,
        b"<root =\"first-negative\" =\"inserted-negative\" A=\"updated-ascii\" />"
    );
    assert_eq!(
        element.get_attribute(&negative_next).as_ref(),
        wide("inserted-negative")
    );
}
