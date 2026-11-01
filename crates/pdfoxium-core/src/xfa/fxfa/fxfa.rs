// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com

//! Complete portable enums and masks from `xfa/fxfa/fxfa.h`.
#![allow(non_camel_case_types, non_upper_case_globals)]
/// Values match JSPLATFORM_ALERT_BUTTON_*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum AlertButton {
    kOK = 0,
    kOKCancel = 1,
    kYesNo = 2,
    kYesNoCancel = 3,
}
impl AlertButton {
    pub const kDefault: Self = Self::kOK;
}
/// Values match JSPLATFORM_ALERT_ICON_*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum AlertIcon {
    kError = 0,
    kWarning = 1,
    kQuestion = 2,
    kStatus = 3,
    kAsterisk = 4,
}
impl AlertIcon {
    pub const kDefault: Self = Self::kError;
}
/// Values match JSPLATFORM_ALERT_RETURN_*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum AlertReturn {
    kOK = 1,
    kCancel = 2,
    kNo = 3,
    kYes = 4,
}
/// Values match FORMTYPE_*.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum FormType {
    kNone = 0,
    kAcroForm = 1,
    kXFAFull = 2,
    kXFAForeground = 3,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub enum XfaEventError {
    kError = -1,
    kNotExist = 0,
    kSuccess = 1,
    kDisabled = 2,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct XfaPrintOpt(pub u8);
impl XfaPrintOpt {
    pub const kShowDialog: Self = Self(1 << 0);
    pub const kCanCancel: Self = Self(1 << 1);
    pub const kShrinkPage: Self = Self(1 << 2);
    pub const kAsImage: Self = Self(1 << 3);
    pub const kReverseOrder: Self = Self(1 << 4);
    pub const kPrintAnnot: Self = Self(1 << 5);
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(transparent)]
pub struct XfaWidgetStatus(pub u16);
impl XfaWidgetStatus {
    pub const kNone: Self = Self(0);
    pub const kAccess: Self = Self(1 << 0);
    pub const kButtonDown: Self = Self(1 << 1);
    pub const kDisabled: Self = Self(1 << 2);
    pub const kFocused: Self = Self(1 << 3);
    pub const kPrintable: Self = Self(1 << 4);
    pub const kRectCached: Self = Self(1 << 5);
    pub const kTextEditValueChanged: Self = Self(1 << 6);
    pub const kViewable: Self = Self(1 << 7);
    pub const kVisible: Self = Self(1 << 8);
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }
}
macro_rules! mask_ops {
    ($name:ident) => {
        impl std::ops::BitOr for $name {
            type Output = Self;
            fn bitor(self, rhs: Self) -> Self {
                Self(self.0 | rhs.0)
            }
        }
        impl std::ops::BitAnd for $name {
            type Output = Self;
            fn bitand(self, rhs: Self) -> Self {
                Self(self.0 & rhs.0)
            }
        }
        impl std::ops::BitOrAssign for $name {
            fn bitor_assign(&mut self, rhs: Self) {
                self.0 |= rhs.0;
            }
        }
        impl std::ops::BitAndAssign for $name {
            fn bitand_assign(&mut self, rhs: Self) {
                self.0 &= rhs.0;
            }
        }
        impl std::ops::Not for $name {
            type Output = Self;
            fn not(self) -> Self {
                Self(!self.0)
            }
        }
    };
}
mask_ops!(XfaPrintOpt);
mask_ops!(XfaWidgetStatus);
