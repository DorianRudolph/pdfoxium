// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Shared viewer chrome. Icons use strokes, keeping them sharp at every DPI.
use eframe::egui::{self, Color32, Stroke};

// Warm paper surfaces and a pine accent, shared by chrome and view-only overlays.
pub const SURFACE: Color32 = Color32::from_rgb(247, 245, 238);
pub const CANVAS: Color32 = Color32::from_rgb(220, 225, 216);
pub const TEXT: Color32 = Color32::from_rgb(40, 57, 50);
pub const MUTED: Color32 = Color32::from_rgb(102, 115, 104);
pub const ACCENT: Color32 = Color32::from_rgb(37, 105, 88);
pub const TEXT_SELECTION: Color32 = Color32::from_rgba_unmultiplied_const(37, 105, 88, 80);

pub fn configure(ctx: &egui::Context) {
    // Eframe applies the system theme after app creation; store our light style
    // explicitly so that first-frame theme detection cannot replace it.
    ctx.set_theme(egui::Theme::Light);
    let mut style = (*ctx.global_style()).clone();
    style.visuals = egui::Visuals::light();
    let v = &mut style.visuals;
    let border = Color32::from_rgb(211, 218, 204);
    let control = Color32::from_rgb(233, 236, 224);
    let hover = Color32::from_rgb(220, 229, 214);
    let selected = Color32::from_rgb(215, 234, 219);
    v.panel_fill = SURFACE;
    v.window_fill = v.panel_fill;
    v.override_text_color = Some(TEXT);
    v.weak_text_color = Some(MUTED);
    v.selection.bg_fill = selected;
    v.selection.stroke = Stroke::new(1_f32, ACCENT);
    v.hyperlink_color = ACCENT;
    v.text_cursor.stroke.color = ACCENT;
    v.extreme_bg_color = Color32::from_rgb(255, 254, 249);
    v.faint_bg_color = Color32::from_rgb(240, 241, 231);
    v.code_bg_color = control;
    v.window_stroke = Stroke::new(1_f32, border);
    v.warn_fg_color = Color32::from_rgb(150, 96, 37);
    v.error_fg_color = Color32::from_rgb(164, 63, 44);
    v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, border);
    v.widgets.noninteractive.fg_stroke.color = MUTED;
    v.widgets.inactive.weak_bg_fill = control;
    v.widgets.inactive.bg_fill = control;
    v.widgets.inactive.fg_stroke.color = TEXT;
    v.widgets.inactive.bg_stroke = Stroke::NONE;
    v.widgets.hovered.weak_bg_fill = hover;
    v.widgets.hovered.bg_fill = hover;
    v.widgets.hovered.fg_stroke.color = ACCENT;
    v.widgets.hovered.bg_stroke = Stroke::NONE;
    v.widgets.active.weak_bg_fill = selected;
    v.widgets.active.bg_fill = selected;
    v.widgets.active.fg_stroke.color = ACCENT;
    v.widgets.active.bg_stroke = Stroke::NONE;
    v.widgets.open = v.widgets.active;
    style.spacing.item_spacing = egui::vec2(6., 8.);
    style.spacing.button_padding = egui::vec2(10., 6.);
    style.spacing.interact_size.y = 28.;
    style.spacing.window_margin = egui::Margin::same(16);
    style
        .text_styles
        .insert(egui::TextStyle::Heading, egui::FontId::proportional(17.));
    style
        .text_styles
        .insert(egui::TextStyle::Small, egui::FontId::proportional(11.));
    ctx.set_style_of(egui::Theme::Light, style);
}

#[derive(Clone, Copy)]
pub enum Icon {
    Previous,
    Next,
    Sidebar,
    Search,
    Annotate,
    Close,
    Text,
    Rectangle,
    Ellipse,
    Note,
    Delete,
}
pub fn icon(
    ui: &mut egui::Ui,
    icon: Icon,
    selected: bool,
    enabled: bool,
    label: &str,
) -> egui::Response {
    ui.add_enabled_ui(enabled, |ui| {
        let r = ui.add(
            egui::Button::new("")
                .min_size(egui::vec2(30., 28.))
                .selected(selected)
                .frame_when_inactive(selected),
        );
        if ui.is_rect_visible(r.rect) {
            let v = ui.style().interact_selectable(&r, selected);
            let stroke = Stroke::new(1.5_f32, v.fg_stroke.color);
            let p = ui.painter();
            let c = r.rect.center();
            let at = |x, y| c + egui::vec2(x, y);
            let line = |a, b| {
                p.line_segment([a, b], stroke);
            };
            let rect = |x, y, w, h| {
                p.rect_stroke(
                    egui::Rect::from_min_size(at(x, y), egui::vec2(w, h)),
                    1.,
                    stroke,
                    egui::StrokeKind::Inside,
                );
            };
            match icon {
                Icon::Previous | Icon::Next => {
                    let d = if matches!(icon, Icon::Previous) {
                        -1.
                    } else {
                        1.
                    };
                    line(at(-d * 2., -5.), at(d * 3., 0.));
                    line(at(d * 3., 0.), at(-d * 2., 5.));
                }
                Icon::Sidebar => {
                    rect(-7., -6., 14., 12.);
                    line(at(-2., -6.), at(-2., 6.));
                }
                Icon::Search => {
                    p.circle_stroke(at(-1.5, -1.5), 4.5, stroke);
                    line(at(2., 2.), at(7., 7.));
                }
                Icon::Close => {
                    line(at(-4., -4.), at(4., 4.));
                    line(at(-4., 4.), at(4., -4.));
                }
                Icon::Annotate => {
                    line(at(-6., 5.), at(4., -5.));
                    line(at(-4., 7.), at(6., -3.));
                    line(at(4., -5.), at(6., -3.));
                    line(at(-6., 5.), at(-7., 8.));
                    line(at(-7., 8.), at(-4., 7.));
                }
                Icon::Text => {
                    // Stroke tessellation can round the two serif endpoints
                    // differently at fractional DPI/centres. Integer pixel
                    // edges and matching width parity keep the T symmetric.
                    let scale = ui.ctx().pixels_per_point();
                    let thickness = (1.5 * scale).round().max(1.);
                    let stem_x = (c.x * scale - thickness / 2.).round();
                    let top = (c.y * scale - 6. * scale).round();
                    let bottom = (c.y * scale + 5. * scale).round();
                    let bar = |width: f32, y: f32, height: f32| {
                        let wing = ((width * scale - thickness) / 2.).round().max(0.);
                        p.rect_filled(
                            egui::Rect::from_min_max(
                                egui::pos2((stem_x - wing) / scale, y / scale),
                                egui::pos2(
                                    (stem_x + thickness + wing) / scale,
                                    (y + height) / scale,
                                ),
                            ),
                            0.,
                            stroke.color,
                        );
                    };
                    bar(12., top, thickness);
                    bar(0., top + thickness, bottom - top - thickness);
                    bar(6., bottom, thickness);
                }
                Icon::Rectangle => rect(-7., -5., 14., 10.),
                Icon::Ellipse => {
                    p.circle_stroke(c, 6., stroke);
                }
                Icon::Note => {
                    rect(-6., -6., 12., 12.);
                    line(at(-3., -2.), at(3., -2.));
                    line(at(-3., 1.), at(3., 1.));
                }
                Icon::Delete => {
                    rect(-4., -3., 8., 10.);
                    line(at(-6., -5.), at(6., -5.));
                    line(at(-2., -7.), at(2., -7.));
                }
            }
        }
        r.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, enabled, label));
        r.on_hover_text(label)
    })
    .inner
}

pub fn section(ui: &mut egui::Ui, label: &str) {
    ui.add_space(8.);
    ui.label(
        egui::RichText::new(label)
            .small()
            .strong()
            .color(ui.visuals().weak_text_color()),
    );
    ui.add_space(2.);
}
