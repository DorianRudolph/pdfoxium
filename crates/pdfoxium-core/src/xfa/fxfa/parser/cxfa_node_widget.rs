// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Widget model getters from `cxfa_node.cpp`, shared by actual FF consumers.
use super::{cxfa_node::CxfaNode, xfa_utils::map_rotation};
use crate::{
    fxcrt::fx_coordinates::CfxRectF,
    xfa::fxfa::fxfa_basic::{XfaAttribute as A, XfaAttributeValue as V, XfaElement as E},
};
impl CxfaNode {
    pub fn get_rotate(&self) -> i32 {
        self.try_integer(A::Rotate, false)
            .map_or(0, |rotation| map_rotation(rotation) / 90 * 90)
    }
    pub fn get_border_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Border).0
    }
    pub fn get_caption_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Caption).0
    }
    pub fn get_font_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Font).0
    }
    pub fn get_font_size(&self) -> f32 {
        let size = self
            .get_font_if_exists()
            .map_or(10., |font| font.font_get_size());
        if size < 0.1 {
            10.
        } else {
            size
        }
    }
    pub fn get_line_height(&self) -> f32 {
        let height = self
            .get_property(0, E::Para)
            .0
            .map_or(0., |para| para.para_get_line_height());
        if height < 1. {
            self.get_font_size() * 1.2
        } else {
            height
        }
    }
    pub fn get_text_color(&self) -> u32 {
        self.get_font_if_exists()
            .map_or(0xff000000, |font| font.font_get_color())
    }
    pub fn get_margin_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Margin).0
    }
    pub fn get_para_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Para).0
    }
    pub fn get_calculate_if_exists(&self) -> Option<Self> {
        self.get_property(0, E::Calculate).0
    }
    pub fn is_open_access(&self) -> bool {
        let mut node = Some(self.clone());
        while let Some(current) = node {
            if current.get_enum(A::Access) != V::Open {
                return false;
            }
            node = current.get_container_parent();
        }
        true
    }
    pub fn get_excl_group_if_exists(&self) -> Option<Self> {
        self.parent()
            .filter(|node| node.get_element_type() == E::ExclGroup)
    }
    pub fn get_excl_group_first_member(&self) -> Option<Self> {
        let mut node = self.first_child();
        while let Some(current) = node {
            if current.get_element_type() == E::Field {
                return Some(current);
            }
            node = current.next_sibling();
        }
        None
    }
    pub fn get_excl_group_next_member(&self, node: Option<&Self>) -> Option<Self> {
        let mut next = node?.next_sibling();
        while let Some(current) = next {
            if current.get_element_type() == E::Field {
                return Some(current);
            }
            next = current.next_sibling();
        }
        None
    }
    pub fn get_ui_border(&self) -> Option<Self> {
        self.get_ui_child_node()?.get_property(0, E::Border).0
    }
    /// Insets retain the native RectF representation (width/right and
    /// height/bottom are amounts, not an enclosing rectangle).
    pub fn get_ui_margin(&self) -> CfxRectF {
        let Some(ui) = self.get_ui_child_node() else {
            return CfxRectF::default();
        };
        let Some(margin) = ui.get_property(0, E::Margin).0 else {
            return CfxRectF::default();
        };
        let border = self.get_ui_border();
        if border
            .as_ref()
            .is_some_and(|border| border.box_get_presence() != V::Visible)
        {
            return CfxRectF::default();
        }
        let (mut left, mut top, mut right, mut bottom) = (
            margin.margin_try_left_inset(),
            margin.margin_try_top_inset(),
            margin.margin_try_right_inset(),
            margin.margin_try_bottom_inset(),
        );
        if let Some(border) = border {
            if left.is_none() || top.is_none() || right.is_none() || bottom.is_none() {
                let (_, style3d, _) = border.box_get_3d_style();
                let strokes = border.box_get_strokes();
                let thickness = |index: usize| {
                    strokes[index*2+1].as_ref().filter(|stroke|stroke.stroke_is_visible()).map_or(0.,|stroke|if index==0{2.5}else{0.}+stroke.stroke_get_thickness()*if style3d{4.}else{2.})
                };
                if top.is_none() {
                    top = Some(thickness(0));
                }
                if right.is_none() {
                    right = Some(thickness(1));
                }
                if bottom.is_none() {
                    bottom = Some(thickness(2));
                }
                if left.is_none() {
                    left = Some(thickness(3));
                }
            }
        }
        CfxRectF::new(
            left.unwrap_or(0.),
            top.unwrap_or(0.),
            right.unwrap_or(0.),
            bottom.unwrap_or(0.),
        )
    }
    pub fn is_horizontal_scroll_policy_off(&self) -> bool {
        self.get_ui_child_node()
            .is_some_and(|node| node.get_enum(A::HScrollPolicy) == V::Off)
    }
    pub fn is_vertical_scroll_policy_off(&self) -> bool {
        self.get_ui_child_node()
            .is_some_and(|node| node.get_enum(A::VScrollPolicy) == V::Off)
    }
    pub fn get_number_of_cells(&self) -> Option<i32> {
        Some(
            self.get_ui_child_node()?
                .get_child(0, E::Comb, false)?
                .get_integer(A::NumberOfCells),
        )
    }
    pub fn is_multi_line(&self) -> bool {
        self.get_ui_child_node()
            .is_some_and(|node| node.get_boolean(A::MultiLine))
    }
    pub fn get_max_chars(&self) -> (E, i32) {
        if let Some(child) = self
            .get_child(0, E::Value, false)
            .and_then(|node| node.first_child())
        {
            match child.get_element_type() {
                E::Text => return (E::Text, child.get_integer(A::MaxChars)),
                E::ExData => return (E::ExData, child.get_integer(A::MaxLength).max(0)),
                _ => {}
            }
        }
        (E::Unknown, 0)
    }
}
