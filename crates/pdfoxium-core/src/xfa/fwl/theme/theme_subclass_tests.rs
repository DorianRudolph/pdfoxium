// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Actual subclass dispatch and pinned state colors, including themes whose
//! document-bound control consumer is still deferred.
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxRectF},
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::{
        fgas::graphics::cfgas_gegraphics::CfgasGeGraphics,
        fwl::{
            cfwl_themebackground::CfwlThemeBackground,
            cfwl_themepart::{Part, PartState},
            cfwl_themetext::CfwlThemeText,
            cfwl_widget::{CfwlWidget, FwlType, FwlWidget, Properties},
            test_support::new_app,
            theme::{cfwl_checkboxtp, cfwl_monthcalendartp},
        },
    },
};
use std::rc::Rc;

struct Widget {
    base: CfwlWidget,
    kind: FwlType,
}
impl FwlWidget for Widget {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        self.kind
    }
    fn update(&self) {}
    fn draw_widget(&self, _graphics: &mut CfgasGeGraphics, _matrix: CfxMatrix) {}
}
fn render(
    kind: FwlType,
    part: Part,
    states: PartState,
    static_background: bool,
    data_rect: Option<CfxRectF>,
) -> CfxDibitmap {
    let (app, _host) = new_app();
    let widget = Rc::new(Widget {
        base: CfwlWidget::new(&app, Properties::default(), None),
        kind,
    });
    app.register_widget(widget.clone());
    let mut params = CfwlThemeBackground::new(part, widget.as_ref());
    params.part_rect = CfxRectF::new(4., 4., 24., 24.);
    params.states = states;
    params.static_background = static_background;
    params.data_rect = data_rect;
    let mut bitmap = CfxDibitmap::create(32, 32, FxdibFormat::Bgra, 0).unwrap();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        let mut graphics = CfgasGeGraphics::new(&mut device);
        let original = graphics.get_clip_rect();
        app.theme_provider().draw_background(&params, &mut graphics);
        assert_eq!(graphics.get_clip_rect(), original);
    }
    bitmap
}
fn pixel(bitmap: &CfxDibitmap, x: usize, y: i32) -> u32 {
    u32::from_le_bytes(
        bitmap.scanline(y).unwrap()[x * 4..x * 4 + 4]
            .try_into()
            .unwrap(),
    )
}

#[test]
fn background_dispatch_retains_source_state_colors() {
    for (kind, part, states, static_background, expected) in [
        (
            FwlType::Barcode,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffece9d8,
        ),
        (
            FwlType::Caret,
            Part::Background,
            PartState::HIGHLIGHT,
            false,
            0xff000000,
        ),
        (
            FwlType::Caret,
            Part::Background,
            PartState::NORMAL,
            false,
            0,
        ),
        (
            FwlType::ComboBox,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::ComboBox,
            Part::Background,
            PartState::SELECTED,
            false,
            0xff99c1da,
        ),
        (
            FwlType::ComboBox,
            Part::Background,
            PartState::DISABLED,
            false,
            0xfff1efe2,
        ),
        (
            FwlType::Edit,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::Edit,
            Part::Background,
            PartState::READ_ONLY,
            false,
            0xffece9d8,
        ),
        (
            FwlType::Edit,
            Part::Background,
            PartState::DISABLED,
            false,
            0xfff1efe2,
        ),
        (
            FwlType::Edit,
            Part::Background,
            PartState::DISABLED,
            true,
            0xffece9d8,
        ),
        (
            FwlType::ListBox,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::ListBox,
            Part::ListItem,
            PartState::SELECTED,
            false,
            0xff99c1da,
        ),
        (
            FwlType::ListBox,
            Part::Check,
            PartState::NORMAL,
            false,
            0xff0000ff,
        ),
        (
            FwlType::ListBox,
            Part::Check,
            PartState::CHECKED,
            false,
            0xffff0000,
        ),
        (
            FwlType::ListBox,
            Part::Check,
            PartState::CHECKED | PartState::FOCUSED,
            false,
            0xff000000,
        ),
        (
            FwlType::MonthCalendar,
            Part::Header,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::MonthCalendar,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::MonthCalendar,
            Part::DatesIn,
            PartState::HOVERED,
            false,
            0xffc1d3fb,
        ),
        (
            FwlType::MonthCalendar,
            Part::DatesIn,
            PartState::SELECTED,
            false,
            0xffadbcef,
        ),
        (
            FwlType::MonthCalendar,
            Part::DatesIn,
            PartState::SELECTED | PartState::HOVERED,
            false,
            0xffadbcef,
        ),
        (
            FwlType::PushButton,
            Part::Background,
            PartState::NORMAL,
            false,
            0xffffffff,
        ),
        (
            FwlType::PushButton,
            Part::Background,
            PartState::DISABLED | PartState::DEFAULT,
            false,
            0xfff5f4ea,
        ),
        (
            FwlType::ScrollBar,
            Part::Thumb,
            PartState::NORMAL,
            false,
            0xffb6cdfb,
        ),
        (
            FwlType::ScrollBar,
            Part::LowerTrack,
            PartState::NORMAL,
            false,
            0xfffefefb,
        ),
    ] {
        let bitmap = render(kind, part, states, static_background, None);
        assert_eq!(
            pixel(&bitmap, 16, 16),
            expected,
            "{kind:?}/{part:?}/{states:?}"
        );
        assert_eq!(pixel(&bitmap, 0, 0), 0);
    }
    let bitmap = render(
        FwlType::ListBox,
        Part::Background,
        PartState::NORMAL,
        false,
        Some(CfxRectF::new(8., 8., 8., 8.)),
    );
    assert_eq!(pixel(&bitmap, 10, 10), 0xffece9d8);
    assert_eq!(pixel(&bitmap, 24, 24), 0xffffffff);
}

#[test]
fn remaining_theme_parts_draw_arrows_borders_and_markers() {
    for (kind, part, state) in [
        (
            FwlType::DateTimePicker,
            Part::DropDownButton,
            PartState::NORMAL,
        ),
        (FwlType::ComboBox, Part::DropDownButton, PartState::PRESSED),
        (FwlType::PictureBox, Part::Border, PartState::NORMAL),
        (FwlType::Barcode, Part::Border, PartState::NORMAL),
        (FwlType::CheckBox, Part::CheckBox, PartState::CHECKED),
        (FwlType::ScrollBar, Part::ForeArrow, PartState::NORMAL),
        (FwlType::ScrollBar, Part::BackArrow, PartState::DISABLED),
        (FwlType::MonthCalendar, Part::LBtn, PartState::HOVERED),
        (FwlType::MonthCalendar, Part::RBtn, PartState::PRESSED),
        (FwlType::MonthCalendar, Part::HSeparator, PartState::NORMAL),
        (FwlType::MonthCalendar, Part::WeekNumSep, PartState::NORMAL),
        (
            FwlType::MonthCalendar,
            Part::DateInCircle,
            PartState::NORMAL,
        ),
        (FwlType::MonthCalendar, Part::TodayCircle, PartState::NORMAL),
    ] {
        let bitmap = render(kind, part, state, false, None);
        assert!(
            bitmap.buffer().chunks_exact(4).any(|pixel| pixel[3] != 0),
            "{kind:?}/{part:?}"
        );
        assert_eq!(pixel(&bitmap, 0, 0), 0);
    }
}

#[test]
fn checkbox_and_calendar_text_color_precedence_matches_source() {
    let (app, _host) = new_app();
    let widget = Rc::new(Widget {
        base: CfwlWidget::new(&app, Properties::default(), None),
        kind: FwlType::MonthCalendar,
    });
    app.register_widget(widget.clone());
    for (part, state, expected) in [
        (Part::Caption, PartState::NORMAL, 0xff0099ff),
        (Part::DatesIn, PartState::HOVERED, 0xffffffff),
        (Part::DatesIn, PartState::SELECTED, 0xffffffff),
        (
            Part::DatesIn,
            PartState::SELECTED | PartState::FLAGGED,
            0xff000000,
        ),
        (Part::DatesOut, PartState::HOVERED, 0xff000000),
    ] {
        let mut params = CfwlThemeText::new(part, widget.as_ref());
        params.states = state;
        assert_eq!(cfwl_monthcalendartp::text_color(&params), expected);
    }
    let mut params = CfwlThemeText::new(Part::Caption, widget.as_ref());
    assert_eq!(cfwl_checkboxtp::text_color(&params), 0xff000000);
    params.states = PartState::DISABLED | PartState::FOCUSED;
    assert_eq!(cfwl_checkboxtp::text_color(&params), 0xffaca899);
}
