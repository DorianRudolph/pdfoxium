// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2018 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.
//! Complete `xfa_basic_data_unittest.cpp` cases, followed by table integrity checks.
use super::*;
fn wide(name: &str) -> Vec<u32> {
    name.chars().map(u32::from).collect()
}
#[test]
fn get_packet_by_name_test() {
    assert!(get_packet_by_name(&wide("")).is_none());
    assert!(get_packet_by_name(&wide("nonesuch")).is_none());
    assert_eq!(
        get_packet_by_name(&wide("datasets")).unwrap().packet_type,
        XfaPacketType::Datasets
    );
    assert_eq!(
        get_packet_by_name(&wide("sourceSet")).unwrap().packet_type,
        XfaPacketType::SourceSet
    );
}
#[test]
fn packet_to_name() {
    assert_eq!(
        get_packet_by_index(XfaPacketType::Datasets).name,
        "datasets"
    );
    assert_eq!(
        get_packet_by_index(XfaPacketType::ConnectionSet).name,
        "connectionSet"
    );
}
#[test]
fn get_element_by_name_test() {
    assert_eq!(get_element_by_name(&wide("")), XfaElement::Unknown);
    assert_eq!(get_element_by_name(&wide("nonesuch")), XfaElement::Unknown);
    assert_eq!(
        get_element_by_name(&wide("connectionSet")),
        XfaElement::ConnectionSet
    );
    assert_eq!(get_element_by_name(&wide("items")), XfaElement::Items);
    assert_eq!(get_element_by_name(&wide("model")), XfaElement::Unknown);
}
#[test]
fn element_to_name_test() {
    assert_eq!(element_to_name(XfaElement::Conformance), "conformance");
    assert_eq!(element_to_name(XfaElement::Tagged), "tagged");
    assert_eq!(element_to_name(XfaElement::Node), "node");
}
#[test]
fn get_attribute_by_name_test() {
    assert!(get_attribute_by_name(&wide("")).is_none());
    assert!(get_attribute_by_name(&wide("nonesuch")).is_none());
    assert_eq!(
        get_attribute_by_name(&wide("h")).unwrap().attribute,
        XfaAttribute::H
    );
    assert_eq!(
        get_attribute_by_name(&wide("short")).unwrap().attribute,
        XfaAttribute::Short
    );
    assert_eq!(
        get_attribute_by_name(&wide("decipherOnly"))
            .unwrap()
            .attribute,
        XfaAttribute::DecipherOnly
    );
}
#[test]
fn attribute_to_name_test() {
    assert_eq!(attribute_to_name(XfaAttribute::SpaceBelow), "spaceBelow");
    assert_eq!(
        attribute_to_name(XfaAttribute::DecipherOnly),
        "decipherOnly"
    );
}
#[test]
fn get_attribute_value_by_name_test() {
    assert!(get_attribute_value_by_name(&wide("")).is_none());
    assert!(get_attribute_value_by_name(&wide("nonesuch")).is_none());
    assert_eq!(
        get_attribute_value_by_name(&wide("*")).unwrap(),
        XfaAttributeValue::Asterisk
    );
    assert_eq!(
        get_attribute_value_by_name(&wide("visible")).unwrap(),
        XfaAttributeValue::Visible
    );
    assert_eq!(
        get_attribute_value_by_name(&wide("lowered")).unwrap(),
        XfaAttributeValue::Lowered
    );
}
#[test]
fn attribute_value_to_name_test() {
    assert_eq!(attribute_value_to_name(XfaAttributeValue::Rl_tb), "rl-tb");
    assert_eq!(
        attribute_value_to_name(XfaAttributeValue::Lowered),
        "lowered"
    );
}
#[test]
fn complete_tables_and_ordinals() {
    assert_eq!(
        (
            PACKETS.len(),
            ELEMENTS.len(),
            ATTRIBUTES.len(),
            ATTRIBUTE_VALUES.len(),
            ELEMENT_ATTRIBUTES.len()
        ),
        (15, 320, 312, 266, 546)
    );
    for (i, &(hash, packet)) in PACKETS.iter().enumerate() {
        assert_eq!(packet.packet_type as usize, i);
        assert_eq!(get_packet_by_index(packet.packet_type), packet);
        assert_eq!(XfaPacketType::from_index(i), Some(packet.packet_type));
        if hash != u32::MAX {
            assert_eq!(hash_name(&wide(packet.name)), hash);
            assert_eq!(get_packet_by_name(&wide(packet.name)), Some(packet));
        }
    }
    for (i, record) in ELEMENTS.iter().enumerate() {
        assert_eq!(record.element as usize, i);
        assert_eq!(element_to_name(record.element), record.name);
        assert_eq!(XfaElement::from_i32(i as i32), Some(record.element));
        if record.hash != u32::MAX {
            assert_eq!(hash_name(&wide(record.name)), record.hash);
            assert_eq!(get_element_by_name(&wide(record.name)), record.element);
        }
        let mut parent = record.element;
        for depth in 0..=ELEMENTS.len() {
            if parent == XfaElement::Unknown {
                break;
            }
            assert!(depth < ELEMENTS.len(), "parent cycle");
            parent = element_parent(parent);
        }
    }
    for (i, record) in ATTRIBUTES.iter().enumerate() {
        assert_eq!(record.attribute as usize, i);
        assert_eq!(hash_name(&wide(record.name)), record.hash);
        assert_eq!(attribute_to_name(record.attribute), record.name);
        assert_eq!(
            get_attribute_by_name(&wide(record.name)),
            Some(AttributeInfo {
                attribute: record.attribute,
                script_type: record.script_type
            })
        );
        assert_eq!(XfaAttribute::from_index(i), Some(record.attribute));
    }
    for (i, record) in ATTRIBUTE_VALUES.iter().enumerate() {
        assert_eq!(record.value as usize, i);
        assert_eq!(hash_name(&wide(record.name)), record.hash);
        assert_eq!(attribute_value_to_name(record.value), record.name);
        assert_eq!(
            get_attribute_value_by_name(&wide(record.name)),
            Some(record.value)
        );
        assert_eq!(XfaAttributeValue::from_i32(i as i32), Some(record.value));
    }
    assert!(PACKETS.windows(2).all(|p| p[0].0 <= p[1].0));
    assert!(ELEMENTS.windows(2).all(|p| p[0].hash <= p[1].hash));
    assert!(ATTRIBUTES.windows(2).all(|p| p[0].hash <= p[1].hash));
    assert!(ATTRIBUTE_VALUES.windows(2).all(|p| p[0].hash <= p[1].hash));
    assert_eq!(XfaElement::from_i32(-1), Some(XfaElement::Unknown));
    assert_eq!(XfaAttribute::from_i32(-1), Some(XfaAttribute::Unknown));
    assert_eq!(XfaElement::from_i32(-2), None);
    assert_eq!(XfaAttributeValue::from_i32(-1), None);
    assert_eq!(XfaAttributeValue::from_index(ATTRIBUTE_VALUES.len()), None);
}
#[test]
fn hash_collisions_internal_names_and_unknowns() {
    for name in ["datasets", "items", "h", "visible"] {
        let ordinary = wide(name);
        let mut collision = vec![0];
        collision.extend_from_slice(&ordinary);
        assert_eq!(hash_name(&ordinary), hash_name(&collision));
        assert!(get_packet_by_name(&collision).is_none());
        assert_eq!(get_element_by_name(&collision), XfaElement::Unknown);
        assert!(get_attribute_by_name(&collision).is_none());
        assert!(get_attribute_value_by_name(&collision).is_none());
    }
    assert!(get_packet_by_name(&wide("user")).is_none());
    assert_eq!(get_packet_by_index(XfaPacketType::User).uri, None);
    for name in ["node", "model", "tree", "object", "******"] {
        assert_eq!(get_element_by_name(&wide(name)), XfaElement::Unknown);
    }
    assert_eq!(get_element_by_name(&[u32::MAX]), XfaElement::Unknown);
    assert!(get_packet_by_name(&[u32::MAX]).is_none());
    assert_eq!(element_to_name(XfaElement::Unknown), "");
    assert_eq!(attribute_to_name(XfaAttribute::Unknown), "");
    assert_eq!(element_parent(XfaElement::Unknown), XfaElement::Unknown);
}
#[test]
fn script_descriptor_metadata_and_parent_lookup() {
    assert!(ELEMENT_ATTRIBUTES
        .windows(2)
        .all(|p| (p[0].element, p[0].attribute) < (p[1].element, p[1].attribute)));
    for record in ELEMENT_ATTRIBUTES {
        let descriptor = find_script_attribute_descriptor(
            record.element,
            &wide(attribute_to_name(record.attribute)),
        )
        .unwrap();
        assert_eq!(descriptor.attribute, record.attribute);
        assert_eq!(descriptor.callback_name, record.callback_name);
    }
    assert!(find_script_attribute_descriptor(XfaElement::Unknown, &wide("h")).is_none());
    assert!(find_script_attribute_descriptor(XfaElement::Field, &wide("nonesuch")).is_none());
    let inherited = find_script_attribute_descriptor(XfaElement::Field, &wide("name")).unwrap();
    assert_eq!(inherited.attribute, XfaAttribute::Name);
    assert!(!inherited.callback_name.is_empty());
}

// Native FXFABasic hash cases expand the complete pinned .inc tables. These
// table literals retain every native hash/name pair, including sentinel rows.
fn hash_test_case(hash: u32, name: &str, so_far: &mut u32) {
    let actual = crate::fxcrt::bytestring::hash_code_as_if_w(name.as_bytes());
    if hash != u32::MAX {
        assert_eq!(hash, actual, "{name}");
        assert!(*so_far < hash, "hash order at {name}: {hash:#x}");
    } else {
        assert_ne!(hash, actual, "sentinel name {name}");
    }
    *so_far = hash;
}
#[test]
fn packet_hash_test() {
    let mut so_far = 0;
    for &(hash, packet) in PACKETS {
        hash_test_case(hash, packet.name, &mut so_far);
    }
}
#[test]
fn attribute_hash_test() {
    let mut so_far = 0;
    for record in ATTRIBUTES {
        hash_test_case(record.hash, record.name, &mut so_far);
    }
}
#[test]
fn value_hash_test() {
    let mut so_far = 0;
    for record in ATTRIBUTE_VALUES {
        hash_test_case(record.hash, record.name, &mut so_far);
    }
}
#[test]
fn element_hash_test() {
    let mut so_far = 0;
    for record in ELEMENTS {
        hash_test_case(record.hash, record.name, &mut so_far);
    }
}
