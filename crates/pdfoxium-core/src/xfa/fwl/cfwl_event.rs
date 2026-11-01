// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2016 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! `cfwl_event` and its mouse, scroll, selection, text-change and validate types.
//! A typed payload retains the native synchronous mutation/cancellation contract.
use super::{cfwl_message::MouseCommand, cfwl_widget::WidgetRef};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EventType {
    CheckStateChanged,
    Click,
    Close,
    EditChanged,
    Mouse,
    PostDropDown,
    PreDropDown,
    Scroll,
    SelectChanged,
    TextWillChange,
    TextFull,
    Validate,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum ScrollCode {
    None = 1,
    Min,
    Max,
    PageBackward,
    PageForward,
    StepBackward,
    StepForward,
    Pos,
    TrackPos,
    EndScroll,
}
#[derive(Clone)]
pub enum EventData {
    CheckStateChanged,
    Click,
    Close,
    EditChanged,
    Mouse(MouseCommand),
    PostDropDown,
    PreDropDown,
    Scroll {
        code: ScrollCode,
        pos: f32,
    },
    SelectChanged {
        left_button_up: bool,
        year: i32,
        month: i32,
        day: i32,
    },
    TextWillChange {
        change_text: Vec<u32>,
        previous_text: Vec<u32>,
        selection_start: usize,
        selection_end: usize,
        cancelled: bool,
    },
    TextFull,
    Validate {
        insert: Vec<u32>,
        validate: bool,
    },
}
#[derive(Clone)]
pub struct FwlEvent {
    pub source: Option<WidgetRef>,
    pub destination: Option<WidgetRef>,
    pub data: EventData,
}
pub type CfwlEvent = FwlEvent;
impl FwlEvent {
    pub fn new(source: Option<WidgetRef>, data: EventData) -> Self {
        Self {
            source,
            destination: None,
            data,
        }
    }
    pub fn with_destination(
        source: Option<WidgetRef>,
        destination: Option<WidgetRef>,
        data: EventData,
    ) -> Self {
        Self {
            source,
            destination,
            data,
        }
    }
    pub fn get_type(&self) -> EventType {
        match self.data {
            EventData::CheckStateChanged => EventType::CheckStateChanged,
            EventData::Click => EventType::Click,
            EventData::Close => EventType::Close,
            EventData::EditChanged => EventType::EditChanged,
            EventData::Mouse(_) => EventType::Mouse,
            EventData::PostDropDown => EventType::PostDropDown,
            EventData::PreDropDown => EventType::PreDropDown,
            EventData::Scroll { .. } => EventType::Scroll,
            EventData::SelectChanged { .. } => EventType::SelectChanged,
            EventData::TextWillChange { .. } => EventType::TextWillChange,
            EventData::TextFull => EventType::TextFull,
            EventData::Validate { .. } => EventType::Validate,
        }
    }
    pub fn mouse(
        source: Option<WidgetRef>,
        destination: Option<WidgetRef>,
        command: MouseCommand,
    ) -> Self {
        Self::with_destination(source, destination, EventData::Mouse(command))
    }
    pub fn scroll(source: Option<WidgetRef>, code: ScrollCode, pos: f32) -> Self {
        Self::new(source, EventData::Scroll { code, pos })
    }
    pub fn select_changed(source: Option<WidgetRef>, left_button_up: bool) -> Self {
        Self::new(
            source,
            EventData::SelectChanged {
                left_button_up,
                year: -1,
                month: -1,
                day: -1,
            },
        )
    }
    pub fn date_selected(source: Option<WidgetRef>, year: i32, month: i32, day: i32) -> Self {
        Self::new(
            source,
            EventData::SelectChanged {
                left_button_up: false,
                year,
                month,
                day,
            },
        )
    }
    pub fn text_will_change(
        source: Option<WidgetRef>,
        change_text: Vec<u32>,
        previous_text: Vec<u32>,
        selection_start: usize,
        selection_end: usize,
    ) -> Self {
        Self::new(
            source,
            EventData::TextWillChange {
                change_text,
                previous_text,
                selection_start,
                selection_end,
                cancelled: false,
            },
        )
    }
    pub fn validate(source: Option<WidgetRef>, insert: Vec<u32>) -> Self {
        Self::new(
            source,
            EventData::Validate {
                insert,
                validate: true,
            },
        )
    }
}
