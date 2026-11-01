// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use super::*;
use std::collections::BTreeSet;

#[test]
fn every_source_factory_entry_has_complete_metadata() {
    // Compare the actual pinned factory rather than a second hand-maintained
    // element list. Intermediate/pseudo elements have no allocation case.
    let source = include_str!("../../../../../../upstream/pdfium/xfa/fxfa/parser/cxfa_node.cpp");
    let factory = source
        .split("CXFA_Node* CXFA_Node::Create(")
        .nth(1)
        .unwrap();
    let mut source_elements = BTreeSet::new();
    let mut pending_element = None;
    for line in factory.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix("case XFA_Element::") {
            pending_element = Some(name.trim_end_matches(':'));
        }
        if line.starts_with("node = cppgc::MakeGarbageCollected<CXFA_") {
            assert!(source_elements.insert(pending_element.take().unwrap().to_string()));
        }
    }
    let actual: BTreeSet<_> = SCHEMAS
        .iter()
        .map(|schema| format!("{:?}", schema.element))
        .collect();
    assert_eq!(actual, source_elements);
    assert_eq!(SCHEMAS.len(), 303);
    assert_eq!(
        SCHEMAS.iter().map(|s| s.properties.len()).sum::<usize>(),
        425
    );
    assert_eq!(
        SCHEMAS.iter().map(|s| s.attributes.len()).sum::<usize>(),
        1124
    );
    for schema in &SCHEMAS {
        assert_eq!(schema_for(schema.element), Some(schema));
        assert_ne!(schema.valid_packets, 0);
        assert_eq!(schema.valid_packets & !((1 << 15) - 1), 0);
        let mut properties = BTreeSet::new();
        for data in schema.properties {
            assert!(properties.insert(data.element));
            assert!(data.occurrence_count > 0);
            assert_eq!(data.flags & !(PROPERTY_ONE_OF | PROPERTY_DEFAULT_ONE_OF), 0);
        }
        let mut attributes = BTreeSet::new();
        for data in schema.attributes {
            assert!(attributes.insert(data.attribute));
            assert!(matches!(
                (data.attribute_type, data.default_value),
                (
                    XfaAttributeType::CData,
                    AttributeDefault::None | AttributeDefault::Text(_)
                ) | (XfaAttributeType::Boolean, AttributeDefault::Boolean(_))
                    | (XfaAttributeType::Integer, AttributeDefault::Integer(_))
                    | (XfaAttributeType::Enum, AttributeDefault::Enum(_))
                    | (XfaAttributeType::Measure, AttributeDefault::Measure(_))
            ));
        }
    }
    for element in [
        XfaElement::Unknown,
        XfaElement::List,
        XfaElement::TreeList,
        XfaElement::DataWindow,
        XfaElement::EventPseudoModel,
        XfaElement::Node,
        XfaElement::Object,
        XfaElement::Model,
    ] {
        assert!(schema_for(element).is_none());
    }
}

#[test]
fn constructor_packet_masks_include_intermediate_bases() {
    use XfaElement as E;
    use XfaPacketType as P;
    for element in [E::Arc, E::Rectangle, E::Border, E::Edge, E::Corner] {
        let schema = schema_for(element).unwrap();
        assert_eq!(schema.object_type, XfaObjectType::Node);
        assert_eq!(
            schema.valid_packets,
            packet_mask(P::Template) | packet_mask(P::Form)
        );
        assert!(schema.supports_packet(P::Template));
        assert!(schema.supports_packet(P::Form));
        assert!(!schema.supports_packet(P::Config));
        assert!(!schema.supports_packet(P::Datasets));
    }
    for (element, packet) in [
        (E::SourceSet, P::SourceSet),
        (E::DataModel, P::Datasets),
        (E::Xfa, P::Xdp),
        (E::XmlConnection, P::ConnectionSet),
        (E::Xdp, P::Config),
    ] {
        let schema = schema_for(element).unwrap();
        assert_eq!(schema.valid_packets, packet_mask(packet));
        assert!(schema.supports_packet(packet));
        assert!(!schema.supports_packet(P::User));
    }
    assert!(schema_for(E::Font).unwrap().supports_packet(P::Config));
    assert!(schema_for(E::Locale).unwrap().supports_packet(P::LocaleSet));
    assert!(!schema_for(E::Locale).unwrap().supports_packet(P::Template));
    assert_eq!(
        schema_for(E::Subform).unwrap().object_type,
        XfaObjectType::ContainerNode
    );
    assert_eq!(
        schema_for(E::DataModel).unwrap().object_type,
        XfaObjectType::ModelNode
    );
    assert_eq!(
        schema_for(E::To).unwrap().object_type,
        XfaObjectType::ContentNode
    );
}

#[test]
fn numeric_null_text_enum_and_measurement_defaults_remain_distinct() {
    use AttributeDefault as D;
    use XfaAttribute as A;
    use XfaAttributeValue as V;
    use XfaElement as E;
    for (element, attribute, expected) in [
        (E::Data, A::Lock, D::Integer(0)),
        (E::Occur, A::Initial, D::Integer(1)),
        (E::Decimal, A::LeadDigits, D::Integer(-1)),
        (E::ExData, A::MaxLength, D::Integer(-1)),
        (E::Value, A::Override, D::Boolean(false)),
        // Equate uses an actual nullptr rather than a cast integer zero.
        (E::Equate, A::Force, D::Boolean(false)),
        (E::Font, A::Typeface, D::Text("Courier")),
        (E::Font, A::FontHorizontalScale, D::Text("100%")),
        (E::Barcode, A::Type, D::None),
        (E::Barcode, A::WideNarrowRatio, D::Text("3:1")),
        (E::Barcode, A::ModuleWidth, D::Measure("0.25mm")),
        (E::Edge, A::Thickness, D::Measure("0.5pt")),
        (E::Subform, A::W, D::Measure("0in")),
        (E::Border, A::Break, D::Enum(V::Close)),
        (E::Subform, A::Layout, D::Enum(V::Position)),
        (E::Barcode, A::Checksum, D::Enum(V::None)),
    ] {
        assert_eq!(
            schema_for(element)
                .unwrap()
                .attribute(attribute)
                .unwrap()
                .default_value,
            expected,
            "{element:?}.{attribute:?}"
        );
    }
    // A declared null CData default is distinguishable from an absent attribute.
    assert!(schema_for(E::Barcode).unwrap().attribute(A::Type).is_some());
    assert!(schema_for(E::Barcode)
        .unwrap()
        .attribute(A::Typeface)
        .is_none());
}

#[test]
fn property_occurrences_oneof_and_default_oneof_follow_source_tables() {
    use XfaElement as E;
    for (element, child, expected) in [
        (E::Border, E::Edge, 4),
        (E::Border, E::Corner, 4),
        (E::DayNames, E::Day, 7),
        (E::MonthNames, E::Month, 12),
        (E::NumberSymbols, E::NumberSymbol, 5),
    ] {
        assert_eq!(
            schema_for(element)
                .unwrap()
                .property(child)
                .unwrap()
                .occurrence_count,
            expected
        );
    }
    let ui = schema_for(E::Ui).unwrap();
    assert_eq!(ui.property(E::TextEdit).unwrap().flags, PROPERTY_ONE_OF);
    assert_eq!(ui.property(E::Picture).unwrap().flags, 0);
    let fill = schema_for(E::Fill).unwrap();
    assert_eq!(
        fill.property(E::Solid).unwrap().flags,
        PROPERTY_ONE_OF | PROPERTY_DEFAULT_ONE_OF
    );
    assert_eq!(fill.property(E::Linear).unwrap().flags, PROPERTY_ONE_OF);
    assert!(fill.property(E::Field).is_none());
}
