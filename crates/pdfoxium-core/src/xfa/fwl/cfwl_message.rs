// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_message`, `cfwl_messagekey`, `cfwl_messagekillfocus`,
//! `cfwl_messagesetfocus`, `cfwl_messagemouse`, `cfwl_messagemousewheel`.
//! An owned enum replaces native downcasts; widget references are scoped owners.
use super::cfwl_widget::WidgetRef;
use crate::fxcrt::fx_coordinates::{CfxPointF, CfxVector};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageType {
    Key,
    KillFocus,
    Mouse,
    MouseWheel,
    SetFocus,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum KeyCommand {
    KeyDown,
    Char,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum MouseCommand {
    LeftButtonDown,
    LeftButtonUp,
    LeftButtonDblClk,
    RightButtonDown,
    RightButtonUp,
    RightButtonDblClk,
    Move,
    Enter,
    Leave,
    Hover,
}
#[derive(Clone)]
pub enum MessageData {
    Key {
        command: KeyCommand,
        flags: u32,
        key_code_or_char: u32,
    },
    KillFocus {
        set_focus: Option<WidgetRef>,
    },
    SetFocus,
    Mouse {
        command: MouseCommand,
        flags: u32,
        pos: CfxPointF,
    },
    MouseWheel {
        pos: CfxPointF,
        delta: CfxVector,
    },
}
#[derive(Clone)]
pub struct FwlMessage {
    pub destination: Option<WidgetRef>,
    pub data: MessageData,
}
pub type CfwlMessage = FwlMessage;
impl FwlMessage {
    pub fn new(destination: Option<WidgetRef>, data: MessageData) -> Self {
        Self { destination, data }
    }
    pub fn key(
        destination: Option<WidgetRef>,
        command: KeyCommand,
        flags: u32,
        key_code_or_char: u32,
    ) -> Self {
        Self::new(
            destination,
            MessageData::Key {
                command,
                flags,
                key_code_or_char,
            },
        )
    }
    pub fn mouse(
        destination: Option<WidgetRef>,
        command: MouseCommand,
        flags: u32,
        pos: CfxPointF,
    ) -> Self {
        Self::new(
            destination,
            MessageData::Mouse {
                command,
                flags,
                pos,
            },
        )
    }
    pub fn mouse_wheel(destination: Option<WidgetRef>, pos: CfxPointF, delta: CfxVector) -> Self {
        Self::new(destination, MessageData::MouseWheel { pos, delta })
    }
    pub fn set_focus(destination: Option<WidgetRef>) -> Self {
        Self::new(destination, MessageData::SetFocus)
    }
    pub fn kill_focus(destination: Option<WidgetRef>) -> Self {
        Self::new(destination, MessageData::KillFocus { set_focus: None })
    }
    pub fn get_type(&self) -> MessageType {
        match self.data {
            MessageData::Key { .. } => MessageType::Key,
            MessageData::KillFocus { .. } => MessageType::KillFocus,
            MessageData::SetFocus => MessageType::SetFocus,
            MessageData::Mouse { .. } => MessageType::Mouse,
            MessageData::MouseWheel { .. } => MessageType::MouseWheel,
        }
    }
    pub fn is_focused_on_widget(&self, widget: &WidgetRef) -> bool {
        matches!(&self.data, MessageData::KillFocus { set_focus: Some(focus) } if std::rc::Rc::ptr_eq(focus, widget))
    }
}
