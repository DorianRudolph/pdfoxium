// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Source hit-test classifications.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(u8)]
pub enum FwlWidgetHit {
    #[default]
    Unknown = 0,
    Client,
    Left,
    Top,
    Right,
    Bottom,
    LeftTop,
    RightTop,
    LeftBottom,
    RightBottom,
    Titlebar,
    MinBox,
    MaxBox,
    CloseBox,
    HScrollBar,
    VScrollBar,
    Border,
    Edge,
    Edit,
    HyperLink,
    UpButton,
    DownButton,
}
