// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Detached appearance drafts shared by new-annotation tools and selected annotations.
use eframe::egui;
use pdfoxium::{AnnotationStyle as Style, Color};
use serde_json::{json, Value};
#[derive(Clone, Copy)]
pub struct Defaults {
    pub text: Style,
    pub shape: Style,
    pub highlight: Style,
}
impl Default for Defaults {
    fn default() -> Self {
        Self {
            text: Style::FreeText {
                font_size: 12.,
                color: Color::BLACK,
            },
            shape: Style::Shape {
                border_color: Color::rgb(200, 40, 40),
                border_width: 1.,
                fill: None,
            },
            highlight: Style::Highlight {
                color: Color::rgba(255, 220, 40, 100),
            },
        }
    }
}
fn rgb(c: Color) -> [u8; 3] {
    [c.red, c.green, c.blue]
}
pub fn snapshot(style: Style) -> Value {
    match style {
        Style::FreeText { font_size, color } => json!({"font_size":font_size,"color":rgb(color)}),
        Style::Shape {
            border_color,
            border_width,
            fill,
        } => {
            json!({"border_color":rgb(border_color),"border_width":border_width,"fill":fill.map(rgb)})
        }
        Style::Highlight { color } => {
            json!({"color":rgb(color),"opacity":f32::from(color.alpha)/255.})
        }
    }
}
fn color(value: &Value, alpha: u8) -> Result<Color, String> {
    let v = value
        .as_array()
        .filter(|v| v.len() == 3)
        .ok_or("color must be [red, green, blue] with bytes 0..255")?;
    let mut rgb = [0u8; 3];
    for (target, source) in rgb.iter_mut().zip(v) {
        *target = source
            .as_u64()
            .and_then(|n| u8::try_from(n).ok())
            .ok_or("color components must be integers 0..255")?;
    }
    Ok(Color::rgba(rgb[0], rgb[1], rgb[2], alpha))
}
fn positive(v: &Value) -> Result<f32, String> {
    let n = v.as_f64().ok_or("style value must be numeric")? as f32;
    if !n.is_finite() || n <= 0. {
        return Err("style value must be positive and finite".into());
    }
    Ok(n)
}
pub fn patch(mut style: Style, values: &Value) -> Result<Style, String> {
    for (key, value) in values.as_object().ok_or("style must be an object")? {
        if matches!(key.as_str(), "id" | "index" | "kind") {
            continue;
        }
        match (&mut style, key.as_str()) {
            (Style::FreeText { font_size, .. }, "font_size") => *font_size = positive(value)?,
            (Style::FreeText { color: c, .. } | Style::Highlight { color: c }, "color") => {
                *c = color(value, c.alpha)?
            }
            (Style::Shape { border_color, .. }, "border_color") => {
                *border_color = color(value, border_color.alpha)?
            }
            (Style::Shape { border_width, .. }, "border_width") => *border_width = positive(value)?,
            (
                Style::Shape {
                    fill, border_color, ..
                },
                "fill",
            ) => {
                *fill = if value.is_null() {
                    None
                } else {
                    Some(color(value, border_color.alpha)?)
                }
            }
            (Style::Highlight { color }, "opacity") => {
                let opacity = value
                    .as_f64()
                    .filter(|n| n.is_finite() && (0.0..=1.0).contains(n))
                    .ok_or("opacity must be 0..1")?;
                color.alpha = (opacity * 255.).round() as u8;
            }
            _ => return Err(format!("{key} is not a setting for this annotation type")),
        }
    }
    Ok(style)
}
type Controls = Vec<(&'static str, &'static str, egui::Rect)>;
fn swatch(
    ui: &mut egui::Ui,
    c: &mut Color,
    id: &'static str,
    label: &'static str,
    controls: &mut Controls,
) {
    let mut values = rgb(*c);
    let response = ui.color_edit_button_srgb(&mut values).on_hover_text(label);
    controls.push((id, label, response.rect));
    if response.changed() {
        *c = Color::rgba(values[0], values[1], values[2], c.alpha);
    }
}
fn color_control(
    ui: &mut egui::Ui,
    c: &mut Color,
    id: &'static str,
    label: &'static str,
    controls: &mut Controls,
) {
    ui.label(label);
    swatch(ui, c, id, label, controls);
    ui.end_row();
}
pub fn show(ui: &mut egui::Ui, style: &mut Style, controls: &mut Controls) {
    let kind = match style {
        Style::FreeText { .. } => "text",
        Style::Shape { .. } => "shape",
        Style::Highlight { .. } => "highlight",
    };
    egui::Grid::new(("annotation-style", kind))
        .num_columns(2)
        .min_col_width(94.)
        .spacing([12., 10.])
        .show(ui, |ui| match style {
            Style::FreeText { font_size, color } => {
                ui.label("Font size");
                let r = ui.add(
                    egui::DragValue::new(font_size)
                        .range(0.1..=200.)
                        .speed(0.5)
                        .suffix(" pt"),
                );
                controls.push(("annotation.style.font_size", "Font size", r.rect));
                ui.end_row();
                color_control(ui, color, "annotation.style.color", "Text color", controls);
            }
            Style::Shape {
                border_color,
                border_width,
                fill,
            } => {
                color_control(
                    ui,
                    border_color,
                    "annotation.style.border_color",
                    "Border",
                    controls,
                );
                ui.label("Width");
                let r = ui.add(
                    egui::DragValue::new(border_width)
                        .range(0.1..=30.)
                        .speed(0.25)
                        .suffix(" pt"),
                );
                controls.push(("annotation.style.border_width", "Border width", r.rect));
                ui.end_row();
                ui.label("Fill");
                ui.horizontal(|ui| {
                    let mut enabled = fill.is_some();
                    let r = ui
                        .checkbox(&mut enabled, "")
                        .on_hover_text("Fill the shape");
                    controls.push(("annotation.style.fill_enabled", "Fill", r.rect));
                    if r.changed() {
                        *fill = enabled.then(|| Color::rgba(255, 230, 150, border_color.alpha));
                    }
                    if let Some(fill) = fill {
                        swatch(
                            ui,
                            fill,
                            "annotation.style.fill_color",
                            "Fill color",
                            controls,
                        );
                    }
                });
                ui.end_row();
            }
            Style::Highlight { color } => {
                color_control(ui, color, "annotation.style.color", "Color", controls);
                let mut opacity = f32::from(color.alpha) * 100. / 255.;
                ui.label("Opacity");
                let r = ui.add(
                    egui::DragValue::new(&mut opacity)
                        .range(0.0..=100.)
                        .speed(1.)
                        .max_decimals(0)
                        .suffix(" %"),
                );
                controls.push(("annotation.style.opacity", "Opacity", r.rect));
                if r.changed() {
                    color.alpha = (opacity * 255. / 100.).round() as u8;
                }
                ui.end_row();
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn style_commands_validate_values_and_keep_color_separate_from_opacity() {
        let defaults = Defaults::default();
        let style = patch(
            defaults.highlight,
            &json!({"color":[10,20,30],"opacity":0.25}),
        )
        .unwrap();
        assert_eq!(
            style,
            Style::Highlight {
                color: Color::rgba(10, 20, 30, 64)
            }
        );
        for invalid in [
            json!({"opacity":-0.1}),
            json!({"opacity":1.1}),
            json!({"color":[256,0,0]}),
            json!({"font_size":12}),
        ] {
            assert!(patch(defaults.highlight, &invalid).is_err());
        }
        let shape = patch(
            defaults.shape,
            &json!({"fill":[100,150,200],"border_width":3}),
        )
        .unwrap();
        assert_eq!(
            snapshot(patch(shape, &json!({"fill":null})).unwrap())["fill"],
            Value::Null
        );
        assert!(patch(defaults.text, &json!({"font_size":0})).is_err());
    }
}
