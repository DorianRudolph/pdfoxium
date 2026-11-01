// Copyright 2014 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Direct `xfa/fwl/cfwl_monthcalendar.{h,cpp}` port. Boa supplies today's
//! local date; the existing PDF date helpers supply Gregorian calculations.
//! Day entries have stable shared owners, with no collection borrow across
//! host font, theme, repaint or selection callbacks.
use super::{
    cfwl_app::CfwlApp,
    cfwl_message::{FwlMessage, MessageData, MouseCommand},
    cfwl_themebackground::CfwlThemeBackground,
    cfwl_themepart::{Part, PartState},
    cfwl_themetext::CfwlThemeText,
    cfwl_widget::{dispatch_message_to, CfwlWidget, FwlType, FwlWidget, Properties, WidgetRef},
};
use crate::{
    fxcrt::fx_coordinates::{CfxMatrix, CfxPointF, CfxRectF, CfxSizeF},
    fxjs::{fx_date_helpers as dates, js_date},
    xfa::{fde::cfde_data::FdeTextAlignment, fgas::graphics::cfgas_gegraphics::CfgasGeGraphics},
};
use boa_engine::Context;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const HSEP_HEIGHT: f32 = 1.;
const HMARGIN: f32 = 3.;
const VMARGIN: f32 = 2.;
const ROWS: f32 = 9.;
const COLUMNS: f32 = 7.;
const HEADER_VMARGIN: f32 = 7.;
const HEADER_HMARGIN: f32 = 5.;
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
struct Date {
    year: i32,
    month: i32,
    day: i32,
}
impl Date {
    const fn new(year: i32, month: i32, day: i32) -> Self {
        Self { year, month, day }
    }
}
struct DateInfo {
    day: i32,
    day_of_week: i32,
    flagged: bool,
    selected: Cell<bool>,
    rect: Cell<CfxRectF>,
    text: Vec<u32>,
}
impl DateInfo {
    fn part_states(&self) -> PartState {
        let mut state = PartState::NORMAL;
        if self.flagged {
            state |= PartState::FLAGGED;
        }
        if self.selected.get() {
            state |= PartState::SELECTED;
        }
        state
    }
}
fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}
fn valid_month(year: i32, month: i32) -> bool {
    (1..=9999).contains(&year) && (1..=12).contains(&month)
}
fn month_days(year: i32, month: i32) -> i32 {
    if !valid_month(year, month) {
        return 0;
    }
    (dates::make_day(year, month, 1) - dates::make_day(year, month - 1, 1)) as i32
}
fn weekday(year: i32, month: i32) -> i32 {
    dates::get_day_of_week_from_time(dates::make_date(dates::make_day(year, month - 1, 1), 0.))
}
fn date_from_context(context: &Context) -> Date {
    let time = js_date::current_date_time(context);
    Date::new(
        dates::get_year_from_time(time),
        dates::get_month_from_time(time) + 1,
        dates::get_day_from_time(time),
    )
}

pub struct CfwlMonthCalendar {
    base: CfwlWidget,
    today: Cell<Option<Date>>,
    displayed: Cell<Date>,
    head_rect: Cell<CfxRectF>,
    week_rect: Cell<CfxRectF>,
    left_button_rect: Cell<CfxRectF>,
    right_button_rect: Cell<CfxRectF>,
    dates_rect: Cell<CfxRectF>,
    separator_rect: Cell<CfxRectF>,
    head_text_rect: Cell<CfxRectF>,
    today_rect: Cell<CfxRectF>,
    today_flag_rect: Cell<CfxRectF>,
    date_array: RefCell<Vec<Rc<DateInfo>>>,
    hovered: Cell<i32>,
    left_button_states: Cell<PartState>,
    right_button_states: Cell<PartState>,
    head_size: Cell<CfxSizeF>,
    cell_size: Cell<CfxSizeF>,
    today_size: Cell<CfxSizeF>,
    selected_days: RefCell<Vec<i32>>,
    client_rect: Cell<CfxRectF>,
    generation: Cell<u64>,
}
impl CfwlMonthCalendar {
    pub fn new(app: &Rc<CfwlApp>, properties: Properties, outer: Option<WidgetRef>) -> Rc<Self> {
        let calendar = Rc::new(Self {
            base: CfwlWidget::new(app, properties, outer),
            today: Cell::new(None),
            displayed: Cell::new(Date::new(2011, 1, 1)),
            head_rect: Cell::new(CfxRectF::default()),
            week_rect: Cell::new(CfxRectF::default()),
            left_button_rect: Cell::new(CfxRectF::default()),
            right_button_rect: Cell::new(CfxRectF::default()),
            dates_rect: Cell::new(CfxRectF::default()),
            separator_rect: Cell::new(CfxRectF::default()),
            head_text_rect: Cell::new(CfxRectF::default()),
            today_rect: Cell::new(CfxRectF::default()),
            today_flag_rect: Cell::new(CfxRectF::default()),
            date_array: RefCell::new(Vec::new()),
            hovered: Cell::new(-1),
            left_button_states: Cell::new(PartState::NORMAL),
            right_button_states: Cell::new(PartState::NORMAL),
            head_size: Cell::new(CfxSizeF::default()),
            cell_size: Cell::new(CfxSizeF::default()),
            today_size: Cell::new(CfxSizeF::default()),
            selected_days: RefCell::new(Vec::new()),
            client_rect: Cell::new(CfxRectF::default()),
            generation: Cell::new(0),
        });
        app.register_widget(calendar.clone());
        calendar
    }
    fn ensure_initialized(&self) {
        if self.today.get().is_none() {
            self.set_today_from_context(&Context::default());
        }
    }
    /// Use the caller's existing Boa host hooks, including its local timezone.
    /// No separate clock, timezone implementation or JavaScript Date override.
    pub fn set_today_from_context(&self, context: &Context) {
        let today = date_from_context(context);
        self.today.set(Some(today));
        self.displayed.set(Date::new(today.year, today.month, 1));
        self.reset_date_items();
        self.layout();
    }
    pub fn today(&self) -> (i32, i32, i32) {
        self.ensure_initialized();
        let date = self.today.get().expect("initialized date");
        (date.year, date.month, date.day)
    }
    pub fn displayed_month(&self) -> (i32, i32) {
        self.ensure_initialized();
        let date = self.displayed.get();
        (date.year, date.month)
    }
    pub fn selection(&self) -> Option<(i32, i32, i32)> {
        let day = self.selected_days.borrow().first().copied()?;
        let date = self.displayed.get();
        Some((date.year, date.month, day))
    }
    pub fn set_select(&self, year: i32, month: i32, day: i32) {
        self.ensure_initialized();
        if day < 1 || day > month_days(year, month) {
            return;
        }
        self.change_to_month(year, month);
        self.add_selected_day(day);
    }
    pub fn day_rect(&self, day: i32) -> CfxRectF {
        if day < 1 {
            return CfxRectF::default();
        }
        self.date_array
            .borrow()
            .get((day - 1) as usize)
            .map_or(CfxRectF::default(), |info| info.rect.get())
    }
    pub fn left_button_rect(&self) -> CfxRectF {
        self.left_button_rect.get()
    }
    pub fn right_button_rect(&self) -> CfxRectF {
        self.right_button_rect.get()
    }
    pub fn today_rect(&self) -> CfxRectF {
        self.today_rect.get()
    }
    pub fn date_count(&self) -> usize {
        self.date_array.borrow().len()
    }
    pub fn hovered_day(&self) -> i32 {
        self.hovered.get()
    }
    pub fn next_month(&self) {
        self.ensure_initialized();
        let date = self.displayed.get();
        if date >= Date::new(2200, 1, 1) {
            return;
        }
        let (year, month) = if date.month == 12 {
            (date.year + 1, 1)
        } else {
            (date.year, date.month + 1)
        };
        self.change_to_month(year, month);
    }
    pub fn prev_month(&self) {
        self.ensure_initialized();
        let date = self.displayed.get();
        if date <= Date::new(1500, 12, 1) {
            return;
        }
        let (year, month) = if date.month == 1 {
            (date.year - 1, 12)
        } else {
            (date.year, date.month - 1)
        };
        self.change_to_month(year, month);
    }
    pub fn jump_to_today(&self) {
        self.ensure_initialized();
        let date = self.today.get().unwrap();
        if self.displayed.get().year != date.year || self.displayed.get().month != date.month {
            self.change_to_month(date.year, date.month);
        }
        self.add_selected_day(date.day);
    }
    fn reset_date_items(&self) {
        let date = self.displayed.get();
        let today = self.today.get();
        let first_weekday = weekday(date.year, date.month);
        let selected = self.selected_days.borrow();
        let items = (1..=month_days(date.year, date.month))
            .map(|day| {
                Rc::new(DateInfo {
                    day,
                    day_of_week: (first_weekday + day - 1) % 7,
                    flagged: today == Some(Date::new(date.year, date.month, day)),
                    selected: Cell::new(selected.contains(&day)),
                    rect: Cell::new(CfxRectF::default()),
                    text: wide(&day.to_string()),
                })
            })
            .collect();
        drop(selected);
        *self.date_array.borrow_mut() = items;
        self.generation.set(self.generation.get().wrapping_add(1));
    }
    fn change_to_month(&self, year: i32, month: i32) {
        if !valid_month(year, month) {
            return;
        }
        self.displayed.set(Date::new(year, month, 1));
        self.hovered.set(-1);
        // A selection in a different month must not become an invalid day or
        // silently refer to another date. The native list was day-only.
        self.selected_days.borrow_mut().clear();
        self.reset_date_items();
        self.calculate_date_items();
    }
    fn add_selected_day(&self, day: i32) {
        if day < 1 || day > self.date_array.borrow().len() as i32 {
            return;
        }
        if self.selected_days.borrow().as_slice() == [day] {
            return;
        }
        // Source's inverted Contains test prevented the first selection.
        for item in self.date_array.borrow().iter() {
            item.selected.set(item.day == day);
        }
        let mut selected = self.selected_days.borrow_mut();
        selected.clear();
        selected.push(day);
    }
    fn get_head_text(year: i32, month: i32) -> Vec<u32> {
        wide(&format!(
            "{}, {}",
            dates::FULL_MONTHS[(month - 1) as usize],
            year
        ))
    }
    fn get_today_text(date: Date) -> Vec<u32> {
        wide(&format!("Today, {}/{}/{}", date.day, date.month, date.year))
    }
    fn calc_size(&self) -> CfxSizeF {
        self.ensure_initialized();
        let mut max_week = CfxSizeF::default();
        for day in WEEKDAYS {
            let size = self.base.calc_text_size(&wide(day), false);
            max_week.width = max_week.width.max(size.width);
            max_week.height = max_week.height.max(size.height);
        }
        let mut max_day = CfxSizeF::default();
        for day in 10..=31 {
            let size = self.base.calc_text_size(&wide(&day.to_string()), false);
            max_day.width = max_day.width.max(size.width);
            max_day.height = max_day.height.max(size.height);
        }
        let cell = CfxSizeF::new(
            (0.5 + max_week.width.max(max_day.width)) as i32 as f32,
            max_week.height.max(max_day.height),
        );
        self.cell_size.set(cell);
        let mut size = CfxSizeF::new(
            cell.width * COLUMNS + HMARGIN * COLUMNS * 2. + HEADER_HMARGIN * 2.,
            0.,
        );
        let mut max_month = CfxSizeF::default();
        for month in dates::FULL_MONTHS {
            let current = self.base.calc_text_size(&wide(month), false);
            max_month.width = max_month.width.max(current.width);
            max_month.height = max_month.height.max(current.height);
        }
        let date = self.today.get().unwrap();
        let year_size = self
            .base
            .calc_text_size(&Self::get_head_text(date.year, date.month), false);
        let head = CfxSizeF::new(
            max_month.width + year_size.width,
            max_month.height.max(year_size.height),
        );
        self.head_size.set(head);
        size.width = size
            .width
            .max(head.width + HEADER_HMARGIN * 2. + cell.width * 2.);
        let mut today_size = self.base.calc_text_size(&Self::get_today_text(date), false);
        today_size.height = today_size.height.max(cell.height);
        self.today_size.set(today_size);
        size.height = cell.width
            + cell.height * (ROWS - 2.)
            + today_size.height
            + VMARGIN * ROWS * 2.
            + HEADER_VMARGIN * 4.;
        size
    }
    fn calc_head_size(&self) {
        let client = self.client_rect.get();
        let head = self.head_size.get();
        let cell = self.cell_size.get();
        self.head_text_rect.set(CfxRectF::new(
            client.left + (client.width - head.width) / 2.,
            client.top + HEADER_VMARGIN + VMARGIN + (cell.width - head.height) / 2.,
            head.width,
            head.height,
        ));
    }
    fn calc_today_size(&self) {
        let client = self.client_rect.get();
        let dates = self.dates_rect.get();
        let cell = self.cell_size.get();
        let today = self.today_size.get();
        self.today_flag_rect.set(CfxRectF::new(
            client.left + HEADER_HMARGIN + HMARGIN,
            dates.bottom() + HEADER_VMARGIN + VMARGIN,
            cell.width,
            today.height,
        ));
        self.today_rect.set(CfxRectF::new(
            client.left + HEADER_HMARGIN + cell.width + HMARGIN * 2.,
            dates.bottom() + HEADER_VMARGIN + VMARGIN,
            today.width,
            today.height,
        ));
    }
    fn layout(&self) {
        let client = self.get_client_rect();
        self.client_rect.set(client);
        let cell = self.cell_size.get();
        let head = CfxRectF::new(
            client.left + HEADER_HMARGIN,
            client.top,
            client.width - HEADER_HMARGIN * 2.,
            cell.width + (HEADER_VMARGIN + VMARGIN) * 2.,
        );
        self.head_rect.set(head);
        let week = CfxRectF::new(
            client.left + HEADER_HMARGIN,
            head.bottom(),
            client.width - HEADER_HMARGIN * 2.,
            cell.height + VMARGIN * 2.,
        );
        self.week_rect.set(week);
        self.left_button_rect.set(CfxRectF::new(
            client.left + HEADER_HMARGIN,
            client.top + HEADER_VMARGIN,
            cell.width,
            cell.width,
        ));
        self.right_button_rect.set(CfxRectF::new(
            client.right() - HEADER_HMARGIN - cell.width,
            client.top + HEADER_VMARGIN,
            cell.width,
            cell.width,
        ));
        self.separator_rect.set(CfxRectF::new(
            client.left + HEADER_HMARGIN + HMARGIN,
            week.bottom() - VMARGIN,
            client.width - (HEADER_HMARGIN + HMARGIN) * 2.,
            HSEP_HEIGHT,
        ));
        self.dates_rect.set(CfxRectF::new(
            client.left + HEADER_HMARGIN,
            week.bottom(),
            client.width - HEADER_HMARGIN * 2.,
            cell.height * (ROWS - 3.) + VMARGIN * (ROWS - 3.) * 2.,
        ));
        self.calculate_date_items();
        self.calc_head_size();
        self.calc_today_size();
    }
    fn calculate_date_items(&self) {
        let dates = self.dates_rect.get();
        let cell = self.cell_size.get();
        let mut row = 0;
        for item in self.date_array.borrow().iter() {
            item.rect.set(CfxRectF::new(
                dates.left + item.day_of_week as f32 * (cell.width + HMARGIN * 2.),
                dates.top + row as f32 * (cell.height + VMARGIN * 2.),
                cell.width + HMARGIN * 2.,
                cell.height + VMARGIN * 2.,
            ));
            if item.day_of_week == 6 {
                row += 1;
            }
        }
    }
    fn background(
        &self,
        graphics: &mut CfgasGeGraphics,
        part: Part,
        rect: CfxRectF,
        states: PartState,
        matrix: CfxMatrix,
    ) {
        if let Some(app) = self.base.app() {
            let mut params = CfwlThemeBackground::new(part, self);
            params.part_rect = rect;
            params.states = states;
            params.matrix = matrix;
            app.theme_provider().draw_background(&params, graphics);
        }
    }
    fn text(
        &self,
        graphics: &mut CfgasGeGraphics,
        part: Part,
        rect: CfxRectF,
        text: Vec<u32>,
        states: PartState,
        align: FdeTextAlignment,
        matrix: CfxMatrix,
    ) {
        if let Some(app) = self.base.app() {
            let mut params = CfwlThemeText::new(part, self);
            params.part_rect = rect;
            params.states = states;
            params.matrix = matrix;
            params.text = text;
            params.tto_align = align;
            params.tto_styles.single_line = true;
            app.theme_provider().draw_text(&params, graphics);
        }
    }
    fn draw_date_backgrounds(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        let generation = self.generation.get();
        let count = self.date_array.borrow().len();
        for index in 0..count {
            let Some(item) = self.date_array.borrow().get(index).cloned() else {
                break;
            };
            let mut states = item.part_states();
            if !item.selected.get() && item.day == self.hovered.get() {
                states |= PartState::HOVERED;
            }
            // The native consumer uses DateInBK, but its theme dispatch
            // handles DatesIn. Use the handled part so selection/hover ink is visible.
            self.background(graphics, Part::DatesIn, item.rect.get(), states, matrix);
            if self.generation.get() != generation || self.base.is_destroyed() {
                break;
            }
        }
    }
    fn draw_dates(&self, graphics: &mut CfgasGeGraphics, matrix: CfxMatrix) {
        let generation = self.generation.get();
        let count = self.date_array.borrow().len();
        for index in 0..count {
            let Some(item) = self.date_array.borrow().get(index).cloned() else {
                break;
            };
            let mut states = item.part_states();
            if item.day == self.hovered.get() {
                states |= PartState::HOVERED;
            }
            self.text(
                graphics,
                Part::DatesIn,
                item.rect.get(),
                item.text.clone(),
                states,
                FdeTextAlignment::Center,
                matrix,
            );
            if self.generation.get() != generation || self.base.is_destroyed() {
                break;
            }
        }
    }
    pub fn get_day_at_point(&self, point: CfxPointF) -> i32 {
        self.date_array
            .borrow()
            .iter()
            .find(|item| item.rect.get().contains_point(point))
            .map_or(-1, |item| item.day)
    }
    fn on_lbutton_down(&self, point: CfxPointF) {
        if self.left_button_rect.get().contains_point(point) {
            self.left_button_states.set(PartState::PRESSED);
            self.prev_month();
            self.base.repaint_rect(self.client_rect.get());
        } else if self.right_button_rect.get().contains_point(point) {
            self.right_button_states.set(PartState::PRESSED);
            self.next_month();
            self.base.repaint_rect(self.client_rect.get());
        } else if self.today_rect.get().contains_point(point) {
            self.jump_to_today();
            self.base.repaint_rect(self.client_rect.get());
        }
    }
    fn on_lbutton_up(&self, point: CfxPointF) {
        if self.left_button_rect.get().contains_point(point) {
            self.left_button_states.set(PartState::NORMAL);
            self.base.repaint_rect(self.left_button_rect.get());
            return;
        }
        if self.right_button_rect.get().contains_point(point) {
            self.right_button_states.set(PartState::NORMAL);
            self.base.repaint_rect(self.right_button_rect.get());
            return;
        }
        if self.today_rect.get().contains_point(point) {
            return;
        }
        let day = self.get_day_at_point(point);
        if day < 1 {
            return;
        }
        self.add_selected_day(day);
        let date = self.displayed.get();
        if let Some(outer) = self.base.outer() {
            if let Some(picker) = outer.as_datetime_picker() {
                picker.process_sel_changed(date.year, date.month, day);
                if !outer.base().is_destroyed() {
                    picker.hide_month_calendar();
                }
            }
        }
    }
    fn on_mouse_move(&self, point: CfxPointF) {
        let hovered = if self.dates_rect.get().contains_point(point) {
            self.get_day_at_point(point)
        } else {
            -1
        };
        let previous = self.hovered.replace(hovered);
        if previous == hovered {
            return;
        }
        let mut invalid = self.day_rect(previous);
        let next = self.day_rect(hovered);
        if invalid.is_empty() {
            invalid = next;
        } else if !next.is_empty() {
            invalid.union(next);
        }
        if !invalid.is_empty() {
            self.base.repaint_rect(invalid);
        }
    }
    fn on_mouse_leave(&self) {
        let previous = self.hovered.replace(-1);
        let rect = self.day_rect(previous);
        if !rect.is_empty() {
            self.base.repaint_rect(rect);
        }
    }
}
impl FwlWidget for CfwlMonthCalendar {
    fn base(&self) -> &CfwlWidget {
        &self.base
    }
    fn get_class_id(&self) -> FwlType {
        FwlType::MonthCalendar
    }
    fn get_autosized_widget_rect(&self) -> CfxRectF {
        let size = self.calc_size();
        let mut rect = CfxRectF::new(0., 0., size.width, size.height);
        self.base.inflate_widget_rect(&mut rect);
        rect
    }
    fn update(&self) {
        if self.base.is_locked() || self.base.is_destroyed() {
            return;
        }
        self.ensure_initialized();
        if self.cell_size.get().width <= 0. {
            self.calc_size();
        }
        self.reset_date_items();
        self.layout();
    }
    fn draw_widget(&self, graphics: &mut CfgasGeGraphics<'_, '_, '_>, matrix: CfxMatrix) {
        if self.base.is_destroyed() {
            return;
        }
        self.ensure_initialized();
        if self.base.has_border() {
            self.base.draw_border(graphics, Part::Border, matrix);
        }
        self.background(
            graphics,
            Part::Background,
            self.client_rect.get(),
            PartState::NORMAL,
            matrix,
        );
        self.background(
            graphics,
            Part::Header,
            self.head_rect.get(),
            PartState::NORMAL,
            matrix,
        );
        self.background(
            graphics,
            Part::LBtn,
            self.left_button_rect.get(),
            self.left_button_states.get(),
            matrix,
        );
        self.background(
            graphics,
            Part::RBtn,
            self.right_button_rect.get(),
            self.right_button_states.get(),
            matrix,
        );
        self.background(
            graphics,
            Part::HSeparator,
            self.separator_rect.get(),
            PartState::NORMAL,
            matrix,
        );
        self.draw_date_backgrounds(graphics, matrix);
        let today = self.today.get().unwrap();
        let displayed = self.displayed.get();
        if today.year == displayed.year && today.month == displayed.month {
            let rect = self.day_rect(today.day);
            if !rect.is_empty() {
                self.background(
                    graphics,
                    Part::DateInCircle,
                    rect,
                    PartState::NORMAL,
                    matrix,
                );
            }
        }
        let head = Self::get_head_text(displayed.year, displayed.month);
        self.head_size.set(self.base.calc_text_size(&head, false));
        self.calc_head_size();
        self.text(
            graphics,
            Part::Caption,
            self.head_text_rect.get(),
            head,
            PartState::NORMAL,
            FdeTextAlignment::Center,
            matrix,
        );
        let week = self.week_rect.get();
        let cell = self.cell_size.get();
        for (index, day) in WEEKDAYS.iter().enumerate() {
            self.text(
                graphics,
                Part::Week,
                CfxRectF::new(
                    week.left + index as f32 * (cell.width + HMARGIN * 2.),
                    week.top,
                    cell.width,
                    cell.height,
                ),
                wide(day),
                PartState::NORMAL,
                FdeTextAlignment::Center,
                matrix,
            );
        }
        self.draw_dates(graphics, matrix);
        self.text(
            graphics,
            Part::DatesOut,
            CfxRectF::default(),
            Vec::new(),
            PartState::NORMAL,
            FdeTextAlignment::Center,
            matrix,
        );
        let text = Self::get_today_text(today);
        self.today_size.set(self.base.calc_text_size(&text, false));
        self.calc_today_size();
        self.text(
            graphics,
            Part::Today,
            self.today_rect.get(),
            text,
            PartState::NORMAL,
            FdeTextAlignment::CenterLeft,
            matrix,
        );
    }
    fn on_process_message(&self, message: &mut FwlMessage) {
        if self.base.is_destroyed() {
            return;
        }
        match message.data {
            MessageData::SetFocus | MessageData::KillFocus { .. } => {
                if let Some(outer) = self.base.outer() {
                    dispatch_message_to(&outer, message);
                }
            }
            MessageData::Mouse { command, pos, .. } if self.base.is_enabled() => match command {
                MouseCommand::LeftButtonDown => self.on_lbutton_down(pos),
                MouseCommand::LeftButtonUp => self.on_lbutton_up(pos),
                MouseCommand::Move => self.on_mouse_move(pos),
                MouseCommand::Leave => self.on_mouse_leave(),
                _ => {}
            },
            _ => {}
        }
        if !self.base.is_destroyed() && message.destination.is_some() {
            self.base.on_process_message(message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        fxge::{
            agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
            dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
        },
        xfa::fwl::test_support::new_app,
    };
    use boa_engine::{
        context::{time::FixedClock, HostHooks},
        Source,
    };
    struct Offset(i32);
    impl HostHooks for Offset {
        fn local_timezone_offset_seconds(&self, _timestamp: i64) -> i32 {
            self.0
        }
    }
    fn context() -> Context {
        // Explicit fixture: 2024-02-28 23:30 UTC is February29 in the supplied
        // Boa host zone. Ordinary production contexts keep Boa's default zone.
        let time = dates::make_date(dates::make_day(2024, 1, 28), dates::make_time(23, 30, 0, 0));
        Context::builder()
            .clock(Rc::new(FixedClock::from_millis(time as u64)))
            .host_hooks(Rc::new(Offset(3600)))
            .build()
            .unwrap()
    }
    fn calendar() -> (Rc<CfwlApp>, Rc<CfwlMonthCalendar>) {
        let (app, _) = new_app();
        let calendar = CfwlMonthCalendar::new(&app, Properties::default(), None);
        calendar.set_today_from_context(&context());
        let rect = calendar.get_autosized_widget_rect();
        calendar.base.set_widget_rect(rect);
        calendar.update();
        (app, calendar)
    }
    fn middle(rect: CfxRectF) -> CfxPointF {
        CfxPointF::new(rect.left + rect.width / 2., rect.top + rect.height / 2.)
    }
    fn mouse(calendar: &Rc<CfwlMonthCalendar>, command: MouseCommand, point: CfxPointF) {
        let mut message = FwlMessage::mouse(Some(calendar.clone()), command, 0, point);
        calendar.on_process_message(&mut message);
    }
    fn render(calendar: &CfwlMonthCalendar) -> CfxDibitmap {
        let size = calendar.base.get_widget_rect();
        let mut bitmap = CfxDibitmap::create(
            size.width.ceil() as i32 + 2,
            size.height.ceil() as i32 + 2,
            FxdibFormat::Bgra,
            0,
        )
        .unwrap();
        bitmap.clear(0xffffffff);
        {
            let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
            let mut graphics = CfgasGeGraphics::new(&mut device);
            let clip = graphics.get_clip_rect();
            calendar.draw_widget(&mut graphics, CfxMatrix::default());
            assert_eq!(graphics.get_clip_rect(), clip);
        }
        bitmap
    }
    #[test]
    fn existing_calendar_helpers_cover_month_lengths_weekdays_and_centuries() {
        for (year, month, days, first) in [
            (1900, 2, 28, 4),
            (2000, 2, 29, 2),
            (2024, 2, 29, 4),
            (2024, 3, 31, 5),
            (2100, 2, 28, 1),
            (2023, 1, 31, 0),
        ] {
            assert_eq!(month_days(year, month), days);
            assert_eq!(weekday(year, month), first);
        }
        assert_eq!(month_days(2024, 0), 0);
        assert_eq!(month_days(2024, 13), 0);
    }
    #[test]
    fn today_uses_the_supplied_boa_clock_and_local_zone() {
        let (_app, calendar) = calendar();
        let mut context = context();
        let actual = context
            .eval(Source::from_bytes(
                "let d=new Date();[d.getFullYear(),d.getMonth()+1,d.getDate()].join(',')",
            ))
            .unwrap()
            .to_string(&mut context)
            .unwrap()
            .to_std_string_escaped();
        let today = calendar.today();
        assert_eq!(today, (2024, 2, 29));
        assert_eq!(actual, format!("{},{},{}", today.0, today.1, today.2));
        assert_eq!(calendar.displayed_month(), (2024, 2));
        assert_eq!(calendar.date_count(), 29);
    }
    #[test]
    fn first_selection_month_navigation_today_and_bounds() {
        let (_app, calendar) = calendar();
        assert_eq!(calendar.selection(), None);
        calendar.set_select(2024, 2, 29);
        assert_eq!(calendar.selection(), Some((2024, 2, 29)));
        calendar.set_select(2023, 2, 29);
        assert_eq!(calendar.selection(), Some((2024, 2, 29)));
        calendar.set_select(2024, 13, 1);
        assert_eq!(calendar.displayed_month(), (2024, 2));
        calendar.next_month();
        assert_eq!(calendar.displayed_month(), (2024, 3));
        assert_eq!(calendar.date_count(), 31);
        assert_eq!(calendar.selection(), None);
        calendar.prev_month();
        assert_eq!(calendar.displayed_month(), (2024, 2));
        calendar.jump_to_today();
        assert_eq!(calendar.selection(), Some((2024, 2, 29)));
        calendar.set_select(1500, 12, 1);
        calendar.prev_month();
        assert_eq!(calendar.displayed_month(), (1500, 12));
        calendar.set_select(2200, 1, 1);
        calendar.next_month();
        assert_eq!(calendar.displayed_month(), (2200, 1));
    }
    #[test]
    fn day_grid_hit_testing_mouse_selection_and_disabled_input() {
        let (_app, calendar) = calendar();
        let first = calendar.day_rect(1);
        let saturday = calendar.day_rect(3);
        let sunday = calendar.day_rect(4);
        assert!(first.width > 0. && first.height > 0.);
        assert_eq!(saturday.top, first.top);
        assert!(sunday.top > first.top);
        assert!(sunday.left < first.left);
        assert_eq!(calendar.get_day_at_point(middle(first)), 1);
        assert_eq!(calendar.get_day_at_point(CfxPointF::new(-1., -1.)), -1);
        mouse(&calendar, MouseCommand::Move, middle(first));
        assert_eq!(calendar.hovered_day(), 1);
        mouse(&calendar, MouseCommand::Leave, middle(first));
        assert_eq!(calendar.hovered_day(), -1);
        mouse(&calendar, MouseCommand::LeftButtonUp, middle(first));
        assert_eq!(calendar.selection(), Some((2024, 2, 1)));
        calendar
            .base
            .set_states(super::super::cfwl_widget::WidgetState::DISABLED);
        mouse(&calendar, MouseCommand::LeftButtonUp, middle(saturday));
        assert_eq!(calendar.selection(), Some((2024, 2, 1)));
    }
    #[test]
    fn full_calendar_renders_caption_days_selection_and_today() {
        let (_app, calendar) = calendar();
        let first = render(&calendar);
        assert!(
            first
                .buffer()
                .chunks_exact(4)
                .filter(|pixel| *pixel != [255, 255, 255, 255])
                .count()
                > 100
        );
        calendar.set_select(2024, 2, 10);
        let selected = render(&calendar);
        assert_ne!(first.buffer(), selected.buffer());
        let selected_color =
            super::super::theme::cfwl_monthcalendartp::DATES_SELECTED_BACKGROUND_COLOR
                .to_le_bytes();
        assert!(selected
            .buffer()
            .chunks_exact(4)
            .any(|pixel| pixel == selected_color));
        assert_eq!(render(&calendar).buffer(), selected.buffer());
    }
    #[test]
    fn repaint_callback_can_change_the_month_during_mouse_navigation() {
        let (app, host) = new_app();
        let calendar = CfwlMonthCalendar::new(&app, Properties::default(), None);
        calendar.set_today_from_context(&context());
        let rect = calendar.get_autosized_widget_rect();
        calendar.base.set_widget_rect(rect);
        calendar.update();
        let weak = Rc::downgrade(&calendar);
        *host.repaint_hook.borrow_mut() = Some(Rc::new(move |_| {
            if let Some(calendar) = weak.upgrade() {
                calendar.set_select(2025, 1, 15);
            }
        }));
        mouse(
            &calendar,
            MouseCommand::LeftButtonDown,
            middle(calendar.right_button_rect()),
        );
        assert_eq!(calendar.selection(), Some((2025, 1, 15)));
        assert_eq!(calendar.date_count(), 31);
    }
}
