// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Fixed-height outline rows. Normalize static titles once, at document load.
use eframe::egui::{self, Align, Label, Rect, Response, Sense, Ui, UiBuilder};
use pdfoxium::OutlineItem;

pub fn title(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
pub struct Row {
    pub title: Response,
    pub toggle: bool,
}
pub fn row(
    ui: &mut Ui,
    item: &OutlineItem,
    children: bool,
    collapsed: bool,
    target: &str,
    page_label: Option<&str>,
) -> Row {
    let height = ui
        .spacing()
        .interact_size
        .y
        .max(ui.text_style_height(&egui::TextStyle::Body));
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::hover());
    let slot = 20.0;
    let indent = (item.depth.min(12) as f32 * 12.0).min((rect.width() - slot - 40.0).max(0.0));
    let disclosure =
        Rect::from_min_size(rect.min + egui::vec2(indent, 0.0), egui::vec2(slot, height));
    let toggle = children
        && ui
            .place(
                disclosure,
                egui::Button::new(if collapsed { "+" } else { "−" }).small(),
            )
            .clicked();
    let title_rect = Rect::from_min_max(
        egui::pos2(disclosure.max.x + ui.spacing().item_spacing.x, rect.min.y),
        rect.max,
    );
    let mut child = ui.new_child(
        UiBuilder::new()
            .max_rect(title_rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    child.set_clip_rect(title_rect.intersect(ui.clip_rect()));
    if item.target.is_none() {
        child.disable();
    }
    let text = egui::RichText::new(&item.title);
    let text = if item.bold { text.strong() } else { text };
    let text = if item.italic { text.italics() } else { text };
    let mut text_rect = title_rect;
    if let Some(label) = page_label {
        let label_rect = Rect::from_min_max(
            egui::pos2(
                (title_rect.max.x - 48.0).max(title_rect.min.x),
                title_rect.min.y,
            ),
            title_rect.max,
        );
        text_rect.max.x = (label_rect.min.x - 8.0).max(text_rect.min.x);
        child.painter().with_clip_rect(label_rect).text(
            label_rect.right_center(),
            egui::Align2::RIGHT_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
    }
    let mut text_ui = child.new_child(
        UiBuilder::new()
            .max_rect(text_rect)
            .layout(egui::Layout::left_to_right(Align::Center)),
    );
    text_ui.set_clip_rect(text_rect.intersect(ui.clip_rect()));
    text_ui.add(Label::new(text).halign(Align::Min).truncate());
    let response = child
        .interact(title_rect, child.scope_id().with("title"), Sense::click())
        .on_hover_text(format!("{}\n{}", item.title, target))
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    Row {
        title: response,
        toggle,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn whitespace_and_long_titles_do_not_change_row_spacing() {
        assert_eq!(
            title("  Chapter\r\nOne\t\u{a0}title  \n"),
            "Chapter One title"
        );
        for width in [160.0, 240.0, 480.0] {
            let ctx = egui::Context::default();
            let _ = crate::test_frame(&ctx, egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx,|ui| {
                    ui.set_width(width);
                    let mut previous: Option<f32>=None;
                    for (index,(text,children)) in [("Short",true), ("  \n",false), ("A very long bookmark title that must truncate instead of wrapping to more lines",true), ("Blank trailing lines\n\n",false), ("",true), ("Next",false)].iter().enumerate() {
                        let item=OutlineItem {depth:0,title:title(text),target:Some(pdfoxium::LinkTarget::NamedAction(b"NextPage".to_vec())),color:None,bold:index==1,italic:index==2,open:true};
                        let response=ui.push_id(index,|ui|row(ui,&item,*children,true,"Page 1",Some("i"))).inner.title;
                        let expected=ui.spacing().interact_size.y.max(ui.text_style_height(&egui::TextStyle::Body));
                        assert!((response.rect.height()-expected).abs()<0.01);
                        if let Some(y)=previous { assert!((response.rect.min.y-y-expected-ui.spacing().item_spacing.y).abs()<0.01); }
                        previous=Some(response.rect.min.y);
                        assert!(response.rect.max.x<=ui.max_rect().max.x+0.01);
                    }
                });
            });
        }
    }
}
