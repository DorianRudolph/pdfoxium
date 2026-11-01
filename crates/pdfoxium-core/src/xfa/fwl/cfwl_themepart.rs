// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `xfa/fwl/cfwl_themepart.{h,cpp}`. Call-scoped widget and optional rectangle
//! data replace unowned native pointers.
use super::{cfwl_widget::FwlWidget, theme::cfwl_utils::ThemeState};
use crate::fxcrt::fx_coordinates::{CfxMatrix, CfxRectF};
use std::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign, Not};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PartState(pub u16);
impl PartState {
    pub const NORMAL: Self = Self(0);
    pub const CHECKED: Self = Self(1 << 1);
    pub const DEFAULT: Self = Self(1 << 2);
    pub const DISABLED: Self = Self(1 << 3);
    pub const FLAGGED: Self = Self(1 << 4);
    pub const FOCUSED: Self = Self(1 << 5);
    pub const HIGHLIGHT: Self = Self(1 << 6);
    pub const HOVERED: Self = Self(1 << 7);
    pub const NEUTRAL: Self = Self(1 << 9);
    pub const PRESSED: Self = Self(1 << 10);
    pub const READ_ONLY: Self = Self(1 << 11);
    pub const L_SELECTED: Self = Self(1 << 12);
    pub const R_SELECTED: Self = Self(1 << 13);
    pub const SELECTED: Self = Self(1 << 14);
    pub const fn empty() -> Self {
        Self::NORMAL
    }
    pub const fn bits(self) -> u16 {
        self.0
    }
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
    pub const fn contains(self, state: Self) -> bool {
        self.0 & state.0 == state.0
    }
    pub const fn intersects(self, state: Self) -> bool {
        self.0 & state.0 != 0
    }
    pub fn insert(&mut self, state: Self) {
        self.0 |= state.0;
    }
    pub fn remove(&mut self, state: Self) {
        self.0 &= !state.0;
    }
}
impl BitOr for PartState {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}
impl BitAnd for PartState {
    type Output = Self;
    fn bitand(self, rhs: Self) -> Self {
        Self(self.0 & rhs.0)
    }
}
impl Not for PartState {
    type Output = Self;
    fn not(self) -> Self {
        Self(!self.0)
    }
}
impl BitOrAssign for PartState {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}
impl BitAndAssign for PartState {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum Part {
    #[default]
    None = 0,
    BackArrow,
    Background,
    Border,
    Caption,
    Check,
    CheckBox,
    CloseBox,
    CombTextLine,
    DateInBK,
    DateInCircle,
    DatesIn,
    DatesOut,
    DownButton,
    DropDownButton,
    ForeArrow,
    HSeparator,
    HeadText,
    Header,
    Icon,
    Image,
    LBtn,
    ListItem,
    LowerTrack,
    MinimizeBox,
    MaximizeBox,
    NarrowCaption,
    RBtn,
    Thumb,
    ThumbBackArrow,
    ThumbForeArrow,
    ThumbLowerTrack,
    ThumbThumb,
    ThumbUpperTrack,
    Today,
    TodayCircle,
    UpButton,
    UpperTrack,
    VSeparator,
    Week,
    WeekNum,
    WeekNumSep,
}
#[derive(Clone)]
pub struct CfwlThemePart<'a> {
    pub part: Part,
    pub widget: &'a dyn FwlWidget,
    pub matrix: CfxMatrix,
    pub part_rect: CfxRectF,
    pub data_rect: Option<CfxRectF>,
    pub states: PartState,
    pub maximize: bool,
    pub static_background: bool,
}
impl<'a> CfwlThemePart<'a> {
    pub fn new(part: Part, widget: &'a dyn FwlWidget) -> Self {
        Self {
            part,
            widget,
            matrix: CfxMatrix::default(),
            part_rect: CfxRectF::default(),
            data_rect: None,
            states: PartState::NORMAL,
            maximize: false,
            static_background: false,
        }
    }
    pub fn get_part(&self) -> Part {
        self.part
    }
    pub fn get_widget(&self) -> &'a dyn FwlWidget {
        self.widget
    }
    pub fn get_theme_state(&self) -> ThemeState {
        theme_state(self.states)
    }
}
pub fn theme_state(states: PartState) -> ThemeState {
    if states.intersects(PartState::DISABLED) {
        ThemeState::Disable
    } else if states.intersects(PartState::PRESSED) {
        ThemeState::Pressed
    } else if states.intersects(PartState::HOVERED) {
        ThemeState::Hover
    } else {
        ThemeState::Normal
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_state_preserves_native_precedence_and_values() {
        for (states, expected) in [
            (PartState::NORMAL, ThemeState::Normal),
            (PartState::CHECKED | PartState::FOCUSED, ThemeState::Normal),
            (PartState::HOVERED, ThemeState::Hover),
            (PartState::PRESSED | PartState::HOVERED, ThemeState::Pressed),
            (
                PartState::DISABLED | PartState::PRESSED | PartState::HOVERED,
                ThemeState::Disable,
            ),
        ] {
            assert_eq!(theme_state(states), expected);
        }
        assert_eq!(
            [
                ThemeState::Normal as u8,
                ThemeState::Hover as u8,
                ThemeState::Pressed as u8,
                ThemeState::Disable as u8
            ],
            [1, 2, 3, 4]
        );
        assert_eq!(Part::WeekNumSep as u8, 41);
    }
}
