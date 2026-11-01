// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Connected editing -> FGAS layout -> existing AGG glyph rendering checks.
use super::{
    cfde_texteditengine::{CfdeTextEditEngine, RecordOperation, TextChange, TextEditDelegate},
    cfde_textout::CfdeTextOut,
};
use crate::{
    fpdfapi::parser::cpdf_document::CpdfDocument,
    fxcrt::fx_coordinates::CfxMatrix,
    fxge::{
        agg::cfx_agg_devicedriver::CfxAggDeviceDriver,
        dib::{cfx_dibitmap::CfxDibitmap, fx_dib::FxdibFormat},
    },
    xfa::fgas::font::cfgas_gefont::CfgasGeFont,
};

fn wide(text: &str) -> Vec<u32> {
    text.chars().map(u32::from).collect()
}

fn engine() -> CfdeTextEditEngine {
    let font = CfgasGeFont::load_stock_font(&CpdfDocument::new(), b"Helvetica-Bold").unwrap();
    let mut engine = CfdeTextEditEngine::new();
    engine.set_font(Some(font));
    engine.set_font_size(14.);
    engine.set_line_space(18.);
    engine.enable_multi_line(true);
    engine.enable_line_wrap(true);
    engine.set_available_width(160);
    engine
}

fn render(engine: &mut CfdeTextEditEngine) -> Vec<u8> {
    let mut bitmap = CfxDibitmap::create(180, 100, FxdibFormat::Bgra, 0).unwrap();
    let pieces = engine.get_text_pieces().to_vec();
    let font = engine.get_font().unwrap();
    let size = engine.get_font_size();
    let color = engine.get_font_color();
    {
        let mut device = CfxAggDeviceDriver::new(&mut bitmap, false, None);
        for piece in pieces {
            let mut positions = engine.get_display_pos(&piece);
            assert!(CfdeTextOut::draw_string(
                &mut device,
                color,
                &font,
                &mut positions,
                size,
                CfxMatrix::new(1., 0., 0., 1., 5., 5.),
            ));
        }
    }
    bitmap.buffer().to_vec()
}

#[test]
fn edits_undo_and_redo_restore_rendered_glyphs() {
    let mut engine = engine();
    engine.insert(
        0,
        &wide("First line\nSecond line"),
        RecordOperation::InsertRecord,
    );
    let original = render(&mut engine);
    assert!(
        original
            .chunks_exact(4)
            .filter(|pixel| pixel[3] != 0)
            .count()
            > 100
    );
    assert!(engine.get_text_pieces().len() >= 2);
    engine.set_selection(6, 4);
    engine.replace_selected_text(&wide("changed"));
    let changed = render(&mut engine);
    assert_ne!(changed, original);
    assert_eq!(engine.get_text(), wide("First changed\nSecond line"));
    assert!(engine.undo());
    assert_eq!(render(&mut engine), original);
    assert!(engine.redo());
    assert_eq!(render(&mut engine), changed);
    engine.clear();
    assert!(render(&mut engine).iter().all(|&byte| byte == 0));
    assert!(engine.get_text_pieces().is_empty());
}

#[test]
fn password_layout_uses_alias_outlines_without_changing_text() {
    let mut password = engine();
    password.enable_password_mode(true);
    password.set_alias_char(u32::from('*'));
    password.insert(0, &wide("Private"), RecordOperation::InsertRecord);
    let concealed = render(&mut password);
    let mut aliases = engine();
    aliases.insert(0, &wide("*******"), RecordOperation::InsertRecord);
    assert_eq!(concealed, render(&mut aliases));
    assert_eq!(password.get_text(), wide("Private"));
    password.enable_password_mode(false);
    assert_ne!(concealed, render(&mut password));
    assert_eq!(password.get_text(), wide("Private"));
}

#[test]
fn delegate_can_reenter_and_cancel_the_outer_edit() {
    use std::{cell::Cell, rc::Rc};
    struct Delegate(Cell<bool>);
    impl TextEditDelegate for Delegate {
        fn on_text_will_change(&self, engine: &mut CfdeTextEditEngine, change: &mut TextChange) {
            if self.0.replace(false) {
                engine.insert(0, &wide("Nested"), RecordOperation::SkipNotify);
                change.cancelled = true;
            }
        }
    }
    let mut engine = engine();
    engine.set_delegate(Some(Rc::new(Delegate(Cell::new(true)))));
    engine.insert(0, &wide("Cancelled"), RecordOperation::InsertRecord);
    assert_eq!(engine.get_text(), wide("Nested"));
    engine.insert(6, &wide(" edit"), RecordOperation::InsertRecord);
    assert_eq!(engine.get_text(), wide("Nested edit"));
    assert!(render(&mut engine)
        .chunks_exact(4)
        .any(|pixel| pixel[3] != 0));
    assert!(engine.undo());
    assert_eq!(engine.get_text(), wide("Nested"));
}

#[test]
fn limited_insert_undo_and_partial_selection_preserve_neighbors() {
    let mut engine = engine();
    engine.insert(0, &wide("Keep"), RecordOperation::InsertRecord);
    engine.set_has_character_limit(true);
    engine.set_character_limit(6);
    engine.insert(2, &wide("XYZ"), RecordOperation::InsertRecord);
    assert_eq!(engine.get_text(), wide("KeXYep"));
    assert!(engine.undo());
    assert_eq!(engine.get_text(), wide("Keep"));
    assert!(engine.redo());
    assert_eq!(engine.get_text(), wide("KeXYep"));
    let (_, character) = engine.get_character_info(2).unwrap();
    let selection = engine.get_character_rects_in_range(2, 1);
    assert_eq!(selection, vec![character]);
    assert!(character.left > 0.);
}
