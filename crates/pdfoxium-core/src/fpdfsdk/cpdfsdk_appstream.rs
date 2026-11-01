// Copyright 2017 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
// Original code copyright 2014 Foxit Software Inc. http://www.foxitsoftware.com
//! Ordinary widget appearances from `fpdfsdk/cpdfsdk_appstream.{h,cpp}`.
//! The temporary, borderless CPWL window used by GenerateIconAppStream is
//! represented by its actual client rectangle; no host window is required.

use super::cpdfsdk_baannot::BorderStyle;
use super::cpdfsdk_widget::CpdfSdkWidget;
use super::pwl::cpwl_edit_impl::{CpwlEditImpl, EditFontMapProvider};
use crate::fpdfapi::parser::cpdf_dictionary::CpdfDictionary;
use crate::fpdfapi::parser::cpdf_object::{retain_object, ObjectRef};
use crate::fpdfapi::parser::cpdf_reference::CpdfReference;
use crate::fpdfapi::parser::cpdf_scalar_object::{CpdfScalarObject, ObjectType};
use crate::fpdfapi::parser::cpdf_stream::CpdfStream;
use crate::fpdfapi::parser::cpdf_string::CpdfString;
use crate::fpdfdoc::cpdf_apsettings::TextPosition;
use crate::fpdfdoc::cpdf_bafontmap::CpdfBaFontMap;
use crate::fpdfdoc::cpdf_formcontrol::HighlightingMode;
use crate::fpdfdoc::cpdf_formfield::{dict_get, dict_mut, typed};
use crate::fpdfdoc::cpdf_icon::CpdfIcon;
use crate::fpdfdoc::cpdf_iconfit::CpdfIconFit;
use crate::fpdfdoc::cpvt_wordplace::CpvtWordPlace;
use crate::fpdfdoc::ipvt_fontmap::IpvtFontMap;
use crate::fxcrt::fx_coordinates::{CfxFloatRect, CfxMatrix, CfxPointF};
use crate::fxcrt::stream_write_utils::{write_matrix, write_point, write_rect};
use crate::fxge::cfx_color::{CfxColor, ColorType};
use std::fmt::Write;
use std::rc::Rc;

const BEZIER: f32 = 0.5522847498308;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CheckStyle {
    Check,
    Circle,
    Cross,
    Diamond,
    Square,
    Star,
}
#[derive(Clone, Copy)]
struct Dash {
    dash: i32,
    gap: i32,
    phase: i32,
}
impl Default for Dash {
    fn default() -> Self {
        Self {
            dash: 3,
            gap: 0,
            phase: 0,
        }
    }
}
fn point(s: &mut String, p: CfxPointF, op: &str) {
    let _ = write_point(s, p);
    let _ = writeln!(s, " {op}");
}
fn rect(s: &mut String, r: CfxFloatRect) {
    let _ = write_rect(s, r);
    s.push_str(" re\n");
}
fn matrix(s: &mut String, m: CfxMatrix) {
    let _ = write_matrix(s, m);
    s.push_str(" cm\n");
}
fn curve(s: &mut String, a: CfxPointF, b: CfxPointF, c: CfxPointF) {
    let _ = write_point(s, a);
    s.push(' ');
    let _ = write_point(s, b);
    s.push(' ');
    point(s, c, "c");
}
fn closed_loop(s: &mut String, ps: &[CfxPointF]) {
    if let Some(&first) = ps.first() {
        point(s, first, "m");
        for &p in &ps[1..] {
            point(s, p, "l");
        }
        point(s, first, "l");
    }
}
fn color(c: CfxColor, stroke: bool) -> String {
    match c.color_type {
        ColorType::Transparent => String::new(),
        ColorType::Gray => format!("{} {}\n", c.color1, if stroke { "G" } else { "g" }),
        ColorType::Rgb => format!(
            "{} {} {} {}\n",
            c.color1,
            c.color2,
            c.color3,
            if stroke { "RG" } else { "rg" }
        ),
        ColorType::Cmyk => format!(
            "{} {} {} {} {}\n",
            c.color1,
            c.color2,
            c.color3,
            c.color4,
            if stroke { "K" } else { "k" }
        ),
    }
}
fn q(contents: String) -> String {
    format!("q\n{contents}Q\n")
}
fn rect_fill(r: CfxFloatRect, c: CfxColor) -> String {
    let mut s = color(c, false);
    if s.is_empty() {
        return s;
    }
    rect(&mut s, r);
    s.push_str("f\n");
    q(s)
}
fn circle_path(r: CfxFloatRect) -> String {
    let mut s = String::new();
    let x = (r.left + r.right) / 2.;
    let y = (r.bottom + r.top) / 2.;
    let dx = r.width() / 2. * BEZIER;
    let dy = r.height() / 2. * BEZIER;
    point(&mut s, CfxPointF::new(r.left, y), "m");
    curve(
        &mut s,
        CfxPointF::new(r.left, y + dy),
        CfxPointF::new(x - dx, r.top),
        CfxPointF::new(x, r.top),
    );
    curve(
        &mut s,
        CfxPointF::new(x + dx, r.top),
        CfxPointF::new(r.right, y + dy),
        CfxPointF::new(r.right, y),
    );
    curve(
        &mut s,
        CfxPointF::new(r.right, y - dy),
        CfxPointF::new(x + dx, r.bottom),
        CfxPointF::new(x, r.bottom),
    );
    curve(
        &mut s,
        CfxPointF::new(x - dx, r.bottom),
        CfxPointF::new(r.left, y - dy),
        CfxPointF::new(r.left, y),
    );
    s
}
fn half_circle(r: CfxFloatRect, angle: f32) -> String {
    let mut s = String::new();
    let w = r.width();
    let h = r.height();
    matrix(
        &mut s,
        CfxMatrix::new(
            angle.cos(),
            angle.sin(),
            -angle.sin(),
            angle.cos(),
            r.left + w / 2.,
            r.bottom + h / 2.,
        ),
    );
    point(&mut s, CfxPointF::new(-w / 2., 0.), "m");
    curve(
        &mut s,
        CfxPointF::new(-w / 2., h / 2. * BEZIER),
        CfxPointF::new(-w / 2. * BEZIER, h / 2.),
        CfxPointF::new(0., h / 2.),
    );
    curve(
        &mut s,
        CfxPointF::new(w / 2. * BEZIER, h / 2.),
        CfxPointF::new(w / 2., h / 2. * BEZIER),
        CfxPointF::new(w / 2., 0.),
    );
    s
}
fn check_path(r: CfxFloatRect) -> String {
    let mut table = [
        [[0.28, 0.52], [0.27, 0.48], [0.29, 0.40]],
        [[0.30, 0.33], [0.31, 0.29], [0.31, 0.28]],
        [[0.39, 0.28], [0.49, 0.29], [0.77, 0.67]],
        [[0.76, 0.68], [0.78, 0.69], [0.76, 0.75]],
        [[0.76, 0.75], [0.73, 0.80], [0.68, 0.75]],
        [[0.68, 0.74], [0.68, 0.74], [0.44, 0.47]],
        [[0.43, 0.47], [0.40, 0.47], [0.41, 0.58]],
        [[0.40, 0.60], [0.28, 0.66], [0.30, 0.56]],
    ];
    for row in &mut table {
        for p in row {
            p[0] = p[0] * r.width() + r.left;
            p[1] = p[1] * r.height() + r.bottom;
        }
    }
    let mut s = String::new();
    point(&mut s, CfxPointF::new(table[0][0][0], table[0][0][1]), "m");
    for i in 0..table.len() {
        let a = table[i][0];
        let b = table[i][1];
        let c = table[i][2];
        let n = table[(i + 1) % table.len()][0];
        curve(
            &mut s,
            CfxPointF::new(a[0] + (b[0] - a[0]) * BEZIER, a[1] + (b[1] - a[1]) * BEZIER),
            CfxPointF::new(n[0] + (c[0] - n[0]) * BEZIER, n[1] + (c[1] - n[1]) * BEZIER),
            CfxPointF::new(n[0], n[1]),
        );
    }
    s
}
fn caption_style(c: &[u32]) -> Option<CheckStyle> {
    Some(match c.first().copied()? {
        52 => CheckStyle::Check,
        56 => CheckStyle::Cross,
        72 => CheckStyle::Star,
        108 => CheckStyle::Circle,
        110 => CheckStyle::Square,
        117 => CheckStyle::Diamond,
        _ => return None,
    })
}
fn check_mark(r: CfxFloatRect, style: CheckStyle, c: CfxColor, radio: bool) -> String {
    let mut r = r.get_center_square();
    if style == CheckStyle::Circle {
        r.scale_from_center_point(if radio { 0.5 } else { 2. / 3. });
    } else if matches!(
        style,
        CheckStyle::Diamond | CheckStyle::Square | CheckStyle::Star
    ) {
        r.scale_from_center_point(2. / 3.);
    }
    let mut s = color(c, style == CheckStyle::Cross);
    match style {
        CheckStyle::Check => s.push_str(&check_path(r)),
        CheckStyle::Circle => s.push_str(&circle_path(r)),
        CheckStyle::Cross => {
            point(&mut s, CfxPointF::new(r.left, r.top), "m");
            point(&mut s, CfxPointF::new(r.right, r.bottom), "l");
            point(&mut s, CfxPointF::new(r.left, r.bottom), "m");
            point(&mut s, CfxPointF::new(r.right, r.top), "l");
        }
        CheckStyle::Diamond => {
            s.push_str("1 w\n");
            closed_loop(
                &mut s,
                &[
                    CfxPointF::new(r.left, (r.bottom + r.top) / 2.),
                    CfxPointF::new((r.left + r.right) / 2., r.top),
                    CfxPointF::new(r.right, (r.bottom + r.top) / 2.),
                    CfxPointF::new((r.left + r.right) / 2., r.bottom),
                ],
            );
        }
        CheckStyle::Square => closed_loop(
            &mut s,
            &[
                CfxPointF::new(r.left, r.top),
                CfxPointF::new(r.right, r.top),
                CfxPointF::new(r.right, r.bottom),
                CfxPointF::new(r.left, r.bottom),
            ],
        ),
        CheckStyle::Star => {
            let radius = r.height() / (1. + (std::f32::consts::PI / 5.).cos());
            let center = CfxPointF::new((r.left + r.right) / 2., (r.top + r.bottom) / 2.);
            let ps: Vec<_> = (0..5)
                .map(|i| {
                    let a = std::f32::consts::PI / 10. + i as f32 * std::f32::consts::PI * 2. / 5.;
                    CfxPointF::new(center.x + radius * a.cos(), center.y + radius * a.sin())
                })
                .collect();
            closed_loop(&mut s, &[ps[0], ps[2], ps[4], ps[1], ps[3]]);
        }
    }
    s.push_str(if style == CheckStyle::Cross {
        "S\n"
    } else {
        "f\n"
    });
    q(s)
}
fn border(
    r: CfxFloatRect,
    w: f32,
    c: CfxColor,
    lt: CfxColor,
    rb: CfxColor,
    style: BorderStyle,
    dash: Dash,
) -> String {
    if w <= 0. {
        return String::new();
    }
    let mut s = String::new();
    let hw = w / 2.;
    let (l, b, rr, t) = (r.left, r.bottom, r.right, r.top);
    match style {
        BorderStyle::Solid => {
            let col = color(c, false);
            if !col.is_empty() {
                s.push_str(&col);
                rect(&mut s, r);
                rect(&mut s, r.get_deflated(w, w));
                s.push_str("f*\n");
            }
        }
        BorderStyle::Dash => {
            let col = color(c, true);
            if !col.is_empty() {
                s.push_str(&col);
                let _ = writeln!(s, "{w} w [{} {}] {} d", dash.dash, dash.gap, dash.phase);
                closed_loop(
                    &mut s,
                    &[
                        CfxPointF::new(l + hw, b + hw),
                        CfxPointF::new(l + hw, t - hw),
                        CfxPointF::new(rr - hw, t - hw),
                        CfxPointF::new(rr - hw, b + hw),
                    ],
                );
                s.push_str("S\n");
            }
        }
        BorderStyle::Beveled | BorderStyle::Inset => {
            let col = color(lt, false);
            if !col.is_empty() {
                s.push_str(&col);
                let ps = [
                    CfxPointF::new(l + hw, b + hw),
                    CfxPointF::new(l + hw, t - hw),
                    CfxPointF::new(rr - hw, t - hw),
                    CfxPointF::new(rr - w, t - w),
                    CfxPointF::new(l + w, t - w),
                    CfxPointF::new(l + w, b + w),
                ];
                point(&mut s, ps[0], "m");
                for &p in &ps[1..] {
                    point(&mut s, p, "l");
                }
                s.push_str("f\n");
            }
            let col = color(rb, false);
            if !col.is_empty() {
                s.push_str(&col);
                let ps = [
                    CfxPointF::new(rr - hw, t - hw),
                    CfxPointF::new(rr - hw, b + hw),
                    CfxPointF::new(l + hw, b + hw),
                    CfxPointF::new(l + w, b + w),
                    CfxPointF::new(rr - w, b + w),
                    CfxPointF::new(rr - w, t - w),
                ];
                point(&mut s, ps[0], "m");
                for &p in &ps[1..] {
                    point(&mut s, p, "l");
                }
                s.push_str("f\n");
            }
            let col = color(c, false);
            if !col.is_empty() {
                s.push_str(&col);
                rect(&mut s, r);
                rect(&mut s, r.get_deflated(hw, hw));
                s.push_str("f*\n");
            }
        }
        BorderStyle::Underline => {
            let col = color(c, true);
            if !col.is_empty() {
                s.push_str(&col);
                let _ = writeln!(s, "{w} w");
                point(&mut s, CfxPointF::new(l, b + hw), "m");
                point(&mut s, CfxPointF::new(rr, b + hw), "l");
                s.push_str("S\n");
            }
        }
    }
    q(s)
}
fn circle_fill(r: CfxFloatRect, c: CfxColor) -> String {
    let mut s = color(c, false);
    if s.is_empty() {
        return s;
    }
    s.push_str(&circle_path(r));
    s.push_str("f\n");
    q(s)
}
fn circle_border(
    r: CfxFloatRect,
    w: f32,
    c: CfxColor,
    lt: CfxColor,
    rb: CfxColor,
    style: BorderStyle,
    dash: Dash,
) -> String {
    if w <= 0. {
        return String::new();
    }
    let mut s = String::new();
    let hw = w / 2.;
    let r2 = r.get_deflated(hw, hw);
    let r75 = r.get_deflated(hw * 0.75, hw * 0.75);
    if matches!(
        style,
        BorderStyle::Solid | BorderStyle::Underline | BorderStyle::Dash
    ) {
        let col = color(c, true);
        if !col.is_empty() {
            let mut a = format!("{w} w\n");
            if style == BorderStyle::Dash {
                let _ = writeln!(a, "[{} {}] {} d", dash.dash, dash.gap, dash.phase);
            }
            a.push_str(&col);
            a.push_str(&circle_path(r2));
            a.push_str(" S\n");
            s.push_str(&q(a));
        }
    } else {
        for (col, path) in [
            (c, circle_path(r)),
            (lt, half_circle(r75, std::f32::consts::PI / 4.)),
            (rb, half_circle(r75, std::f32::consts::PI * 5. / 4.)),
        ] {
            let col = color(col, true);
            if !col.is_empty() {
                s.push_str(&q(format!("{hw} w\n{col}{path} S\n")));
            }
        }
    }
    q(s)
}
fn drop_button(r: CfxFloatRect) -> String {
    if r.is_empty() {
        return String::new();
    }
    let mut s = rect_fill(r, CfxColor::make_rgb_bytes(220, 220, 220));
    s.push_str(&q(border(
        r,
        2.,
        CfxColor::make_gray(0.),
        CfxColor::make_gray(1.),
        CfxColor::make_gray(0.5),
        BorderStyle::Beveled,
        Dash::default(),
    )));
    if r.width() > 6. && r.height() > 6. {
        let x = (r.left + r.right) / 2.;
        let y = (r.bottom + r.top) / 2.;
        let mut a = " 0 g\n".to_string();
        closed_loop(
            &mut a,
            &[
                CfxPointF::new(x - 3., y + 1.5),
                CfxPointF::new(x + 3., y + 1.5),
                CfxPointF::new(x, y - 1.5),
            ],
        );
        a.push_str("f\n");
        s.push_str(&q(a));
    }
    s
}
fn font_set(map: &dyn IpvtFontMap, index: i32, size: f32) -> String {
    let alias = map.get_pdf_font_alias(index);
    if alias.is_empty() || size <= 0. {
        return String::new();
    }
    format!("/{} {size} Tf\n", String::from_utf8_lossy(&alias))
}
fn word_render(words: &[u8]) -> String {
    if words.is_empty() {
        return String::new();
    }
    {
        let mut s = String::from("<");
        for b in words {
            let _ = write!(s, "{b:02x}");
        }
        s.push_str("> Tj\n");
        s
    }
}
fn edit_stream(
    edit: &CpwlEditImpl<'_>,
    map: &dyn IpvtFontMap,
    offset: CfxPointF,
    continuous: bool,
    sub: u16,
) -> String {
    let mut iter = edit.iterator();
    iter.set_at_index(0);
    let mut s = String::new();
    let mut current_font = -1;
    let mut old = CfxPointF::default();
    let mut old_place = CpvtWordPlace::default();
    let mut words = Vec::new();
    while iter.next_word() {
        let place = iter.word_place();
        let word = iter.word();
        let rtl = word.as_ref().is_some_and(|w| w.is_rtl);
        if continuous && !rtl {
            if place.line_cmp(old_place) != 0 || (words.is_empty() && word.is_some()) {
                if place.line_cmp(old_place) != 0 && !words.is_empty() {
                    s.push_str(&word_render(&words));
                    words.clear();
                }
                let new = word
                    .map(|w| w.location + offset)
                    .or_else(|| iter.line().map(|l| l.point + offset))
                    .unwrap_or(old);
                if new != old {
                    point(&mut s, new - old, "Td");
                    old = new;
                }
            }
            if let Some(word) = word {
                if word.font_index != current_font {
                    if !words.is_empty() {
                        s.push_str(&word_render(&words));
                        words.clear();
                    }
                    s.push_str(&font_set(map, word.font_index, word.font_size));
                    current_font = word.font_index;
                }
                words.extend(edit.get_pdf_word_string(current_font, word.word, sub));
            }
        } else {
            if !words.is_empty() {
                s.push_str(&word_render(&words));
                words.clear();
            }
            if let Some(word) = word {
                let new = word.location + offset;
                if new != old {
                    point(&mut s, new - old, "Td");
                    old = new;
                }
                if word.font_index != current_font {
                    s.push_str(&font_set(map, word.font_index, word.font_size));
                    current_font = word.font_index;
                }
                s.push_str(&word_render(&edit.get_pdf_word_string(
                    current_font,
                    word.word,
                    sub,
                )));
            }
        }
        old_place = place;
    }
    if !words.is_empty() {
        s.push_str(&word_render(&words));
    }
    s
}
fn icon_stream(fit: &CpdfIconFit, icon: Option<&ObjectRef>, r: CfxFloatRect) -> String {
    let Some(icon) = icon else {
        return String::new();
    };
    if r.is_empty() {
        return String::new();
    }
    let icon = CpdfIcon::new(Rc::clone(icon));
    let alias = icon.image_alias();
    if alias.is_empty() {
        return String::new();
    }
    let size = icon.image_size();
    let scale = fit.scale(size, r);
    let offset = fit.image_offset(size, scale, r);
    let mut s = String::new();
    rect(&mut s, r);
    s.push_str("W n\n");
    matrix(
        &mut s,
        CfxMatrix::new(
            scale.x,
            0.,
            0.,
            scale.y,
            r.left + offset.x,
            r.bottom + offset.y,
        ),
    );
    matrix(&mut s, icon.image_matrix().get_inverse());
    let _ = writeln!(s, "0 g 0 G 1 w /{} Do", String::from_utf8_lossy(&alias));
    q(s)
}
fn push_button(
    r: CfxFloatRect,
    map: &dyn IpvtFontMap,
    icon: Option<&ObjectRef>,
    fit: &CpdfIconFit,
    label: &[u32],
    c: CfxColor,
    size: f32,
    layout: TextPosition,
) -> String {
    let provider = EditFontMapProvider { font_map: map };
    let mut edit = CpwlEditImpl::new(Some(&provider));
    edit.set_alignment_h(1);
    edit.set_alignment_v(1);
    edit.set_multi_line(false);
    edit.set_auto_return(false);
    if size == 0. {
        edit.set_auto_font_size(true);
    } else {
        edit.set_font_size(size);
    }
    edit.initialize();
    edit.set_text(label);
    edit.paint();
    let content = edit.content_rect();
    let mut lr = CfxFloatRect::default();
    let mut ir = CfxFloatRect::default();
    match layout {
        TextPosition::Caption => lr = r,
        TextPosition::Icon => ir = r,
        TextPosition::Overlaid => {
            lr = r;
            ir = r;
        }
        TextPosition::Below | TextPosition::Above => {
            if icon.is_none() {
                lr = r;
            } else {
                let h = if size == 0. {
                    r.height() / 3.
                } else {
                    content.height()
                };
                if r.bottom + h > r.top {
                    lr = r;
                } else if layout == TextPosition::Below {
                    lr = CfxFloatRect::new(r.left, r.bottom, r.right, r.bottom + h);
                    ir = CfxFloatRect::new(r.left, lr.top, r.right, r.top);
                } else {
                    lr = CfxFloatRect::new(r.left, r.top - h, r.right, r.top);
                    ir = CfxFloatRect::new(r.left, r.bottom, r.right, lr.bottom);
                }
            }
        }
        TextPosition::Right | TextPosition::Left => {
            if icon.is_none() {
                lr = r;
            } else {
                let w = if size == 0. {
                    content.width().max(r.width() / 3.)
                } else {
                    content.width()
                };
                if r.left + w > r.right {
                    lr = r;
                } else if layout == TextPosition::Right {
                    lr = CfxFloatRect::new(r.right - w, r.bottom, r.right, r.top);
                    ir = CfxFloatRect::new(r.left, r.bottom, lr.left, r.top);
                } else {
                    lr = CfxFloatRect::new(r.left, r.bottom, r.left + w, r.top);
                    ir = CfxFloatRect::new(lr.right, r.bottom, r.right, r.top);
                }
            }
        }
    }
    let mut temp = icon_stream(fit, icon, ir);
    if !lr.is_empty() {
        edit.set_plate_rect(lr);
        edit.paint();
        let text = edit_stream(&edit, map, CfxPointF::default(), true, 0);
        if !text.is_empty() {
            temp.push_str(&format!("BT\n{}{text}ET\n", color(c, false)));
        }
    }
    if temp.is_empty() {
        return temp;
    }
    let mut s = String::new();
    rect(&mut s, r);
    s.push_str("W n\n");
    s.push_str(&temp);
    q(s)
}

pub struct CpdfSdkAppStream<'a> {
    widget: &'a CpdfSdkWidget,
}
impl<'a> CpdfSdkAppStream<'a> {
    pub fn new(widget: &'a CpdfSdkWidget) -> Self {
        Self { widget }
    }
    fn ap_dict(&self) -> ObjectRef {
        dict_mut(&self.widget.dict(), |d| d.get_or_create_dict_for(b"AP"))
    }
    fn remove(&self, kind: &[u8]) {
        dict_mut(&self.ap_dict(), |d| {
            d.remove_for(kind);
        });
    }
    fn write(&self, kind: &[u8], contents: String, state: &[u8]) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let holder = doc.borrow().holder();
        let ap = self.ap_dict();
        let parent = if state.is_empty() {
            ap
        } else {
            dict_mut(&ap, |d| d.get_or_create_dict_for(kind))
        };
        let key = if state.is_empty() { kind } else { state };
        let old = typed(dict_get(&parent, key), ObjectType::Stream);
        let resources = old.and_then(|o| {
            let d = o.borrow().as_any().downcast_ref::<CpdfStream>()?.get_dict();
            typed(dict_get(&d, b"Resources"), ObjectType::Dictionary)
        });
        let mut d = CpdfDictionary::new();
        d.set_new_name_for(b"Type", b"XObject");
        d.set_new_name_for(b"Subtype", b"Form");
        d.set_new_for(b"FormType", CpdfScalarObject::number_from_signed(1));
        // Each write owns a fresh stream, so an appearance shared by another widget
        // is never mutated. Copy resource containers, retaining their indirect assets.
        if let Some(resources) = resources {
            d.set_for(b"Resources", Some(copy_resources(&resources)));
        }
        d.set_matrix_for(b"Matrix", self.widget.matrix());
        d.set_rect_for(b"BBox", self.widget.rotated_rect());
        let stream = retain_object(CpdfStream::from_data_and_dict(
            contents.into_bytes(),
            retain_object(d),
        ));
        let n = holder.borrow_mut().add_indirect_object(stream);
        dict_mut(&parent, |d| {
            d.set_new_for(key, CpdfReference::new(&holder, n));
        });
        true
    }
    fn add_image(&self, kind: &[u8], image: &ObjectRef) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let holder = doc.borrow().holder();
        let Some(stream) = typed(dict_get(&self.ap_dict(), kind), ObjectType::Stream) else {
            return false;
        };
        let sd = stream
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let image_dict = image
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let alias =
            dict_get(&image_dict, b"Name").map_or_else(Vec::new, |o| o.borrow().get_string());
        let n = if image.borrow().obj_num() == 0 {
            holder.borrow_mut().add_indirect_object(Rc::clone(image))
        } else {
            image.borrow().obj_num()
        };
        let res = dict_mut(&sd, |d| d.get_or_create_dict_for(b"Resources"));
        let mut x = CpdfDictionary::new();
        x.set_new_for(&alias, CpdfReference::new(&holder, n));
        dict_mut(&res, |d| {
            d.set_new_for(b"XObject", x);
        });
        true
    }
    fn border_params(&self, bg: CfxColor) -> (f32, CfxColor, CfxColor, Dash) {
        let mut w = self.widget.border_width() as f32;
        let mut lt = CfxColor::default();
        let mut rb = CfxColor::default();
        let mut dash = Dash::default();
        match self.widget.border_style() {
            BorderStyle::Dash => dash.gap = 3,
            BorderStyle::Beveled => {
                w *= 2.;
                lt = CfxColor::make_gray(1.);
                rb = bg / 2.;
            }
            BorderStyle::Inset => {
                w *= 2.;
                lt = CfxColor::make_gray(0.5);
                rb = CfxColor::make_gray(0.75);
            }
            _ => (),
        }
        (w, lt, rb, dash)
    }
    fn background(&self) -> String {
        rect_fill(self.widget.rotated_rect(), self.widget.fill_pwl_color())
    }
    fn border(&self) -> String {
        let bg = self.widget.fill_pwl_color();
        let (w, lt, rb, dash) = self.border_params(bg);
        border(
            self.widget.rotated_rect(),
            w,
            self.widget.border_pwl_color(),
            lt,
            rb,
            self.widget.border_style(),
            dash,
        )
    }
    pub fn set_as_push_button(&self) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let c = self.widget.control();
        let r = self.widget.rotated_rect();
        let bg = c.original_background_color();
        let bc = c.original_border_color();
        let (w, mut lt, mut rb, dash) = self.border_params(bg);
        let client = r.get_deflated(w, w);
        let da = c.default_appearance();
        let text = da.color().unwrap_or(CfxColor::make_gray(0.));
        let size = da.font().map_or(12., |f| f.size);
        let layout = c.text_position();
        let normal = if c.has_mk_entry(b"CA") {
            c.normal_caption()
        } else {
            Vec::new()
        };
        let mut rollover = if c.has_mk_entry(b"RC") {
            c.rollover_caption()
        } else {
            Vec::new()
        };
        let mut down = if c.has_mk_entry(b"AC") {
            c.down_caption()
        } else {
            Vec::new()
        };
        let ni = if c.has_mk_entry(b"I") {
            c.normal_icon()
        } else {
            None
        };
        let mut ri = if c.has_mk_entry(b"RI") {
            c.rollover_icon()
        } else {
            None
        };
        let mut di = if c.has_mk_entry(b"IX") {
            c.down_icon()
        } else {
            None
        };
        for (icon, name) in [
            (&ni, b"ImgA".as_slice()),
            (&ri, b"ImgB".as_slice()),
            (&di, b"ImgC".as_slice()),
        ] {
            if let Some(icon) = icon {
                let d = icon
                    .borrow()
                    .as_any()
                    .downcast_ref::<CpdfStream>()
                    .unwrap()
                    .get_dict();
                dict_mut(&d, |d| {
                    if !d.key_exist(b"Name") {
                        d.set_new_for(b"Name", CpdfString::from_bytes(None, name));
                    }
                });
            }
        }
        let fit = c.icon_fit();
        let bbox = if fit.fitting_bounds() { r } else { client };
        let style = self.widget.border_style();
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"N");
        let content = format!(
            "{}{}{}",
            rect_fill(r, bg),
            border(r, w, bc, lt, rb, style, dash),
            push_button(bbox, &map, ni.as_ref(), &fit, &normal, text, size, layout)
        );
        if !self.write(b"N", content, b"") {
            return false;
        }
        if let Some(icon) = &ni {
            if !self.add_image(b"N", icon) {
                return false;
            }
        }
        if !matches!(
            c.highlighting_mode(),
            HighlightingMode::Push | HighlightingMode::Toggle
        ) {
            self.remove(b"D");
            self.remove(b"R");
            return true;
        }
        if rollover.is_empty() && ri.is_none() {
            rollover = normal.clone();
            ri = ni.clone();
        }
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"R");
        let content = format!(
            "{}{}{}",
            rect_fill(r, bg),
            border(r, w, bc, lt, rb, style, dash),
            push_button(bbox, &map, ri.as_ref(), &fit, &rollover, text, size, layout)
        );
        if !self.write(b"R", content, b"") {
            return false;
        }
        if let Some(icon) = &ri {
            if !self.add_image(b"R", icon) {
                return false;
            }
        }
        if down.is_empty() && di.is_none() {
            down = normal;
            di = ni;
        }
        match style {
            BorderStyle::Beveled => std::mem::swap(&mut lt, &mut rb),
            BorderStyle::Inset => {
                lt = CfxColor::make_gray(0.);
                rb = CfxColor::make_gray(1.);
            }
            _ => (),
        }
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"D");
        let content = format!(
            "{}{}{}",
            rect_fill(r, bg - 0.25),
            border(r, w, bc, lt, rb, style, dash),
            push_button(bbox, &map, di.as_ref(), &fit, &down, text, size, layout)
        );
        if !self.write(b"D", content, b"") {
            return false;
        }
        di.as_ref().is_none_or(|icon| self.add_image(b"D", icon))
    }
    pub fn set_as_check_box(&self) -> bool {
        self.set_as_check(false)
    }
    pub fn set_as_radio_button(&self) -> bool {
        self.set_as_check(true)
    }
    fn set_as_check(&self, radio: bool) -> bool {
        let c = self.widget.control();
        let bg = c.original_background_color();
        let bc = c.original_border_color();
        let (w, mut lt, mut rb, dash) = self.border_params(bg);
        let style = self.widget.border_style();
        let r = self.widget.rotated_rect();
        let client = r.get_deflated(w, w);
        let text = c.default_appearance().color().unwrap_or_default();
        let mark = caption_style(&c.normal_caption()).unwrap_or(if radio {
            CheckStyle::Circle
        } else {
            CheckStyle::Check
        });
        let center = r.get_center_square().get_deflated(1., 1.);
        let circular = radio && mark == CheckStyle::Circle;
        if circular {
            match style {
                BorderStyle::Beveled => {
                    lt = CfxColor::make_gray(1.);
                    rb = bg - 0.25;
                }
                BorderStyle::Inset => {
                    lt = CfxColor::make_gray(0.5);
                    rb = CfxColor::make_gray(0.75);
                }
                _ => (),
            }
        }
        let off = if circular {
            format!(
                "{}{}",
                circle_fill(center, bg),
                circle_border(center, w, bc, lt, rb, style, dash)
            )
        } else {
            format!(
                "{}{}",
                rect_fill(r, bg),
                border(r, w, bc, lt, rb, style, dash)
            )
        };
        match style {
            BorderStyle::Beveled => std::mem::swap(&mut lt, &mut rb),
            BorderStyle::Inset => {
                lt = CfxColor::make_gray(0.);
                rb = CfxColor::make_gray(1.);
            }
            _ => (),
        }
        let down_off = if circular {
            let mut bk = bg - 0.25;
            match style {
                BorderStyle::Beveled => {
                    lt = bg - 0.25;
                    rb = CfxColor::make_gray(1.);
                    bk = bg;
                }
                BorderStyle::Inset => {
                    lt = CfxColor::make_gray(0.);
                    rb = CfxColor::make_gray(1.);
                }
                _ => (),
            }
            format!(
                "{}{}",
                circle_fill(center, bk),
                circle_border(center, w, bc, lt, rb, style, dash)
            )
        } else {
            format!(
                "{}{}",
                rect_fill(r, bg - 0.25),
                border(r, w, bc, lt, rb, style, dash)
            )
        };
        let glyph = check_mark(client, mark, text, radio);
        let state = c.checked_ap_state();
        let ok = self.write(b"N", format!("{off}{glyph}"), &state)
            && self.write(b"N", off, b"Off")
            && self.write(b"D", format!("{down_off}{glyph}"), &state)
            && self.write(b"D", down_off, b"Off");
        if ok && self.widget.app_state().is_empty() {
            self.widget.set_app_state_off();
        }
        ok
    }
    pub fn set_as_combo_box(&self, value: Option<&[u32]>) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let c = self.widget.control();
        let Some(field) = c.field() else {
            return false;
        };
        let client = self.widget.client_rect();
        let mut button = client;
        button.left = button.right - 13.;
        button.normalize();
        let mut er = client;
        er.right = button.left;
        er.normalize();
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"N");
        let provider = EditFontMapProvider { font_map: &map };
        let mut edit = CpwlEditImpl::new(Some(&provider));
        edit.enable_refresh(false);
        edit.set_plate_rect(er);
        edit.set_alignment_v(1);
        let size = self.widget.font_size();
        if size == 0. {
            edit.set_auto_font_size(true);
        } else {
            edit.set_font_size(size);
        }
        edit.initialize();
        edit.set_text_direction(self.widget.text_direction());
        let v = value.map(<[u32]>::to_vec).unwrap_or_else(|| {
            let selected = field.selected_index(0);
            if selected < 0 {
                field.value()
            } else {
                field.option_label(selected)
            }
        });
        edit.set_text(&v);
        edit.paint();
        let text = edit_stream(&edit, &map, CfxPointF::default(), true, 0);
        let mut body = String::new();
        if !text.is_empty() {
            let mut s = String::new();
            let content = edit.content_rect();
            if content.width() > er.width() || content.height() > er.height() {
                rect(&mut s, er);
                s.push_str("W\nn\n");
            }
            s.push_str(&format!(
                "BT\n{}{text}ET\n",
                color(self.widget.text_pwl_color(), false)
            ));
            body.push_str(&format!("/Tx BMC\n{}EMC\n", q(s)));
        }
        body.push_str(&drop_button(button));
        self.write(
            b"N",
            format!("{}{}{body}", self.background(), self.border()),
            b"",
        )
    }
    pub fn set_as_list_box(&self) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let c = self.widget.control();
        let Some(field) = c.field() else {
            return false;
        };
        let client = self.widget.client_rect();
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"N");
        let provider = EditFontMapProvider { font_map: &map };
        let mut edit = CpwlEditImpl::new(Some(&provider));
        edit.enable_refresh(false);
        edit.set_plate_rect(CfxFloatRect::new(client.left, 0., client.right, 0.));
        let size = self.widget.font_size();
        edit.set_font_size(if size == 0. { 12. } else { size });
        edit.initialize();
        edit.set_text_direction(self.widget.text_direction());
        let mut list = String::new();
        let mut y = client.top;
        for i in field.top_visible_index().max(0)
            ..i32::try_from(field.count_options()).unwrap_or(i32::MAX)
        {
            let selected =
                (0..field.count_selected_items()).any(|j| field.selected_index(j as i32) == i);
            edit.set_text(&field.option_label(i));
            edit.paint();
            let h = edit.content_rect().height();
            let col = if selected {
                list.push_str(&rect_fill(
                    CfxFloatRect::new(client.left, y - h, client.right, y),
                    CfxColor::make_rgb_bytes(0, 51, 113),
                ));
                CfxColor::make_gray(1.)
            } else {
                self.widget.text_pwl_color()
            };
            list.push_str(&format!(
                "BT\n{}{}ET\n",
                color(col, false),
                edit_stream(&edit, &map, CfxPointF::new(0., y), true, 0)
            ));
            y -= h;
        }
        let body = if list.is_empty() {
            String::new()
        } else {
            let mut s = String::new();
            rect(&mut s, client);
            s.push_str("W\nn\n");
            s.push_str(&list);
            format!("/Tx BMC\n{}EMC\n", q(s))
        };
        self.write(
            b"N",
            format!("{}{}{body}", self.background(), self.border()),
            b"",
        )
    }
    pub fn set_as_text_field(&self, value: Option<&[u32]>) -> bool {
        let Some(doc) = self.widget.document() else {
            return false;
        };
        let c = self.widget.control();
        let Some(field) = c.field() else {
            return false;
        };
        let map = CpdfBaFontMap::new(&doc, self.widget.dict(), b"N");
        let provider = EditFontMapProvider { font_map: &map };
        let mut edit = CpwlEditImpl::new(Some(&provider));
        edit.enable_refresh(false);
        let client = self.widget.client_rect();
        edit.set_plate_rect(client);
        edit.set_alignment_h(c.control_alignment());
        let flags = field.field_flags();
        let multi = flags & (1 << 12) != 0;
        let comb = flags & (1 << 24) != 0;
        if multi {
            edit.set_multi_line(true);
            edit.set_auto_return(true);
        } else {
            edit.set_alignment_v(1);
        }
        let sub = if flags & (1 << 13) != 0 {
            edit.set_password_char(b'*' as u16);
            b'*' as u16
        } else {
            0
        };
        #[cfg(feature = "xfa")]
        let xfa_value = if value.is_none() && self.widget.get_mix_xfa_widget().is_some() {
            Some(self.widget.value())
        } else {
            None
        };
        #[cfg(feature = "xfa")]
        let value = value.or(xfa_value.as_deref());
        let mut max = field.max_len();
        let mut size = self.widget.font_size();
        if max > 0 {
            if comb {
                edit.set_char_array(max);
                if size == 0. {
                    size = super::pwl::cpwl_edit::CpwlEdit::char_array_auto_font_size(
                        map.get_pdf_font(0).as_ref(),
                        client,
                        max,
                    );
                }
            } else {
                if let Some(value) = value {
                    max = i32::try_from(value.len()).unwrap_or(i32::MAX);
                }
                edit.set_limit_char(max);
            }
        }
        if size == 0. {
            edit.set_auto_font_size(true);
        } else {
            edit.set_font_size(size);
        }
        edit.initialize();
        edit.set_text_direction(self.widget.text_direction());
        let v = value.map(<[u32]>::to_vec).unwrap_or_else(|| field.value());
        edit.set_text(&v);
        edit.paint();
        let text = edit_stream(&edit, &map, CfxPointF::default(), !comb, sub);
        let mut body = String::new();
        if !text.is_empty() {
            let mut s = String::new();
            let content = edit.content_rect();
            if content.width() > client.width() || content.height() > client.height() {
                rect(&mut s, client);
                s.push_str("W\nn\n");
            }
            s.push_str(&format!(
                "BT\n{}{text}ET\n",
                color(self.widget.text_pwl_color(), false)
            ));
            body = format!("/Tx BMC\n{}EMC\n", q(s));
        }
        let mut lines = String::new();
        if comb
            && max > 0
            && matches!(
                self.widget.border_style(),
                BorderStyle::Solid | BorderStyle::Dash
            )
        {
            let col = color(self.widget.border_pwl_color(), true);
            if !col.is_empty() {
                let _ = writeln!(lines, "{} w", self.widget.border_width());
                lines.push_str(&col);
                if self.widget.border_style() == BorderStyle::Solid {
                    lines.push_str(" 2 J 0 j\n");
                } else {
                    lines.push_str("[3 3] 0 d\n");
                }
                for i in 1..max {
                    let x = client.left + client.width() / max as f32 * i as f32;
                    point(&mut lines, CfxPointF::new(x, client.bottom), "m");
                    point(&mut lines, CfxPointF::new(x, client.top), "l");
                    lines.push_str("S\n");
                }
                lines = q(lines);
            }
        }
        self.write(
            b"N",
            format!("{}{}{lines}{body}", self.background(), self.border()),
            b"",
        )
    }
}
fn copy_dictionary(source: &ObjectRef) -> ObjectRef {
    let mut d = CpdfDictionary::new();
    source
        .borrow()
        .as_any()
        .downcast_ref::<CpdfDictionary>()
        .unwrap()
        .with_locked_entries(|entries| {
            for (key, value) in entries {
                d.set_for(key, Some(Rc::clone(value)));
            }
        });
    retain_object(d)
}
fn copy_resources(source: &ObjectRef) -> ObjectRef {
    let result = copy_dictionary(source);
    for key in [b"Font".as_slice(), b"XObject".as_slice()] {
        if let Some(child) = typed(dict_get(source, key), ObjectType::Dictionary) {
            dict_mut(&result, |d| {
                d.set_for(key, Some(copy_dictionary(&child)));
            });
        }
    }
    result
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn caption_styles_and_all_marks_are_drawn() {
        let r = CfxFloatRect::new(2., 3., 22., 23.);
        for (code, style) in [
            (52, CheckStyle::Check),
            (56, CheckStyle::Cross),
            (72, CheckStyle::Star),
            (108, CheckStyle::Circle),
            (110, CheckStyle::Square),
            (117, CheckStyle::Diamond),
        ] {
            assert_eq!(caption_style(&[code]), Some(style));
            let output = check_mark(r, style, CfxColor::make_gray(0.), false);
            assert!(output.starts_with("q\n"));
            assert!(output.ends_with("Q\n"));
            assert!(output.contains(if style == CheckStyle::Cross {
                "S\n"
            } else {
                "f\n"
            }));
        }
        assert_eq!(caption_style(&[]), None);
    }
    #[test]
    fn circular_border_states_and_transparent_fills() {
        let r = CfxFloatRect::new(0., 0., 20., 20.);
        assert!(rect_fill(r, CfxColor::default()).is_empty());
        for style in [
            BorderStyle::Solid,
            BorderStyle::Dash,
            BorderStyle::Beveled,
            BorderStyle::Inset,
            BorderStyle::Underline,
        ] {
            let output = circle_border(
                r,
                2.,
                CfxColor::make_gray(0.),
                CfxColor::make_gray(1.),
                CfxColor::make_gray(0.5),
                style,
                Dash {
                    dash: 3,
                    gap: 3,
                    phase: 0,
                },
            );
            assert!(output.contains("c\n"));
            assert!(output.contains("S\n"));
            if style == BorderStyle::Dash {
                assert!(output.contains("[3 3] 0 d"));
            }
        }
    }
    #[test]
    fn binary_pdf_words_preserve_font_codes() {
        let encoded = word_render(&[0, 128, 255, b'(', b')']);
        assert!(encoded.is_ascii());
        assert_eq!(encoded, "<0080ff2829> Tj\n");
    }
    #[test]
    fn regenerating_shared_appearance_preserves_resources_and_original_stream() {
        use crate::fpdfapi::parser::cpdf_array::CpdfArray;
        use crate::fpdfapi::parser::cpdf_document::CpdfDocument;
        use crate::fpdfdoc::cpdf_interactiveform::CpdfInteractiveForm;
        use std::cell::RefCell;
        let document = Rc::new(RefCell::new(CpdfDocument::new()));
        document.borrow_mut().create_new_doc();
        let mut font = CpdfDictionary::new();
        font.set_new_name_for(b"Type", b"Font");
        font.set_new_name_for(b"Subtype", b"Type1");
        font.set_new_name_for(b"BaseFont", b"Helvetica");
        let font = retain_object(font);
        let mut fonts = CpdfDictionary::new();
        fonts.set_for(b"F1", Some(Rc::clone(&font)));
        let mut resources = CpdfDictionary::new();
        resources.set_new_for(b"Font", fonts);
        let resources = retain_object(resources);
        let mut stream_dict = CpdfDictionary::new();
        stream_dict.set_for(b"Resources", Some(Rc::clone(&resources)));
        let original = retain_object(CpdfStream::from_data_and_dict(
            b"original".to_vec(),
            retain_object(stream_dict),
        ));
        let holder = document.borrow().holder();
        let original_number = holder
            .borrow_mut()
            .add_indirect_object(Rc::clone(&original));
        let mut ap = CpdfDictionary::new();
        ap.set_new_for(b"N", CpdfReference::new(&holder, original_number));
        let mut widget = CpdfDictionary::new();
        widget.set_new_name_for(b"Subtype", b"Widget");
        widget.set_new_name_for(b"FT", b"Tx");
        widget.set_new_string_for(b"T", b"field");
        widget.set_rect_for(b"Rect", CfxFloatRect::new(0., 0., 100., 20.));
        widget.set_new_for(b"AP", ap);
        let mut fields = CpdfArray::new();
        fields.append_new(widget);
        let root = document.borrow().root().unwrap();
        dict_mut(&root, |r| {
            let mut acro = CpdfDictionary::new();
            acro.set_new_for(b"Fields", fields);
            r.set_new_for(b"AcroForm", acro);
        });
        let form = CpdfInteractiveForm::new(&document);
        let field = form.get_field(0, &[]).unwrap();
        let widget = CpdfSdkWidget::new(&document, field.get_control(0).unwrap());
        let app = CpdfSdkAppStream::new(&widget);
        assert!(app.write(b"N", "first".into(), b""));
        let first = dict_get(&app.ap_dict(), b"N").unwrap();
        assert!(!Rc::ptr_eq(&first, &original));
        let first_dict = first
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let first_resources = dict_get(&first_dict, b"Resources").unwrap();
        assert!(!Rc::ptr_eq(&first_resources, &resources));
        let first_fonts = dict_get(&first_resources, b"Font").unwrap();
        assert!(Rc::ptr_eq(&dict_get(&first_fonts, b"F1").unwrap(), &font));
        assert!(app.write(b"N", "second".into(), b""));
        let second = dict_get(&app.ap_dict(), b"N").unwrap();
        assert!(!Rc::ptr_eq(&first, &second));
        let second_dict = second
            .borrow()
            .as_any()
            .downcast_ref::<CpdfStream>()
            .unwrap()
            .get_dict();
        let second_fonts =
            dict_get(&dict_get(&second_dict, b"Resources").unwrap(), b"Font").unwrap();
        assert!(Rc::ptr_eq(&dict_get(&second_fonts, b"F1").unwrap(), &font));
        assert_eq!(
            original
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_in_memory_raw_data(),
            b"original"
        );
        assert_eq!(
            first
                .borrow()
                .as_any()
                .downcast_ref::<CpdfStream>()
                .unwrap()
                .get_in_memory_raw_data(),
            b"first"
        );
    }
}
