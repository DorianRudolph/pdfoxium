// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

// Direct constructor/table translation: 303 concrete XML node constructors,
// 425 property entries and 1124 attribute entries. CXFA_Box, CXFA_Stroke and
// CXFA_Rectangle forward metadata unchanged through their intermediate bases.
use AttributeDefault as D;
use XfaAttribute as A;
use XfaAttributeType as T;
use XfaAttributeValue as V;
use XfaElement as E;
use XfaPacketType as P;

static SCHEMAS: [NodeSchema; 303] = [
    // cxfa_ps.cpp: CXFA_Ps via CXFA_Node.
    NodeSchema {
        element: E::Ps,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::FontInfo, 1, 0),
            property(E::Jog, 1, 0),
            property(E::Xdc, 1, 0),
            property(E::BatchOutput, 1, 0),
            property(E::OutputBin, 1, 0),
            property(E::Compress, 1, 0),
            property(E::Staple, 1, 0),
            property(E::MediumInfo, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_to.cpp: CXFA_To via CXFA_Node.
    NodeSchema {
        element: E::To,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_ui.cpp: CXFA_Ui via CXFA_Node.
    NodeSchema {
        element: E::Ui,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::CheckButton, 1, 1),
            property(E::ChoiceList, 1, 1),
            property(E::DefaultUi, 1, 1),
            property(E::Barcode, 1, 1),
            property(E::Button, 1, 1),
            property(E::DateTimeEdit, 1, 1),
            property(E::Picture, 1, 0),
            property(E::ImageEdit, 1, 1),
            property(E::PasswordEdit, 1, 1),
            property(E::NumericEdit, 1, 1),
            property(E::Signature, 1, 1),
            property(E::TextEdit, 1, 1),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_recordset.cpp: CXFA_RecordSet via CXFA_Node.
    NodeSchema {
        element: E::RecordSet,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Max, T::Integer, D::Integer(0)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::EofAction, T::Enum, D::Enum(V::MoveLast)),
            attribute(A::CursorType, T::Enum, D::Enum(V::ForwardOnly)),
            attribute(A::LockType, T::Enum, D::Enum(V::ReadOnly)),
            attribute(A::BofAction, T::Enum, D::Enum(V::MoveFirst)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::CursorLocation, T::Enum, D::Enum(V::Client)),
        ],
    },
    // cxfa_subsetbelow.cpp: CXFA_SubsetBelow via CXFA_Node.
    NodeSchema {
        element: E::SubsetBelow,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_subformset.cpp: CXFA_SubformSet via CXFA_Node.
    NodeSchema {
        element: E::SubformSet,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Break, 1, 0),
            property(E::Overflow, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Bookend, 1, 0),
            property(E::Extras, 1, 0),
            property(E::Occur, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Relation, T::Enum, D::Enum(V::Ordered)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_adobeextensionlevel.cpp: CXFA_AdobeExtensionLevel via CXFA_Node.
    NodeSchema {
        element: E::AdobeExtensionLevel,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_typeface.cpp: CXFA_Typeface via CXFA_Node.
    NodeSchema {
        element: E::Typeface,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::CData, D::None)],
    },
    // cxfa_break.cpp: CXFA_Break via CXFA_Node.
    NodeSchema {
        element: E::Break,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::BeforeTarget, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::OverflowTarget, T::CData, D::None),
            attribute(A::OverflowLeader, T::CData, D::None),
            attribute(A::OverflowTrailer, T::CData, D::None),
            attribute(A::StartNew, T::Boolean, D::Boolean(false)),
            attribute(A::BookendTrailer, T::CData, D::None),
            attribute(A::After, T::Enum, D::Enum(V::Auto)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::BookendLeader, T::CData, D::None),
            attribute(A::AfterTarget, T::CData, D::None),
            attribute(A::Before, T::Enum, D::Enum(V::Auto)),
        ],
    },
    // cxfa_fontinfo.cpp: CXFA_FontInfo via CXFA_Node.
    NodeSchema {
        element: E::FontInfo,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::SubsetBelow, 1, 0),
            property(E::Map, 1, 0),
            property(E::Embed, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_numberpattern.cpp: CXFA_NumberPattern via CXFA_Node.
    NodeSchema {
        element: E::NumberPattern,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Numeric))],
    },
    // cxfa_dynamicrender.cpp: CXFA_DynamicRender via CXFA_Node.
    NodeSchema {
        element: E::DynamicRender,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_printscaling.cpp: CXFA_PrintScaling via CXFA_Node.
    NodeSchema {
        element: E::PrintScaling,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_checkbutton.cpp: CXFA_CheckButton via CXFA_Node.
    NodeSchema {
        element: E::CheckButton,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::AllowNeutral, T::Boolean, D::Boolean(false)),
            attribute(A::Mark, T::Enum, D::Enum(V::Default)),
            attribute(A::Shape, T::Enum, D::Enum(V::Square)),
            attribute(A::Size, T::Measure, D::Measure("10pt")),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_datepatterns.cpp: CXFA_DatePatterns via CXFA_Node.
    NodeSchema {
        element: E::DatePatterns,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::DatePattern, 4, 0)],
        attributes: &[],
    },
    // cxfa_sourceset.cpp: CXFA_SourceSet via CXFA_Node.
    NodeSchema {
        element: E::SourceSet,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_amd.cpp: CXFA_Amd via CXFA_Node.
    NodeSchema {
        element: E::Amd,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_arc.cpp: CXFA_Arc via CXFA_Box.
    NodeSchema {
        element: E::Arc,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Edge, 1, 0), property(E::Fill, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::StartAngle, T::Integer, D::Integer(0)),
            attribute(A::SweepAngle, T::Integer, D::Integer(360)),
            attribute(A::Circular, T::Boolean, D::Boolean(false)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Hand, T::Enum, D::Enum(V::Even)),
        ],
    },
    // cxfa_day.cpp: CXFA_Day via CXFA_Node.
    NodeSchema {
        element: E::Day,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_era.cpp: CXFA_Era via CXFA_Node.
    NodeSchema {
        element: E::Era,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_jog.cpp: CXFA_Jog via CXFA_Node.
    NodeSchema {
        element: E::Jog,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_log.cpp: CXFA_Log via CXFA_Node.
    NodeSchema {
        element: E::Log,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::To, 1, 0),
            property(E::Uri, 1, 0),
            property(E::Mode, 1, 0),
            property(E::Threshold, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_map.cpp: CXFA_Map via CXFA_Node.
    NodeSchema {
        element: E::Map,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config) | packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Bind, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::From, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_mdp.cpp: CXFA_Mdp via CXFA_Node.
    NodeSchema {
        element: E::Mdp,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::SignatureType, T::Enum, D::Enum(V::Filter)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Permissions, T::Integer, D::Integer(2)),
        ],
    },
    // cxfa_breakbefore.cpp: CXFA_BreakBefore via CXFA_Node.
    NodeSchema {
        element: E::BreakBefore,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Script, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::StartNew, T::Boolean, D::Boolean(false)),
            attribute(A::Trailer, T::CData, D::None),
            attribute(A::TargetType, T::Enum, D::Enum(V::Auto)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
            attribute(A::Leader, T::CData, D::None),
        ],
    },
    // cxfa_oid.cpp: CXFA_Oid via CXFA_Node.
    NodeSchema {
        element: E::Oid,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_pcl.cpp: CXFA_Pcl via CXFA_Node.
    NodeSchema {
        element: E::Pcl,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::FontInfo, 1, 0),
            property(E::Jog, 1, 0),
            property(E::Xdc, 1, 0),
            property(E::BatchOutput, 1, 0),
            property(E::PageOffset, 1, 0),
            property(E::OutputBin, 1, 0),
            property(E::Staple, 1, 0),
            property(E::MediumInfo, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pdf.cpp: CXFA_Pdf via CXFA_Node.
    NodeSchema {
        element: E::Pdf,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::AdobeExtensionLevel, 1, 0),
            property(E::FontInfo, 1, 0),
            property(E::Xdc, 1, 0),
            property(E::Pdfa, 1, 0),
            property(E::BatchOutput, 1, 0),
            property(E::ViewerPreferences, 1, 0),
            property(E::ScriptModel, 1, 0),
            property(E::Version, 1, 0),
            property(E::SubmitFormat, 1, 0),
            property(E::SilentPrint, 1, 0),
            property(E::Producer, 1, 0),
            property(E::Compression, 1, 0),
            property(E::Interactive, 1, 0),
            property(E::Encryption, 1, 0),
            property(E::RenderPolicy, 1, 0),
            property(E::OpenAction, 1, 0),
            property(E::Creator, 1, 0),
            property(E::Linearized, 1, 0),
            property(E::Tagged, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_ref.cpp: CXFA_Ref via CXFA_Node.
    NodeSchema {
        element: E::Ref,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_uri.cpp: CXFA_Uri via CXFA_Node.
    NodeSchema {
        element: E::Uri,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_xdc.cpp: CXFA_Xdc via CXFA_Node.
    NodeSchema {
        element: E::Xdc,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Xdc),
        properties: &[property(E::Uri, 1, 0), property(E::Xsl, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_xdp.cpp: CXFA_Xdp via CXFA_Node.
    NodeSchema {
        element: E::Xdp,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Packets, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_xfa.cpp: CXFA_Xfa via CXFA_Node.
    NodeSchema {
        element: E::Xfa,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Xdp),
        properties: &[],
        attributes: &[
            attribute(A::TimeStamp, T::CData, D::None),
            attribute(A::Uuid, T::CData, D::None),
        ],
    },
    // cxfa_xsl.cpp: CXFA_Xsl via CXFA_Node.
    NodeSchema {
        element: E::Xsl,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Uri, 1, 0), property(E::Debug, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_zpl.cpp: CXFA_Zpl via CXFA_Node.
    NodeSchema {
        element: E::Zpl,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::FontInfo, 1, 0),
            property(E::Xdc, 1, 0),
            property(E::BatchOutput, 1, 0),
            property(E::FlipLabel, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_cache.cpp: CXFA_Cache via CXFA_Node.
    NodeSchema {
        element: E::Cache,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::TemplateCache, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_margin.cpp: CXFA_Margin via CXFA_Node.
    NodeSchema {
        element: E::Margin,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::LeftInset, T::Measure, D::Measure("0in")),
            attribute(A::BottomInset, T::Measure, D::Measure("0in")),
            attribute(A::TopInset, T::Measure, D::Measure("0in")),
            attribute(A::RightInset, T::Measure, D::Measure("0in")),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_keyusage.cpp: CXFA_KeyUsage via CXFA_Node.
    NodeSchema {
        element: E::KeyUsage,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::NonRepudiation, T::CData, D::None),
            attribute(A::EncipherOnly, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::DigitalSignature, T::CData, D::None),
            attribute(A::CrlSign, T::CData, D::None),
            attribute(A::KeyAgreement, T::CData, D::None),
            attribute(A::KeyEncipherment, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::DataEncipherment, T::CData, D::None),
            attribute(A::KeyCertSign, T::CData, D::None),
            attribute(A::DecipherOnly, T::CData, D::None),
        ],
    },
    // cxfa_exclude.cpp: CXFA_Exclude via CXFA_Node.
    NodeSchema {
        element: E::Exclude,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_choicelist.cpp: CXFA_ChoiceList via CXFA_Node.
    NodeSchema {
        element: E::ChoiceList,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Open, T::Enum, D::Enum(V::UserControl)),
            attribute(A::CommitOn, T::Enum, D::Enum(V::Select)),
            attribute(A::TextEntry, T::Boolean, D::Boolean(false)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_level.cpp: CXFA_Level via CXFA_Node.
    NodeSchema {
        element: E::Level,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_labelprinter.cpp: CXFA_LabelPrinter via CXFA_Node.
    NodeSchema {
        element: E::LabelPrinter,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::FontInfo, 1, 0),
            property(E::Xdc, 1, 0),
            property(E::BatchOutput, 1, 0),
            property(E::FlipLabel, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::Enum, D::Enum(V::Zpl)),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_calendarsymbols.cpp: CXFA_CalendarSymbols via CXFA_Node.
    NodeSchema {
        element: E::CalendarSymbols,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[
            property(E::EraNames, 1, 0),
            property(E::DayNames, 2, 0),
            property(E::MeridiemNames, 1, 0),
            property(E::MonthNames, 2, 0),
        ],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Gregorian))],
    },
    // cxfa_para.cpp: CXFA_Para via CXFA_Node.
    NodeSchema {
        element: E::Para,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Hyphenation, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::HAlign, T::Enum, D::Enum(V::Left)),
            attribute(A::TextIndent, T::Measure, D::Measure("0in")),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Widows, T::Integer, D::Integer(0)),
            attribute(A::MarginRight, T::Measure, D::Measure("0in")),
            attribute(A::MarginLeft, T::Measure, D::Measure("0in")),
            attribute(A::RadixOffset, T::Measure, D::Measure("0in")),
            attribute(A::Preserve, T::CData, D::None),
            attribute(A::SpaceBelow, T::Measure, D::Measure("0in")),
            attribute(A::VAlign, T::Enum, D::Enum(V::Top)),
            attribute(A::TabDefault, T::CData, D::None),
            attribute(A::TabStops, T::CData, D::None),
            attribute(A::Orphans, T::Integer, D::Integer(0)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::LineHeight, T::Measure, D::Measure("0pt")),
            attribute(A::SpaceAbove, T::Measure, D::Measure("0in")),
        ],
    },
    // cxfa_part.cpp: CXFA_Part via CXFA_Node.
    NodeSchema {
        element: E::Part,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pdfa.cpp: CXFA_Pdfa via CXFA_Node.
    NodeSchema {
        element: E::Pdfa,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Amd, 1, 0),
            property(E::Part, 1, 0),
            property(E::IncludeXDPContent, 1, 0),
            property(E::Conformance, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_filter.cpp: CXFA_Filter via CXFA_Node.
    NodeSchema {
        element: E::Filter,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Mdp, 1, 0),
            property(E::Certificates, 1, 0),
            property(E::TimeStamp, 1, 0),
            property(E::Handler, 1, 0),
            property(E::DigestMethods, 1, 0),
            property(E::Encodings, 1, 0),
            property(E::Reasons, 1, 0),
            property(E::AppearanceFilter, 1, 0),
            property(E::LockDocument, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Version, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::AddRevocationInfo, T::CData, D::None),
        ],
    },
    // cxfa_present.cpp: CXFA_Present via CXFA_Node.
    NodeSchema {
        element: E::Present,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Xdp, 1, 0),
            property(E::Cache, 1, 0),
            property(E::Pagination, 1, 0),
            property(E::Overprint, 1, 0),
            property(E::BehaviorOverride, 1, 0),
            property(E::Copies, 1, 0),
            property(E::Output, 1, 0),
            property(E::Validate, 1, 0),
            property(E::Layout, 1, 0),
            property(E::Script, 1, 0),
            property(E::Common, 1, 0),
            property(E::PaginationOverride, 1, 0),
            property(E::Destination, 1, 0),
            property(E::IncrementalMerge, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pagination.cpp: CXFA_Pagination via CXFA_Node.
    NodeSchema {
        element: E::Pagination,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_encoding.cpp: CXFA_Encoding via CXFA_Node.
    NodeSchema {
        element: E::Encoding,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_event.cpp: CXFA_Event via CXFA_Node.
    NodeSchema {
        element: E::Event,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Execute, 1, 1),
            property(E::Script, 1, 1),
            property(E::SignData, 1, 1),
            property(E::Extras, 1, 0),
            property(E::Submit, 1, 1),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Listen, T::Enum, D::Enum(V::RefOnly)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Activity, T::Enum, D::Enum(V::Click)),
        ],
    },
    // cxfa_whitespace.cpp: CXFA_Whitespace via CXFA_Node.
    NodeSchema {
        element: E::Whitespace,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_defaultui.cpp: CXFA_DefaultUi via CXFA_Node.
    NodeSchema {
        element: E::DefaultUi,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_datamodel.cpp: CXFA_DataModel via CXFA_Node.
    NodeSchema {
        element: E::DataModel,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Datasets),
        properties: &[],
        attributes: &[],
    },
    // cxfa_barcode.cpp: CXFA_Barcode via CXFA_Node.
    NodeSchema {
        element: E::Barcode,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::DataRowCount, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::DataPrep, T::Enum, D::Enum(V::None)),
            attribute(A::Type, T::CData, D::None),
            attribute(A::TextLocation, T::Enum, D::Enum(V::Below)),
            attribute(A::ModuleWidth, T::Measure, D::Measure("0.25mm")),
            attribute(A::PrintCheckDigit, T::Boolean, D::Boolean(false)),
            attribute(A::ModuleHeight, T::Measure, D::Measure("5mm")),
            attribute(A::StartChar, T::CData, D::None),
            attribute(A::Truncate, T::Boolean, D::Boolean(false)),
            attribute(A::WideNarrowRatio, T::CData, D::Text("3:1")),
            attribute(A::ErrorCorrectionLevel, T::CData, D::None),
            attribute(A::UpsMode, T::Enum, D::Enum(V::UsCarrier)),
            attribute(A::Checksum, T::Enum, D::Enum(V::None)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::DataColumnCount, T::CData, D::None),
            attribute(A::RowColumnRatio, T::CData, D::None),
            attribute(A::DataLength, T::CData, D::None),
            attribute(A::EndChar, T::CData, D::None),
        ],
    },
    // cxfa_timepattern.cpp: CXFA_TimePattern via CXFA_Node.
    NodeSchema {
        element: E::TimePattern,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Med))],
    },
    // cxfa_batchoutput.cpp: CXFA_BatchOutput via CXFA_Node.
    NodeSchema {
        element: E::BatchOutput,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Format, T::Enum, D::Enum(V::None)),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_enforce.cpp: CXFA_Enforce via CXFA_Node.
    NodeSchema {
        element: E::Enforce,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_currencysymbols.cpp: CXFA_CurrencySymbols via CXFA_Node.
    NodeSchema {
        element: E::CurrencySymbols,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::CurrencySymbol, 3, 0)],
        attributes: &[],
    },
    // cxfa_addsilentprint.cpp: CXFA_AddSilentPrint via CXFA_Node.
    NodeSchema {
        element: E::AddSilentPrint,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_rename.cpp: CXFA_Rename via CXFA_Node.
    NodeSchema {
        element: E::Rename,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_operation.cpp: CXFA_Operation via CXFA_Node.
    NodeSchema {
        element: E::Operation,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Output, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Input, T::CData, D::None),
        ],
    },
    // cxfa_typefaces.cpp: CXFA_Typefaces via CXFA_Node.
    NodeSchema {
        element: E::Typefaces,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_subjectdns.cpp: CXFA_SubjectDNs via CXFA_Node.
    NodeSchema {
        element: E::SubjectDNs,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_issuers.cpp: CXFA_Issuers via CXFA_Node.
    NodeSchema {
        element: E::Issuers,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_wsdlconnection.cpp: CXFA_WsdlConnection via CXFA_Node.
    NodeSchema {
        element: E::WsdlConnection,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[
            property(E::Operation, 1, 0),
            property(E::WsdlAddress, 1, 0),
            property(E::SoapAddress, 1, 0),
            property(E::SoapAction, 1, 0),
            property(E::EffectiveOutputPolicy, 1, 0),
            property(E::EffectiveInputPolicy, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::DataDescription, T::CData, D::None),
        ],
    },
    // cxfa_debug.cpp: CXFA_Debug via CXFA_Node.
    NodeSchema {
        element: E::Debug,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Uri, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_delta.cpp: CXFA_Delta via CXFA_Node.
    NodeSchema {
        element: E::Delta,
        object_type: XfaObjectType::Object,
        valid_packets: packet_mask(P::Form),
        properties: &[],
        attributes: &[],
    },
    // cxfa_eranames.cpp: CXFA_EraNames via CXFA_Node.
    NodeSchema {
        element: E::EraNames,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::Era, 2, 0)],
        attributes: &[],
    },
    // cxfa_modifyannots.cpp: CXFA_ModifyAnnots via CXFA_Node.
    NodeSchema {
        element: E::ModifyAnnots,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_startnode.cpp: CXFA_StartNode via CXFA_Node.
    NodeSchema {
        element: E::StartNode,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_button.cpp: CXFA_Button via CXFA_Node.
    NodeSchema {
        element: E::Button,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Highlight, T::Enum, D::Enum(V::Inverted)),
        ],
    },
    // cxfa_format.cpp: CXFA_Format via CXFA_Node.
    NodeSchema {
        element: E::Format,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Picture, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_border.cpp: CXFA_Border via CXFA_Rectangle.
    NodeSchema {
        element: E::Border,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Edge, 4, 0),
            property(E::Corner, 4, 0),
            property(E::Fill, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Break, T::Enum, D::Enum(V::Close)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Hand, T::Enum, D::Enum(V::Even)),
        ],
    },
    // cxfa_area.cpp: CXFA_Area via CXFA_Node.
    NodeSchema {
        element: E::Area,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Desc, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Level, T::Integer, D::Integer(0)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::ColSpan, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_hyphenation.cpp: CXFA_Hyphenation via CXFA_Node.
    NodeSchema {
        element: E::Hyphenation,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::WordCharacterCount, T::Integer, D::Integer(7)),
            attribute(A::Hyphenate, T::Boolean, D::Boolean(false)),
            attribute(A::ExcludeInitialCap, T::Boolean, D::Boolean(false)),
            attribute(A::PushCharacterCount, T::Integer, D::Integer(3)),
            attribute(A::RemainCharacterCount, T::Integer, D::Integer(3)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::ExcludeAllCaps, T::Boolean, D::Boolean(false)),
        ],
    },
    // cxfa_text.cpp: CXFA_Text via CXFA_Node.
    NodeSchema {
        element: E::Text,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Rid, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::MaxChars, T::Integer, D::Integer(0)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_time.cpp: CXFA_Time via CXFA_Node.
    NodeSchema {
        element: E::Time,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_type.cpp: CXFA_Type via CXFA_Node.
    NodeSchema {
        element: E::Type,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_overprint.cpp: CXFA_Overprint via CXFA_Node.
    NodeSchema {
        element: E::Overprint,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_certificates.cpp: CXFA_Certificates via CXFA_Node.
    NodeSchema {
        element: E::Certificates,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::KeyUsage, 1, 0),
            property(E::SubjectDNs, 1, 0),
            property(E::Issuers, 1, 0),
            property(E::Signing, 1, 0),
            property(E::Oids, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Url, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::CredentialServerPolicy, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::UrlPolicy, T::CData, D::None),
        ],
    },
    // cxfa_encryptionmethods.cpp: CXFA_EncryptionMethods via CXFA_Node.
    NodeSchema {
        element: E::EncryptionMethods,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_setproperty.cpp: CXFA_SetProperty via CXFA_Node.
    NodeSchema {
        element: E::SetProperty,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Connection, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
        ],
    },
    // cxfa_printername.cpp: CXFA_PrinterName via CXFA_Node.
    NodeSchema {
        element: E::PrinterName,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_startpage.cpp: CXFA_StartPage via CXFA_Node.
    NodeSchema {
        element: E::StartPage,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pageoffset.cpp: CXFA_PageOffset via CXFA_Node.
    NodeSchema {
        element: E::PageOffset,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_datetime.cpp: CXFA_DateTime via CXFA_Node.
    NodeSchema {
        element: E::DateTime,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_comb.cpp: CXFA_Comb via CXFA_Node.
    NodeSchema {
        element: E::Comb,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::NumberOfCells, T::Integer, D::Integer(0)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_pattern.cpp: CXFA_Pattern via CXFA_Node.
    NodeSchema {
        element: E::Pattern,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::CrossHatch)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_ifempty.cpp: CXFA_IfEmpty via CXFA_Node.
    NodeSchema {
        element: E::IfEmpty,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_suppressbanner.cpp: CXFA_SuppressBanner via CXFA_Node.
    NodeSchema {
        element: E::SuppressBanner,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_outputbin.cpp: CXFA_OutputBin via CXFA_Node.
    NodeSchema {
        element: E::OutputBin,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_field.cpp: CXFA_Field via CXFA_Node.
    NodeSchema {
        element: E::Field,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Ui, 1, 0),
            property(E::Margin, 1, 0),
            property(E::Para, 1, 0),
            property(E::Format, 1, 0),
            property(E::Border, 1, 0),
            property(E::Assist, 1, 0),
            property(E::Traversal, 1, 0),
            property(E::Keep, 1, 0),
            property(E::Validate, 1, 0),
            property(E::Caption, 1, 0),
            property(E::Bind, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Font, 1, 0),
            property(E::Value, 1, 0),
            property(E::Calculate, 1, 0),
            property(E::Extras, 1, 0),
            property(E::Items, 2, 0),
        ],
        attributes: &[
            attribute(A::H, T::Measure, D::Measure("0in")),
            attribute(A::W, T::Measure, D::Measure("0in")),
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::HAlign, T::Enum, D::Enum(V::Left)),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Access, T::Enum, D::Enum(V::Open)),
            attribute(A::Rotate, T::Integer, D::Integer(0)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::VAlign, T::Enum, D::Enum(V::Top)),
            attribute(A::MaxH, T::Measure, D::Measure("0in")),
            attribute(A::MaxW, T::Measure, D::Measure("0in")),
            attribute(A::MinH, T::Measure, D::Measure("0in")),
            attribute(A::MinW, T::Measure, D::Measure("0in")),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::ColSpan, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Locale, T::CData, D::None),
            attribute(A::AnchorType, T::Enum, D::Enum(V::TopLeft)),
            attribute(A::AccessKey, T::CData, D::None),
        ],
    },
    // cxfa_agent.cpp: CXFA_Agent via CXFA_Node.
    NodeSchema {
        element: E::Agent,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_outputxsl.cpp: CXFA_OutputXSL via CXFA_Node.
    NodeSchema {
        element: E::OutputXSL,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Uri, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_adjustdata.cpp: CXFA_AdjustData via CXFA_Node.
    NodeSchema {
        element: E::AdjustData,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_autosave.cpp: CXFA_AutoSave via CXFA_Node.
    NodeSchema {
        element: E::AutoSave,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_contentarea.cpp: CXFA_ContentArea via CXFA_Node.
    NodeSchema {
        element: E::ContentArea,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Desc, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::H, T::Measure, D::Measure("0in")),
            attribute(A::W, T::Measure, D::Measure("0in")),
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_wsdladdress.cpp: CXFA_WsdlAddress via CXFA_Node.
    NodeSchema {
        element: E::WsdlAddress,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_solid.cpp: CXFA_Solid via CXFA_Node.
    NodeSchema {
        element: E::Solid,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_datetimesymbols.cpp: CXFA_DateTimeSymbols via CXFA_Node.
    NodeSchema {
        element: E::DateTimeSymbols,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_encryptionlevel.cpp: CXFA_EncryptionLevel via CXFA_Node.
    NodeSchema {
        element: E::EncryptionLevel,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_edge.cpp: CXFA_Edge via CXFA_Stroke.
    NodeSchema {
        element: E::Edge,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Cap, T::Enum, D::Enum(V::Square)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Stroke, T::Enum, D::Enum(V::Solid)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Thickness, T::Measure, D::Measure("0.5pt")),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_stipple.cpp: CXFA_Stipple via CXFA_Node.
    NodeSchema {
        element: E::Stipple,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Rate, T::Integer, D::Integer(50)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_attributes.cpp: CXFA_Attributes via CXFA_Node.
    NodeSchema {
        element: E::Attributes,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_versioncontrol.cpp: CXFA_VersionControl via CXFA_Node.
    NodeSchema {
        element: E::VersionControl,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::SourceBelow, T::Enum, D::Enum(V::Update)),
            attribute(A::OutputBelow, T::Enum, D::Enum(V::Warn)),
            attribute(A::SourceAbove, T::Enum, D::Enum(V::Warn)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_meridiem.cpp: CXFA_Meridiem via CXFA_Node.
    NodeSchema {
        element: E::Meridiem,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_exclgroup.cpp: CXFA_ExclGroup via CXFA_Node.
    NodeSchema {
        element: E::ExclGroup,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Para, 1, 0),
            property(E::Border, 1, 0),
            property(E::Assist, 1, 0),
            property(E::Traversal, 1, 0),
            property(E::Validate, 1, 0),
            property(E::Caption, 1, 0),
            property(E::Bind, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Calculate, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::H, T::Measure, D::Measure("0in")),
            attribute(A::W, T::Measure, D::Measure("0in")),
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::HAlign, T::Enum, D::Enum(V::Left)),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Access, T::Enum, D::Enum(V::Open)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::VAlign, T::Enum, D::Enum(V::Top)),
            attribute(A::MaxH, T::Measure, D::Measure("0in")),
            attribute(A::MaxW, T::Measure, D::Measure("0in")),
            attribute(A::MinH, T::Measure, D::Measure("0in")),
            attribute(A::MinW, T::Measure, D::Measure("0in")),
            attribute(A::Layout, T::Enum, D::Enum(V::Position)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::ColSpan, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::AnchorType, T::Enum, D::Enum(V::TopLeft)),
            attribute(A::AccessKey, T::CData, D::None),
        ],
    },
    // cxfa_tooltip.cpp: CXFA_ToolTip via CXFA_Node.
    NodeSchema {
        element: E::ToolTip,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Rid, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_compress.cpp: CXFA_Compress via CXFA_Node.
    NodeSchema {
        element: E::Compress,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Scope, T::Enum, D::Enum(V::ImageOnly)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_reason.cpp: CXFA_Reason via CXFA_Node.
    NodeSchema {
        element: E::Reason,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_execute.cpp: CXFA_Execute via CXFA_Node.
    NodeSchema {
        element: E::Execute,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Connection, T::CData, D::None),
            attribute(A::RunAt, T::Enum, D::Enum(V::Client)),
            attribute(A::ExecuteType, T::Enum, D::Enum(V::Import)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_contentcopy.cpp: CXFA_ContentCopy via CXFA_Node.
    NodeSchema {
        element: E::ContentCopy,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_datetimeedit.cpp: CXFA_DateTimeEdit via CXFA_Node.
    NodeSchema {
        element: E::DateTimeEdit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Comb, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Picker, T::Enum, D::Enum(V::Host)),
            attribute(A::HScrollPolicy, T::Enum, D::Enum(V::Auto)),
        ],
    },
    // cxfa_config.cpp: CXFA_Config via CXFA_Node.
    NodeSchema {
        element: E::Config,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Present, 1, 0),
            property(E::Acrobat, 1, 0),
            property(E::Trace, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_image.cpp: CXFA_Image via CXFA_Node.
    NodeSchema {
        element: E::Image,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::ContentType, T::CData, D::None),
            attribute(A::TransferEncoding, T::Enum, D::Enum(V::Base64)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Aspect, T::Enum, D::Enum(V::Fit)),
            attribute(A::Href, T::CData, D::None),
        ],
    },
    // cxfa_sharpxhtml.cpp: CXFA_SharpxHTML via CXFA_Node.
    NodeSchema {
        element: E::SharpxHTML,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Template)
            | packet_mask(P::Config)
            | packet_mask(P::LocaleSet)
            | packet_mask(P::ConnectionSet)
            | packet_mask(P::SourceSet)
            | packet_mask(P::Form),
        properties: &[],
        attributes: &[attribute(A::Value, T::CData, D::None)],
    },
    // cxfa_numberofcopies.cpp: CXFA_NumberOfCopies via CXFA_Node.
    NodeSchema {
        element: E::NumberOfCopies,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_behavioroverride.cpp: CXFA_BehaviorOverride via CXFA_Node.
    NodeSchema {
        element: E::BehaviorOverride,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_timestamp.cpp: CXFA_TimeStamp via CXFA_Node.
    NodeSchema {
        element: E::TimeStamp,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Server, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_month.cpp: CXFA_Month via CXFA_Node.
    NodeSchema {
        element: E::Month,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_viewerpreferences.cpp: CXFA_ViewerPreferences via CXFA_Node.
    NodeSchema {
        element: E::ViewerPreferences,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::PrintScaling, 1, 0),
            property(E::Enforce, 1, 0),
            property(E::NumberOfCopies, 1, 0),
            property(E::PageRange, 1, 0),
            property(E::AddViewerPreferences, 1, 0),
            property(E::ADBE_JSConsole, 1, 0),
            property(E::DuplexOption, 1, 0),
            property(E::ADBE_JSDebugger, 1, 0),
            property(E::PickTrayByPDFSize, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_scriptmodel.cpp: CXFA_ScriptModel via CXFA_Node.
    NodeSchema {
        element: E::ScriptModel,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_decimal.cpp: CXFA_Decimal via CXFA_Node.
    NodeSchema {
        element: E::Decimal,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::FracDigits, T::Integer, D::Integer(2)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::LeadDigits, T::Integer, D::Integer(-1)),
        ],
    },
    // cxfa_subform.cpp: CXFA_Subform via CXFA_Node.
    NodeSchema {
        element: E::Subform,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Break, 1, 0),
            property(E::Margin, 1, 0),
            property(E::Para, 1, 0),
            property(E::Border, 1, 0),
            property(E::Assist, 1, 0),
            property(E::Traversal, 1, 0),
            property(E::Keep, 1, 0),
            property(E::Validate, 1, 0),
            property(E::PageSet, 1, 0),
            property(E::Overflow, 1, 0),
            property(E::Bind, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Bookend, 1, 0),
            property(E::Calculate, 1, 0),
            property(E::Extras, 1, 0),
            property(E::Variables, 1, 0),
            property(E::Occur, 1, 0),
        ],
        attributes: &[
            attribute(A::H, T::Measure, D::Measure("0in")),
            attribute(A::W, T::Measure, D::Measure("0in")),
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::HAlign, T::Enum, D::Enum(V::Left)),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::AllowMacro, T::Boolean, D::Boolean(false)),
            attribute(A::ColumnWidths, T::CData, D::None),
            attribute(A::Access, T::Enum, D::Enum(V::Open)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::VAlign, T::Enum, D::Enum(V::Top)),
            attribute(A::MaxH, T::Measure, D::Measure("0in")),
            attribute(A::MaxW, T::Measure, D::Measure("0in")),
            attribute(A::MinH, T::Measure, D::Measure("0in")),
            attribute(A::MinW, T::Measure, D::Measure("0in")),
            attribute(A::Layout, T::Enum, D::Enum(V::Position)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::MergeMode, T::Enum, D::Enum(V::ConsumeData)),
            attribute(A::ColSpan, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Locale, T::CData, D::None),
            attribute(A::AnchorType, T::Enum, D::Enum(V::TopLeft)),
            attribute(A::RestoreState, T::Enum, D::Enum(V::Manual)),
            attribute(A::Scope, T::Enum, D::Enum(V::Name)),
        ],
    },
    // cxfa_select.cpp: CXFA_Select via CXFA_Node.
    NodeSchema {
        element: E::Select,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_window.cpp: CXFA_Window via CXFA_Node.
    NodeSchema {
        element: E::Window,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_localeset.cpp: CXFA_LocaleSet via CXFA_Node.
    NodeSchema {
        element: E::LocaleSet,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_handler.cpp: CXFA_Handler via CXFA_Node.
    NodeSchema {
        element: E::Handler,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_presence.cpp: CXFA_Presence via CXFA_Node.
    NodeSchema {
        element: E::Presence,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_record.cpp: CXFA_Record via CXFA_Node.
    NodeSchema {
        element: E::Record,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_embed.cpp: CXFA_Embed via CXFA_Node.
    NodeSchema {
        element: E::Embed,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_version.cpp: CXFA_Version via CXFA_Node.
    NodeSchema {
        element: E::Version,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_command.cpp: CXFA_Command via CXFA_Node.
    NodeSchema {
        element: E::Command,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[
            property(E::Query, 1, 0),
            property(E::Insert, 1, 0),
            property(E::Update, 1, 0),
            property(E::Delete, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Timeout, T::Integer, D::Integer(30)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_copies.cpp: CXFA_Copies via CXFA_Node.
    NodeSchema {
        element: E::Copies,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_staple.cpp: CXFA_Staple via CXFA_Node.
    NodeSchema {
        element: E::Staple,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Mode, T::Enum, D::Enum(V::UsePrinterSetting)),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_submitformat.cpp: CXFA_SubmitFormat via CXFA_Node.
    NodeSchema {
        element: E::SubmitFormat,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_boolean.cpp: CXFA_Boolean via CXFA_Node.
    NodeSchema {
        element: E::Boolean,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_message.cpp: CXFA_Message via CXFA_Node.
    NodeSchema {
        element: E::Message,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::MsgId, 1, 0), property(E::Severity, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_output.cpp: CXFA_Output via CXFA_Node.
    NodeSchema {
        element: E::Output,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::To, 1, 0),
            property(E::Uri, 1, 0),
            property(E::Type, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_psmap.cpp: CXFA_PsMap via CXFA_Node.
    NodeSchema {
        element: E::PsMap,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[],
    },
    // cxfa_excludens.cpp: CXFA_ExcludeNS via CXFA_Node.
    NodeSchema {
        element: E::ExcludeNS,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_assist.cpp: CXFA_Assist via CXFA_Node.
    NodeSchema {
        element: E::Assist,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::ToolTip, 1, 0), property(E::Speak, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Role, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_picture.cpp: CXFA_Picture via CXFA_Node.
    NodeSchema {
        element: E::Picture,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_traversal.cpp: CXFA_Traversal via CXFA_Node.
    NodeSchema {
        element: E::Traversal,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_silentprint.cpp: CXFA_SilentPrint via CXFA_Node.
    NodeSchema {
        element: E::SilentPrint,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::AddSilentPrint, 1, 0),
            property(E::PrinterName, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_webclient.cpp: CXFA_WebClient via CXFA_Node.
    NodeSchema {
        element: E::WebClient,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::FontInfo, 1, 0), property(E::Xdc, 1, 0)],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_producer.cpp: CXFA_Producer via CXFA_Node.
    NodeSchema {
        element: E::Producer,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_corner.cpp: CXFA_Corner via CXFA_Stroke.
    NodeSchema {
        element: E::Corner,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Stroke, T::Enum, D::Enum(V::Solid)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Inverted, T::Boolean, D::Boolean(false)),
            attribute(A::Thickness, T::Measure, D::Measure("0.5pt")),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Join, T::Enum, D::Enum(V::Square)),
            attribute(A::Radius, T::Measure, D::Measure("0in")),
        ],
    },
    // cxfa_msgid.cpp: CXFA_MsgId via CXFA_Node.
    NodeSchema {
        element: E::MsgId,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_color.cpp: CXFA_Color via CXFA_Node.
    NodeSchema {
        element: E::Color,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::CSpace, T::CData, D::Text("SRGB")),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Value, T::CData, D::None),
        ],
    },
    // cxfa_keep.cpp: CXFA_Keep via CXFA_Node.
    NodeSchema {
        element: E::Keep,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Next, T::Enum, D::Enum(V::None)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Previous, T::Enum, D::Enum(V::None)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Intact, T::Enum, D::Enum(V::None)),
        ],
    },
    // cxfa_query.cpp: CXFA_Query via CXFA_Node.
    NodeSchema {
        element: E::Query,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[property(E::RecordSet, 1, 0), property(E::Select, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::CommandType, T::Enum, D::Enum(V::Unknown)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_insert.cpp: CXFA_Insert via CXFA_Node.
    NodeSchema {
        element: E::Insert,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_imageedit.cpp: CXFA_ImageEdit via CXFA_Node.
    NodeSchema {
        element: E::ImageEdit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Data, T::Enum, D::Enum(V::Link)),
        ],
    },
    // cxfa_validate.cpp: CXFA_Validate via CXFA_Node.
    NodeSchema {
        element: E::Validate,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Message, 1, 0),
            property(E::Picture, 1, 0),
            property(E::Script, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::ScriptTest, T::Enum, D::Enum(V::Error)),
            attribute(A::NullTest, T::Enum, D::Enum(V::Disabled)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::FormatTest, T::Enum, D::Enum(V::Warning)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_digestmethods.cpp: CXFA_DigestMethods via CXFA_Node.
    NodeSchema {
        element: E::DigestMethods,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_numberpatterns.cpp: CXFA_NumberPatterns via CXFA_Node.
    NodeSchema {
        element: E::NumberPatterns,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::NumberPattern, 4, 0)],
        attributes: &[],
    },
    // cxfa_pageset.cpp: CXFA_PageSet via CXFA_Node.
    NodeSchema {
        element: E::PageSet,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0), property(E::Occur, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Relation, T::Enum, D::Enum(V::OrderedOccurrence)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::DuplexImposition, T::Enum, D::Enum(V::LongEdge)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_integer.cpp: CXFA_Integer via CXFA_Node.
    NodeSchema {
        element: E::Integer,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_soapaddress.cpp: CXFA_SoapAddress via CXFA_Node.
    NodeSchema {
        element: E::SoapAddress,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_equate.cpp: CXFA_Equate via CXFA_Node.
    NodeSchema {
        element: E::Equate,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::To, T::CData, D::None),
            attribute(A::Force, T::Boolean, D::Boolean(false)),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::From, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_formfieldfilling.cpp: CXFA_FormFieldFilling via CXFA_Node.
    NodeSchema {
        element: E::FormFieldFilling,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pagerange.cpp: CXFA_PageRange via CXFA_Node.
    NodeSchema {
        element: E::PageRange,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_update.cpp: CXFA_Update via CXFA_Node.
    NodeSchema {
        element: E::Update,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_connectstring.cpp: CXFA_ConnectString via CXFA_Node.
    NodeSchema {
        element: E::ConnectString,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_mode.cpp: CXFA_Mode via CXFA_Node.
    NodeSchema {
        element: E::Mode,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_layout.cpp: CXFA_Layout via CXFA_Node.
    NodeSchema {
        element: E::Layout,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_sharpxml.cpp: CXFA_Sharpxml via CXFA_Node.
    NodeSchema {
        element: E::Sharpxml,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[attribute(A::Value, T::CData, D::None)],
    },
    // cxfa_xsdconnection.cpp: CXFA_XsdConnection via CXFA_Node.
    NodeSchema {
        element: E::XsdConnection,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[property(E::Uri, 1, 0), property(E::RootElement, 1, 0)],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::DataDescription, T::CData, D::None),
        ],
    },
    // cxfa_traverse.cpp: CXFA_Traverse via CXFA_Node.
    NodeSchema {
        element: E::Traverse,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Script, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Operation, T::Enum, D::Enum(V::Next)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_encodings.cpp: CXFA_Encodings via CXFA_Node.
    NodeSchema {
        element: E::Encodings,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_template.cpp: CXFA_Template via CXFA_Node.
    NodeSchema {
        element: E::Template,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Uri, 1, 0),
            property(E::Xsl, 1, 0),
            property(E::StartPage, 1, 0),
            property(E::Relevant, 1, 0),
            property(E::Base, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::BaseProfile, T::Enum, D::Enum(V::Full)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_acrobat.cpp: CXFA_Acrobat via CXFA_Node.
    NodeSchema {
        element: E::Acrobat,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::AutoSave, 1, 0),
            property(E::Validate, 1, 0),
            property(E::ValidateApprovalSignatures, 1, 0),
            property(E::Acrobat7, 1, 0),
            property(E::Common, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_validationmessaging.cpp: CXFA_ValidationMessaging via CXFA_Node.
    NodeSchema {
        element: E::ValidationMessaging,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_signing.cpp: CXFA_Signing via CXFA_Node.
    NodeSchema {
        element: E::Signing,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_script.cpp: CXFA_Script via CXFA_Node.
    NodeSchema {
        element: E::Script,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Exclude, 1, 0),
            property(E::CurrentPage, 1, 0),
            property(E::RunScripts, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::ContentType, T::CData, D::None),
            attribute(A::RunAt, T::Enum, D::Enum(V::Client)),
            attribute(A::Binding, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_addviewerpreferences.cpp: CXFA_AddViewerPreferences via CXFA_Node.
    NodeSchema {
        element: E::AddViewerPreferences,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_alwaysembed.cpp: CXFA_AlwaysEmbed via CXFA_Node.
    NodeSchema {
        element: E::AlwaysEmbed,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_passwordedit.cpp: CXFA_PasswordEdit via CXFA_Node.
    NodeSchema {
        element: E::PasswordEdit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::PasswordChar, T::CData, D::Text("*")),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::HScrollPolicy, T::Enum, D::Enum(V::Auto)),
        ],
    },
    // cxfa_numericedit.cpp: CXFA_NumericEdit via CXFA_Node.
    NodeSchema {
        element: E::NumericEdit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Comb, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::HScrollPolicy, T::Enum, D::Enum(V::Auto)),
        ],
    },
    // cxfa_encryptionmethod.cpp: CXFA_EncryptionMethod via CXFA_Node.
    NodeSchema {
        element: E::EncryptionMethod,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_change.cpp: CXFA_Change via CXFA_Node.
    NodeSchema {
        element: E::Change,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_pagearea.cpp: CXFA_PageArea via CXFA_Node.
    NodeSchema {
        element: E::PageArea,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Medium, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Extras, 1, 0),
            property(E::Occur, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::PagePosition, T::Enum, D::Enum(V::Any)),
            attribute(A::OddOrEven, T::Enum, D::Enum(V::Any)),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::InitialNumber, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Numbered, T::Integer, D::Integer(1)),
            attribute(A::BlankOrNotBlank, T::Enum, D::Enum(V::Any)),
        ],
    },
    // cxfa_submiturl.cpp: CXFA_SubmitUrl via CXFA_Node.
    NodeSchema {
        element: E::SubmitUrl,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_oids.cpp: CXFA_Oids via CXFA_Node.
    NodeSchema {
        element: E::Oids,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_signature.cpp: CXFA_Signature via CXFA_Node.
    NodeSchema {
        element: E::Signature,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Filter, 1, 0),
            property(E::Border, 1, 0),
            property(E::Manifest, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::PDF1_3)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_adbe_jsconsole.cpp: CXFA_ADBE_JSConsole via CXFA_Node.
    NodeSchema {
        element: E::ADBE_JSConsole,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_caption.cpp: CXFA_Caption via CXFA_Node.
    NodeSchema {
        element: E::Caption,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Para, 1, 0),
            property(E::Font, 1, 0),
            property(E::Value, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Reserve, T::Measure, D::Measure("-1un")),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Placement, T::Enum, D::Enum(V::Left)),
        ],
    },
    // cxfa_relevant.cpp: CXFA_Relevant via CXFA_Node.
    NodeSchema {
        element: E::Relevant,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_fliplabel.cpp: CXFA_FlipLabel via CXFA_Node.
    NodeSchema {
        element: E::FlipLabel,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_exdata.cpp: CXFA_ExData via CXFA_Node.
    NodeSchema {
        element: E::ExData,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Rid, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::ContentType, T::CData, D::None),
            attribute(A::TransferEncoding, T::Enum, D::Enum(V::None)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::MaxLength, T::Integer, D::Integer(-1)),
            attribute(A::Href, T::CData, D::None),
        ],
    },
    // cxfa_daynames.cpp: CXFA_DayNames via CXFA_Node.
    NodeSchema {
        element: E::DayNames,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::Day, 7, 0)],
        attributes: &[attribute(A::Abbr, T::Boolean, D::Boolean(false))],
    },
    // cxfa_soapaction.cpp: CXFA_SoapAction via CXFA_Node.
    NodeSchema {
        element: E::SoapAction,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_defaulttypeface.cpp: CXFA_DefaultTypeface via CXFA_Node.
    NodeSchema {
        element: E::DefaultTypeface,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::WritingScript, T::Enum, D::Enum(V::Asterisk)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_manifest.cpp: CXFA_Manifest via CXFA_Node.
    NodeSchema {
        element: E::Manifest,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Action, T::Enum, D::Enum(V::Include)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_overflow.cpp: CXFA_Overflow via CXFA_Node.
    NodeSchema {
        element: E::Overflow,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Trailer, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
            attribute(A::Leader, T::CData, D::None),
        ],
    },
    // cxfa_linear.cpp: CXFA_Linear via CXFA_Node.
    NodeSchema {
        element: E::Linear,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::ToRight)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_currencysymbol.cpp: CXFA_CurrencySymbol via CXFA_Node.
    NodeSchema {
        element: E::CurrencySymbol,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Symbol))],
    },
    // cxfa_delete.cpp: CXFA_Delete via CXFA_Node.
    NodeSchema {
        element: E::Delete,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_digestmethod.cpp: CXFA_DigestMethod via CXFA_Node.
    NodeSchema {
        element: E::DigestMethod,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_instancemanager.cpp: CXFA_InstanceManager via CXFA_Node.
    NodeSchema {
        element: E::InstanceManager,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Form),
        properties: &[property(E::Occur, 1, 0)],
        attributes: &[attribute(A::Name, T::CData, D::None)],
    },
    // cxfa_equaterange.cpp: CXFA_EquateRange via CXFA_Node.
    NodeSchema {
        element: E::EquateRange,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::To, T::CData, D::None),
            attribute(A::UnicodeRange, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::From, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_medium.cpp: CXFA_Medium via CXFA_Node.
    NodeSchema {
        element: E::Medium,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::TrayOut, T::Enum, D::Enum(V::Auto)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Orientation, T::Enum, D::Enum(V::Portrait)),
            attribute(A::ImagingBBox, T::CData, D::Text("none")),
            attribute(A::Short, T::Measure, D::Measure("0in")),
            attribute(A::TrayIn, T::Enum, D::Enum(V::Auto)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Stock, T::CData, D::None),
            attribute(A::Long, T::Measure, D::Measure("0in")),
        ],
    },
    // cxfa_textedit.cpp: CXFA_TextEdit via CXFA_Node.
    NodeSchema {
        element: E::TextEdit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Margin, 1, 0),
            property(E::Border, 1, 0),
            property(E::Comb, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::VScrollPolicy, T::Enum, D::Enum(V::Auto)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::AllowRichText, T::Boolean, D::Boolean(false)),
            attribute(A::MultiLine, T::Boolean, D::Boolean(false)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::HScrollPolicy, T::Enum, D::Enum(V::Auto)),
        ],
    },
    // cxfa_templatecache.cpp: CXFA_TemplateCache via CXFA_Node.
    NodeSchema {
        element: E::TemplateCache,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::MaxEntries, T::Integer, D::Integer(5)),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_compressobjectstream.cpp: CXFA_CompressObjectStream via CXFA_Node.
    NodeSchema {
        element: E::CompressObjectStream,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_datavalue.cpp: CXFA_DataValue via CXFA_Node.
    NodeSchema {
        element: E::DataValue,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Datasets),
        properties: &[],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::ContentType, T::CData, D::None),
            attribute(A::Contains, T::Enum, D::Enum(V::Data)),
            attribute(A::Value, T::CData, D::None),
            attribute(A::IsNull, T::Boolean, D::Boolean(false)),
        ],
    },
    // cxfa_accessiblecontent.cpp: CXFA_AccessibleContent via CXFA_Node.
    NodeSchema {
        element: E::AccessibleContent,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_includexdpcontent.cpp: CXFA_IncludeXDPContent via CXFA_Node.
    NodeSchema {
        element: E::IncludeXDPContent,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_xmlconnection.cpp: CXFA_XmlConnection via CXFA_Node.
    NodeSchema {
        element: E::XmlConnection,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[property(E::Uri, 1, 0)],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::DataDescription, T::CData, D::None),
        ],
    },
    // cxfa_validateapprovalsignatures.cpp: CXFA_ValidateApprovalSignatures via CXFA_Node.
    NodeSchema {
        element: E::ValidateApprovalSignatures,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_signdata.cpp: CXFA_SignData via CXFA_Node.
    NodeSchema {
        element: E::SignData,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Filter, 1, 0), property(E::Manifest, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Operation, T::Enum, D::Enum(V::Sign)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
        ],
    },
    // cxfa_packets.cpp: CXFA_Packets via CXFA_Node.
    NodeSchema {
        element: E::Packets,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_datepattern.cpp: CXFA_DatePattern via CXFA_Node.
    NodeSchema {
        element: E::DatePattern,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Med))],
    },
    // cxfa_duplexoption.cpp: CXFA_DuplexOption via CXFA_Node.
    NodeSchema {
        element: E::DuplexOption,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_base.cpp: CXFA_Base via CXFA_Node.
    NodeSchema {
        element: E::Base,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_bind.cpp: CXFA_Bind via CXFA_Node.
    NodeSchema {
        element: E::Bind,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Picture, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::ContentType, T::CData, D::None),
            attribute(A::TransferEncoding, T::Enum, D::Enum(V::None)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Match, T::Enum, D::Enum(V::Once)),
        ],
    },
    // cxfa_compression.cpp: CXFA_Compression via CXFA_Node.
    NodeSchema {
        element: E::Compression,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Level, 1, 0),
            property(E::Type, 1, 0),
            property(E::CompressObjectStream, 1, 0),
            property(E::CompressLogicalStructure, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_user.cpp: CXFA_User via CXFA_Node.
    NodeSchema {
        element: E::User,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_rectangle.cpp: CXFA_Rectangle via CXFA_Box.
    NodeSchema {
        element: E::Rectangle,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Edge, 4, 0),
            property(E::Corner, 4, 0),
            property(E::Fill, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Hand, T::Enum, D::Enum(V::Even)),
        ],
    },
    // cxfa_effectiveoutputpolicy.cpp: CXFA_EffectiveOutputPolicy via CXFA_Node.
    NodeSchema {
        element: E::EffectiveOutputPolicy,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_adbe_jsdebugger.cpp: CXFA_ADBE_JSDebugger via CXFA_Node.
    NodeSchema {
        element: E::ADBE_JSDebugger,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_acrobat7.cpp: CXFA_Acrobat7 via CXFA_Node.
    NodeSchema {
        element: E::Acrobat7,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::DynamicRender, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_interactive.cpp: CXFA_Interactive via CXFA_Node.
    NodeSchema {
        element: E::Interactive,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_locale.cpp: CXFA_Locale via CXFA_Node.
    NodeSchema {
        element: E::Locale,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config) | packet_mask(P::LocaleSet),
        properties: &[
            property(E::DatePatterns, 1, 0),
            property(E::CalendarSymbols, 1, 0),
            property(E::CurrencySymbols, 1, 0),
            property(E::Typefaces, 1, 0),
            property(E::DateTimeSymbols, 1, 0),
            property(E::NumberPatterns, 1, 0),
            property(E::NumberSymbols, 1, 0),
            property(E::TimePatterns, 1, 0),
        ],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_currentpage.cpp: CXFA_CurrentPage via CXFA_Node.
    NodeSchema {
        element: E::CurrentPage,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_data.cpp: CXFA_Data via CXFA_Node.
    NodeSchema {
        element: E::Data,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Uri, 1, 0),
            property(E::Xsl, 1, 0),
            property(E::StartNode, 1, 0),
            property(E::OutputXSL, 1, 0),
            property(E::AdjustData, 1, 0),
            property(E::Attributes, 1, 0),
            property(E::Window, 1, 0),
            property(E::Record, 1, 0),
            property(E::Range, 1, 0),
            property(E::IncrementalLoad, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_date.cpp: CXFA_Date via CXFA_Node.
    NodeSchema {
        element: E::Date,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_desc.cpp: CXFA_Desc via CXFA_Node.
    NodeSchema {
        element: E::Desc,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Text, 1, 0),
            property(E::Time, 1, 0),
            property(E::DateTime, 1, 0),
            property(E::Image, 1, 0),
            property(E::Decimal, 1, 0),
            property(E::Boolean, 1, 0),
            property(E::Integer, 1, 0),
            property(E::ExData, 1, 0),
            property(E::Date, 1, 0),
            property(E::Float, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_encrypt.cpp: CXFA_Encrypt via CXFA_Node.
    NodeSchema {
        element: E::Encrypt,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Config) | packet_mask(P::Form),
        properties: &[property(E::Certificate, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_draw.cpp: CXFA_Draw via CXFA_Node.
    NodeSchema {
        element: E::Draw,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Ui, 1, 0),
            property(E::Margin, 1, 0),
            property(E::Para, 1, 0),
            property(E::Border, 1, 0),
            property(E::Assist, 1, 0),
            property(E::Traversal, 1, 0),
            property(E::Keep, 1, 0),
            property(E::Caption, 1, 0),
            property(E::Desc, 1, 0),
            property(E::Font, 1, 0),
            property(E::Value, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::H, T::Measure, D::Measure("0in")),
            attribute(A::W, T::Measure, D::Measure("0in")),
            attribute(A::X, T::Measure, D::Measure("0in")),
            attribute(A::Y, T::Measure, D::Measure("0in")),
            attribute(A::Id, T::CData, D::None),
            attribute(A::HAlign, T::Enum, D::Enum(V::Left)),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Rotate, T::Integer, D::Integer(0)),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::VAlign, T::Enum, D::Enum(V::Top)),
            attribute(A::MaxH, T::Measure, D::Measure("0in")),
            attribute(A::MaxW, T::Measure, D::Measure("0in")),
            attribute(A::MinH, T::Measure, D::Measure("0in")),
            attribute(A::MinW, T::Measure, D::Measure("0in")),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::ColSpan, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Locale, T::CData, D::None),
            attribute(A::AnchorType, T::Enum, D::Enum(V::TopLeft)),
        ],
    },
    // cxfa_encryption.cpp: CXFA_Encryption via CXFA_Node.
    NodeSchema {
        element: E::Encryption,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::EncryptionLevel, 1, 0),
            property(E::Encrypt, 1, 0),
            property(E::Permissions, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_meridiemnames.cpp: CXFA_MeridiemNames via CXFA_Node.
    NodeSchema {
        element: E::MeridiemNames,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::Meridiem, 2, 0)],
        attributes: &[],
    },
    // cxfa_messaging.cpp: CXFA_Messaging via CXFA_Node.
    NodeSchema {
        element: E::Messaging,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_speak.cpp: CXFA_Speak via CXFA_Node.
    NodeSchema {
        element: E::Speak,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Rid, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Priority, T::Enum, D::Enum(V::Custom)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Disable, T::Boolean, D::Boolean(false)),
        ],
    },
    // cxfa_datagroup.cpp: CXFA_DataGroup via CXFA_Node.
    NodeSchema {
        element: E::DataGroup,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Datasets),
        properties: &[],
        attributes: &[attribute(A::Name, T::CData, D::None)],
    },
    // cxfa_common.cpp: CXFA_Common via CXFA_Node.
    NodeSchema {
        element: E::Common,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::SuppressBanner, 1, 0),
            property(E::VersionControl, 1, 0),
            property(E::LocaleSet, 1, 0),
            property(E::Template, 1, 0),
            property(E::ValidationMessaging, 1, 0),
            property(E::Locale, 1, 0),
            property(E::Data, 1, 0),
            property(E::Messaging, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_sharptext.cpp: CXFA_Sharptext via CXFA_Node.
    NodeSchema {
        element: E::Sharptext,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Template)
            | packet_mask(P::Config)
            | packet_mask(P::LocaleSet)
            | packet_mask(P::ConnectionSet)
            | packet_mask(P::SourceSet)
            | packet_mask(P::Form),
        properties: &[],
        attributes: &[attribute(A::Value, T::CData, D::None)],
    },
    // cxfa_paginationoverride.cpp: CXFA_PaginationOverride via CXFA_Node.
    NodeSchema {
        element: E::PaginationOverride,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_reasons.cpp: CXFA_Reasons via CXFA_Node.
    NodeSchema {
        element: E::Reasons,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_signatureproperties.cpp: CXFA_SignatureProperties via CXFA_Node.
    NodeSchema {
        element: E::SignatureProperties,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_threshold.cpp: CXFA_Threshold via CXFA_Node.
    NodeSchema {
        element: E::Threshold,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_appearancefilter.cpp: CXFA_AppearanceFilter via CXFA_Node.
    NodeSchema {
        element: E::AppearanceFilter,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_fill.cpp: CXFA_Fill via CXFA_Node.
    NodeSchema {
        element: E::Fill,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Pattern, 1, 1),
            property(E::Solid, 1, 3),
            property(E::Stipple, 1, 1),
            property(E::Color, 1, 0),
            property(E::Linear, 1, 1),
            property(E::Extras, 1, 0),
            property(E::Radial, 1, 1),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_font.cpp: CXFA_Font via CXFA_Node.
    NodeSchema {
        element: E::Font,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Config) | packet_mask(P::Form),
        properties: &[property(E::Fill, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::LineThrough, T::Integer, D::Integer(0)),
            attribute(A::Typeface, T::CData, D::Text("Courier")),
            attribute(A::FontHorizontalScale, T::CData, D::Text("100%")),
            attribute(A::Use, T::CData, D::None),
            attribute(A::KerningMode, T::Enum, D::Enum(V::None)),
            attribute(A::Underline, T::Integer, D::Integer(0)),
            attribute(A::BaselineShift, T::Measure, D::Measure("0in")),
            attribute(A::OverlinePeriod, T::Enum, D::Enum(V::All)),
            attribute(A::LetterSpacing, T::CData, D::None),
            attribute(A::LineThroughPeriod, T::Enum, D::Enum(V::All)),
            attribute(A::FontVerticalScale, T::CData, D::Text("100%")),
            attribute(A::PsName, T::CData, D::None),
            attribute(A::Size, T::Measure, D::Measure("10pt")),
            attribute(A::Posture, T::Enum, D::Enum(V::Normal)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Weight, T::Enum, D::Enum(V::Normal)),
            attribute(A::UnderlinePeriod, T::Enum, D::Enum(V::All)),
            attribute(A::Overline, T::Integer, D::Integer(0)),
            attribute(A::GenericFamily, T::Enum, D::Enum(V::Serif)),
        ],
    },
    // cxfa_form.cpp: CXFA_Form via CXFA_Node.
    NodeSchema {
        element: E::Form,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::Form),
        properties: &[],
        attributes: &[attribute(A::Checksum, T::CData, D::None)],
    },
    // cxfa_mediuminfo.cpp: CXFA_MediumInfo via CXFA_Node.
    NodeSchema {
        element: E::MediumInfo,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Map, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_certificate.cpp: CXFA_Certificate via CXFA_Node.
    NodeSchema {
        element: E::Certificate,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_password.cpp: CXFA_Password via CXFA_Node.
    NodeSchema {
        element: E::Password,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_runscripts.cpp: CXFA_RunScripts via CXFA_Node.
    NodeSchema {
        element: E::RunScripts,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_trace.cpp: CXFA_Trace via CXFA_Node.
    NodeSchema {
        element: E::Trace,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_float.cpp: CXFA_Float via CXFA_Node.
    NodeSchema {
        element: E::Float,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_renderpolicy.cpp: CXFA_RenderPolicy via CXFA_Node.
    NodeSchema {
        element: E::RenderPolicy,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_destination.cpp: CXFA_Destination via CXFA_Node.
    NodeSchema {
        element: E::Destination,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_value.cpp: CXFA_Value via CXFA_Node.
    NodeSchema {
        element: E::Value,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Arc, 1, 1),
            property(E::Text, 1, 1),
            property(E::Time, 1, 1),
            property(E::DateTime, 1, 1),
            property(E::Image, 1, 1),
            property(E::Decimal, 1, 1),
            property(E::Boolean, 1, 1),
            property(E::Integer, 1, 1),
            property(E::ExData, 1, 1),
            property(E::Rectangle, 1, 1),
            property(E::Date, 1, 1),
            property(E::Float, 1, 1),
            property(E::Line, 1, 1),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Relevant, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Override, T::Boolean, D::Boolean(false)),
        ],
    },
    // cxfa_bookend.cpp: CXFA_Bookend via CXFA_Node.
    NodeSchema {
        element: E::Bookend,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Trailer, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Leader, T::CData, D::None),
        ],
    },
    // cxfa_exobject.cpp: CXFA_ExObject via CXFA_Node.
    NodeSchema {
        element: E::ExObject,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::CodeType, T::CData, D::None),
            attribute(A::Archive, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::CodeBase, T::CData, D::None),
            attribute(A::ClassId, T::CData, D::None),
        ],
    },
    // cxfa_openaction.cpp: CXFA_OpenAction via CXFA_Node.
    NodeSchema {
        element: E::OpenAction,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::Destination, 1, 0)],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_neverembed.cpp: CXFA_NeverEmbed via CXFA_Node.
    NodeSchema {
        element: E::NeverEmbed,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_binditems.cpp: CXFA_BindItems via CXFA_Node.
    NodeSchema {
        element: E::BindItems,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Connection, T::CData, D::None),
            attribute(A::LabelRef, T::CData, D::None),
            attribute(A::ValueRef, T::CData, D::None),
        ],
    },
    // cxfa_calculate.cpp: CXFA_Calculate via CXFA_Node.
    NodeSchema {
        element: E::Calculate,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Message, 1, 0),
            property(E::Script, 1, 0),
            property(E::Extras, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Override, T::Enum, D::Enum(V::Error)),
        ],
    },
    // cxfa_print.cpp: CXFA_Print via CXFA_Node.
    NodeSchema {
        element: E::Print,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_extras.cpp: CXFA_Extras via CXFA_Node.
    NodeSchema {
        element: E::Extras,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_proto.cpp: CXFA_Proto via CXFA_Node.
    NodeSchema {
        element: E::Proto,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[],
    },
    // cxfa_dsigdata.cpp: CXFA_DSigData via CXFA_Node.
    NodeSchema {
        element: E::DSigData,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[attribute(A::Value, T::CData, D::None)],
    },
    // cxfa_creator.cpp: CXFA_Creator via CXFA_Node.
    NodeSchema {
        element: E::Creator,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_connect.cpp: CXFA_Connect via CXFA_Node.
    NodeSchema {
        element: E::Connect,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet) | packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[
            property(E::Picture, 1, 0),
            property(E::ConnectString, 1, 0),
            property(E::User, 1, 0),
            property(E::Password, 1, 0),
        ],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Timeout, T::Integer, D::Integer(15)),
            attribute(A::Connection, T::CData, D::None),
            attribute(A::Usage, T::Enum, D::Enum(V::ExportAndImport)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::DelayedOpen, T::CData, D::None),
        ],
    },
    // cxfa_permissions.cpp: CXFA_Permissions via CXFA_Node.
    NodeSchema {
        element: E::Permissions,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::ModifyAnnots, 1, 0),
            property(E::ContentCopy, 1, 0),
            property(E::FormFieldFilling, 1, 0),
            property(E::Change, 1, 0),
            property(E::AccessibleContent, 1, 0),
            property(E::Print, 1, 0),
            property(E::PlaintextMetadata, 1, 0),
            property(E::PrintHighQuality, 1, 0),
            property(E::DocumentAssembly, 1, 0),
        ],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_connectionset.cpp: CXFA_ConnectionSet via CXFA_Node.
    NodeSchema {
        element: E::ConnectionSet,
        object_type: XfaObjectType::ModelNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[],
    },
    // cxfa_submit.cpp: CXFA_Submit via CXFA_Node.
    NodeSchema {
        element: E::Submit,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Encrypt, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Format, T::Enum, D::Enum(V::Xdp)),
            attribute(A::EmbedPDF, T::Boolean, D::Boolean(false)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
            attribute(A::TextEncoding, T::CData, D::None),
            attribute(A::XdpContent, T::CData, D::None),
        ],
    },
    // cxfa_range.cpp: CXFA_Range via CXFA_Node.
    NodeSchema {
        element: E::Range,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_linearized.cpp: CXFA_Linearized via CXFA_Node.
    NodeSchema {
        element: E::Linearized,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_packet.cpp: CXFA_Packet via CXFA_Node.
    NodeSchema {
        element: E::Packet,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Xdp),
        properties: &[],
        attributes: &[],
    },
    // cxfa_rootelement.cpp: CXFA_RootElement via CXFA_Node.
    NodeSchema {
        element: E::RootElement,
        object_type: XfaObjectType::TextNode,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_plaintextmetadata.cpp: CXFA_PlaintextMetadata via CXFA_Node.
    NodeSchema {
        element: E::PlaintextMetadata,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_numbersymbols.cpp: CXFA_NumberSymbols via CXFA_Node.
    NodeSchema {
        element: E::NumberSymbols,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::NumberSymbol, 5, 0)],
        attributes: &[],
    },
    // cxfa_printhighquality.cpp: CXFA_PrintHighQuality via CXFA_Node.
    NodeSchema {
        element: E::PrintHighQuality,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_driver.cpp: CXFA_Driver via CXFA_Node.
    NodeSchema {
        element: E::Driver,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[property(E::FontInfo, 1, 0), property(E::Xdc, 1, 0)],
        attributes: &[
            attribute(A::Name, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_incrementalload.cpp: CXFA_IncrementalLoad via CXFA_Node.
    NodeSchema {
        element: E::IncrementalLoad,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_subjectdn.cpp: CXFA_SubjectDN via CXFA_Node.
    NodeSchema {
        element: E::SubjectDN,
        object_type: XfaObjectType::NodeC,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Delimiter, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_compresslogicalstructure.cpp: CXFA_CompressLogicalStructure via CXFA_Node.
    NodeSchema {
        element: E::CompressLogicalStructure,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_incrementalmerge.cpp: CXFA_IncrementalMerge via CXFA_Node.
    NodeSchema {
        element: E::IncrementalMerge,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_radial.cpp: CXFA_Radial via CXFA_Node.
    NodeSchema {
        element: E::Radial,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Color, 1, 0), property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::ToEdge)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_variables.cpp: CXFA_Variables via CXFA_Node.
    NodeSchema {
        element: E::Variables,
        object_type: XfaObjectType::ContainerNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_timepatterns.cpp: CXFA_TimePatterns via CXFA_Node.
    NodeSchema {
        element: E::TimePatterns,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::TimePattern, 4, 0)],
        attributes: &[],
    },
    // cxfa_effectiveinputpolicy.cpp: CXFA_EffectiveInputPolicy via CXFA_Node.
    NodeSchema {
        element: E::EffectiveInputPolicy,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::ConnectionSet),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_nameattr.cpp: CXFA_NameAttr via CXFA_Node.
    NodeSchema {
        element: E::NameAttr,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_conformance.cpp: CXFA_Conformance via CXFA_Node.
    NodeSchema {
        element: E::Conformance,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_transform.cpp: CXFA_Transform via CXFA_Node.
    NodeSchema {
        element: E::Transform,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Config),
        properties: &[
            property(E::Whitespace, 1, 0),
            property(E::Rename, 1, 0),
            property(E::IfEmpty, 1, 0),
            property(E::Presence, 1, 0),
            property(E::Picture, 1, 0),
            property(E::NameAttr, 1, 0),
            property(E::GroupParent, 1, 0),
        ],
        attributes: &[
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_lockdocument.cpp: CXFA_LockDocument via CXFA_Node.
    NodeSchema {
        element: E::LockDocument,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Type, T::Enum, D::Enum(V::Optional)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_breakafter.cpp: CXFA_BreakAfter via CXFA_Node.
    NodeSchema {
        element: E::BreakAfter,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Script, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::StartNew, T::Boolean, D::Boolean(false)),
            attribute(A::Trailer, T::CData, D::None),
            attribute(A::TargetType, T::Enum, D::Enum(V::Auto)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Target, T::CData, D::None),
            attribute(A::Leader, T::CData, D::None),
        ],
    },
    // cxfa_line.cpp: CXFA_Line via CXFA_Node.
    NodeSchema {
        element: E::Line,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Edge, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Slope, T::Enum, D::Enum(V::Backslash)),
            attribute(A::Usehref, T::CData, D::None),
            attribute(A::Hand, T::Enum, D::Enum(V::Even)),
        ],
    },
    // cxfa_source.cpp: CXFA_Source via CXFA_Node.
    NodeSchema {
        element: E::Source,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::SourceSet),
        properties: &[property(E::Connect, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_occur.cpp: CXFA_Occur via CXFA_Node.
    NodeSchema {
        element: E::Occur,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[property(E::Extras, 1, 0)],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Max, T::Integer, D::Integer(1)),
            attribute(A::Min, T::Integer, D::Integer(1)),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Initial, T::Integer, D::Integer(1)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
    // cxfa_picktraybypdfsize.cpp: CXFA_PickTrayByPDFSize via CXFA_Node.
    NodeSchema {
        element: E::PickTrayByPDFSize,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_monthnames.cpp: CXFA_MonthNames via CXFA_Node.
    NodeSchema {
        element: E::MonthNames,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[property(E::Month, 12, 0)],
        attributes: &[attribute(A::Abbr, T::Boolean, D::Boolean(false))],
    },
    // cxfa_severity.cpp: CXFA_Severity via CXFA_Node.
    NodeSchema {
        element: E::Severity,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_groupparent.cpp: CXFA_GroupParent via CXFA_Node.
    NodeSchema {
        element: E::GroupParent,
        object_type: XfaObjectType::NodeV,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_documentassembly.cpp: CXFA_DocumentAssembly via CXFA_Node.
    NodeSchema {
        element: E::DocumentAssembly,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_numbersymbol.cpp: CXFA_NumberSymbol via CXFA_Node.
    NodeSchema {
        element: E::NumberSymbol,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::LocaleSet),
        properties: &[],
        attributes: &[attribute(A::Name, T::Enum, D::Enum(V::Decimal))],
    },
    // cxfa_tagged.cpp: CXFA_Tagged via CXFA_Node.
    NodeSchema {
        element: E::Tagged,
        object_type: XfaObjectType::ContentNode,
        valid_packets: packet_mask(P::Config),
        properties: &[],
        attributes: &[
            attribute(A::Desc, T::CData, D::None),
            attribute(A::Lock, T::Integer, D::Integer(0)),
        ],
    },
    // cxfa_items.cpp: CXFA_Items via CXFA_Node.
    NodeSchema {
        element: E::Items,
        object_type: XfaObjectType::Node,
        valid_packets: packet_mask(P::Template) | packet_mask(P::Form),
        properties: &[],
        attributes: &[
            attribute(A::Id, T::CData, D::None),
            attribute(A::Name, T::CData, D::None),
            attribute(A::Ref, T::CData, D::None),
            attribute(A::Use, T::CData, D::None),
            attribute(A::Presence, T::Enum, D::Enum(V::Visible)),
            attribute(A::Save, T::Boolean, D::Boolean(false)),
            attribute(A::Usehref, T::CData, D::None),
        ],
    },
];

/// The same set of constructible elements as CXFA_Node::Create.
pub fn schema_for(element: XfaElement) -> Option<&'static NodeSchema> {
    let index = match element {
        E::Ps => 0,
        E::To => 1,
        E::Ui => 2,
        E::RecordSet => 3,
        E::SubsetBelow => 4,
        E::SubformSet => 5,
        E::AdobeExtensionLevel => 6,
        E::Typeface => 7,
        E::Break => 8,
        E::FontInfo => 9,
        E::NumberPattern => 10,
        E::DynamicRender => 11,
        E::PrintScaling => 12,
        E::CheckButton => 13,
        E::DatePatterns => 14,
        E::SourceSet => 15,
        E::Amd => 16,
        E::Arc => 17,
        E::Day => 18,
        E::Era => 19,
        E::Jog => 20,
        E::Log => 21,
        E::Map => 22,
        E::Mdp => 23,
        E::BreakBefore => 24,
        E::Oid => 25,
        E::Pcl => 26,
        E::Pdf => 27,
        E::Ref => 28,
        E::Uri => 29,
        E::Xdc => 30,
        E::Xdp => 31,
        E::Xfa => 32,
        E::Xsl => 33,
        E::Zpl => 34,
        E::Cache => 35,
        E::Margin => 36,
        E::KeyUsage => 37,
        E::Exclude => 38,
        E::ChoiceList => 39,
        E::Level => 40,
        E::LabelPrinter => 41,
        E::CalendarSymbols => 42,
        E::Para => 43,
        E::Part => 44,
        E::Pdfa => 45,
        E::Filter => 46,
        E::Present => 47,
        E::Pagination => 48,
        E::Encoding => 49,
        E::Event => 50,
        E::Whitespace => 51,
        E::DefaultUi => 52,
        E::DataModel => 53,
        E::Barcode => 54,
        E::TimePattern => 55,
        E::BatchOutput => 56,
        E::Enforce => 57,
        E::CurrencySymbols => 58,
        E::AddSilentPrint => 59,
        E::Rename => 60,
        E::Operation => 61,
        E::Typefaces => 62,
        E::SubjectDNs => 63,
        E::Issuers => 64,
        E::WsdlConnection => 65,
        E::Debug => 66,
        E::Delta => 67,
        E::EraNames => 68,
        E::ModifyAnnots => 69,
        E::StartNode => 70,
        E::Button => 71,
        E::Format => 72,
        E::Border => 73,
        E::Area => 74,
        E::Hyphenation => 75,
        E::Text => 76,
        E::Time => 77,
        E::Type => 78,
        E::Overprint => 79,
        E::Certificates => 80,
        E::EncryptionMethods => 81,
        E::SetProperty => 82,
        E::PrinterName => 83,
        E::StartPage => 84,
        E::PageOffset => 85,
        E::DateTime => 86,
        E::Comb => 87,
        E::Pattern => 88,
        E::IfEmpty => 89,
        E::SuppressBanner => 90,
        E::OutputBin => 91,
        E::Field => 92,
        E::Agent => 93,
        E::OutputXSL => 94,
        E::AdjustData => 95,
        E::AutoSave => 96,
        E::ContentArea => 97,
        E::WsdlAddress => 98,
        E::Solid => 99,
        E::DateTimeSymbols => 100,
        E::EncryptionLevel => 101,
        E::Edge => 102,
        E::Stipple => 103,
        E::Attributes => 104,
        E::VersionControl => 105,
        E::Meridiem => 106,
        E::ExclGroup => 107,
        E::ToolTip => 108,
        E::Compress => 109,
        E::Reason => 110,
        E::Execute => 111,
        E::ContentCopy => 112,
        E::DateTimeEdit => 113,
        E::Config => 114,
        E::Image => 115,
        E::SharpxHTML => 116,
        E::NumberOfCopies => 117,
        E::BehaviorOverride => 118,
        E::TimeStamp => 119,
        E::Month => 120,
        E::ViewerPreferences => 121,
        E::ScriptModel => 122,
        E::Decimal => 123,
        E::Subform => 124,
        E::Select => 125,
        E::Window => 126,
        E::LocaleSet => 127,
        E::Handler => 128,
        E::Presence => 129,
        E::Record => 130,
        E::Embed => 131,
        E::Version => 132,
        E::Command => 133,
        E::Copies => 134,
        E::Staple => 135,
        E::SubmitFormat => 136,
        E::Boolean => 137,
        E::Message => 138,
        E::Output => 139,
        E::PsMap => 140,
        E::ExcludeNS => 141,
        E::Assist => 142,
        E::Picture => 143,
        E::Traversal => 144,
        E::SilentPrint => 145,
        E::WebClient => 146,
        E::Producer => 147,
        E::Corner => 148,
        E::MsgId => 149,
        E::Color => 150,
        E::Keep => 151,
        E::Query => 152,
        E::Insert => 153,
        E::ImageEdit => 154,
        E::Validate => 155,
        E::DigestMethods => 156,
        E::NumberPatterns => 157,
        E::PageSet => 158,
        E::Integer => 159,
        E::SoapAddress => 160,
        E::Equate => 161,
        E::FormFieldFilling => 162,
        E::PageRange => 163,
        E::Update => 164,
        E::ConnectString => 165,
        E::Mode => 166,
        E::Layout => 167,
        E::Sharpxml => 168,
        E::XsdConnection => 169,
        E::Traverse => 170,
        E::Encodings => 171,
        E::Template => 172,
        E::Acrobat => 173,
        E::ValidationMessaging => 174,
        E::Signing => 175,
        E::Script => 176,
        E::AddViewerPreferences => 177,
        E::AlwaysEmbed => 178,
        E::PasswordEdit => 179,
        E::NumericEdit => 180,
        E::EncryptionMethod => 181,
        E::Change => 182,
        E::PageArea => 183,
        E::SubmitUrl => 184,
        E::Oids => 185,
        E::Signature => 186,
        E::ADBE_JSConsole => 187,
        E::Caption => 188,
        E::Relevant => 189,
        E::FlipLabel => 190,
        E::ExData => 191,
        E::DayNames => 192,
        E::SoapAction => 193,
        E::DefaultTypeface => 194,
        E::Manifest => 195,
        E::Overflow => 196,
        E::Linear => 197,
        E::CurrencySymbol => 198,
        E::Delete => 199,
        E::DigestMethod => 200,
        E::InstanceManager => 201,
        E::EquateRange => 202,
        E::Medium => 203,
        E::TextEdit => 204,
        E::TemplateCache => 205,
        E::CompressObjectStream => 206,
        E::DataValue => 207,
        E::AccessibleContent => 208,
        E::IncludeXDPContent => 209,
        E::XmlConnection => 210,
        E::ValidateApprovalSignatures => 211,
        E::SignData => 212,
        E::Packets => 213,
        E::DatePattern => 214,
        E::DuplexOption => 215,
        E::Base => 216,
        E::Bind => 217,
        E::Compression => 218,
        E::User => 219,
        E::Rectangle => 220,
        E::EffectiveOutputPolicy => 221,
        E::ADBE_JSDebugger => 222,
        E::Acrobat7 => 223,
        E::Interactive => 224,
        E::Locale => 225,
        E::CurrentPage => 226,
        E::Data => 227,
        E::Date => 228,
        E::Desc => 229,
        E::Encrypt => 230,
        E::Draw => 231,
        E::Encryption => 232,
        E::MeridiemNames => 233,
        E::Messaging => 234,
        E::Speak => 235,
        E::DataGroup => 236,
        E::Common => 237,
        E::Sharptext => 238,
        E::PaginationOverride => 239,
        E::Reasons => 240,
        E::SignatureProperties => 241,
        E::Threshold => 242,
        E::AppearanceFilter => 243,
        E::Fill => 244,
        E::Font => 245,
        E::Form => 246,
        E::MediumInfo => 247,
        E::Certificate => 248,
        E::Password => 249,
        E::RunScripts => 250,
        E::Trace => 251,
        E::Float => 252,
        E::RenderPolicy => 253,
        E::Destination => 254,
        E::Value => 255,
        E::Bookend => 256,
        E::ExObject => 257,
        E::OpenAction => 258,
        E::NeverEmbed => 259,
        E::BindItems => 260,
        E::Calculate => 261,
        E::Print => 262,
        E::Extras => 263,
        E::Proto => 264,
        E::DSigData => 265,
        E::Creator => 266,
        E::Connect => 267,
        E::Permissions => 268,
        E::ConnectionSet => 269,
        E::Submit => 270,
        E::Range => 271,
        E::Linearized => 272,
        E::Packet => 273,
        E::RootElement => 274,
        E::PlaintextMetadata => 275,
        E::NumberSymbols => 276,
        E::PrintHighQuality => 277,
        E::Driver => 278,
        E::IncrementalLoad => 279,
        E::SubjectDN => 280,
        E::CompressLogicalStructure => 281,
        E::IncrementalMerge => 282,
        E::Radial => 283,
        E::Variables => 284,
        E::TimePatterns => 285,
        E::EffectiveInputPolicy => 286,
        E::NameAttr => 287,
        E::Conformance => 288,
        E::Transform => 289,
        E::LockDocument => 290,
        E::BreakAfter => 291,
        E::Line => 292,
        E::Source => 293,
        E::Occur => 294,
        E::PickTrayByPDFSize => 295,
        E::MonthNames => 296,
        E::Severity => 297,
        E::GroupParent => 298,
        E::DocumentAssembly => 299,
        E::NumberSymbol => 300,
        E::Tagged => 301,
        E::Items => 302,
        _ => return None,
    };
    Some(&SCHEMAS[index])
}
