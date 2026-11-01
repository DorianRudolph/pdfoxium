// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Create a PDF using the basic editing API: cargo run -p pdfoxium --example edit -- output.pdf
use pdfoxium::{AnnotationSpec, Color, Document, Matrix, Size, StandardFont, TextSpec};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args_os()
        .nth(1)
        .unwrap_or_else(|| "edited.pdf".into());
    let doc = Document::create()?;
    let page = doc.insert_blank_page(
        0,
        Size {
            width: 595.,
            height: 842.,
        },
    )?;
    let font = doc.standard_font(StandardFont::Helvetica)?;
    page.add_text(&TextSpec {
        text: "Edit, highlight, save.",
        font: &font,
        size: 24.,
        matrix: Matrix::translate(72., 740.),
        color: Color::BLACK,
    })?;
    let text = page.text()?;
    let selection = text.select(0..4)?;
    page.add_annotation(
        AnnotationSpec::highlight(selection.rects().to_vec()).contents("A real PDF highlight"),
    )?;
    // Regenerates dirty content into the in-memory PDF, then releases page caches.
    page.unload()?;
    doc.save_to(&mut std::fs::File::create(output)?)?;
    Ok(())
}
