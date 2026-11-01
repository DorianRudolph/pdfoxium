// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{Document, Rect};
fn fixture() -> Vec<u8> {
    let content = "BT /F1 12 Tf 20 160 Td (Hello be) Tj /F2 12 Tf (go) Tj /F1 12 Tf (ne BANANA) Tj 0 -25 Td (Second line) Tj ET BT /F1 12 Tf 200 160 Td (Column) Tj ET";
    let objects = ["<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 220] /Resources << /Font << /F1 4 0 R /F2 5 0 R >> >> /Contents 6 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Oblique >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len())];
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf += &format!("{} 0 obj\n{obj}\nendobj\n", i + 1);
    }
    let xref = pdf.len();
    pdf += &format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
    for offset in offsets {
        pdf += &format!("{offset:010} 00000 n \n");
    }
    pdf += &format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    );
    pdf.into_bytes()
}
fn contains(outer: Rect, inner: Rect) -> bool {
    outer.left <= inner.left
        && outer.right >= inner.right
        && outer.bottom <= inner.bottom
        && outer.top >= inner.top
}
#[test]
fn loose_selection_merges_styles_without_changing_tight_geometry_or_text() {
    let document = Document::from_bytes(fixture()).unwrap();
    let page = document.page(0).unwrap();
    let text = page.text().unwrap();
    let selected = text.select(6..12).unwrap();
    let tight = selected.rects();
    assert_eq!(tight.len(), 3);
    assert_eq!(selected.text(), "begone");
    let display = selected.display_rects();
    assert_eq!(display.len(), 1);
    assert!(tight.iter().all(|r| contains(display[0], *r)));
    assert!(display[0].height() > tight[0].height());
    assert_eq!(selected.byte_range(), 6..12);
    assert_eq!(selected.rects(), tight);
    page.unload().unwrap();
    drop((page, document, text));
    assert_eq!(selected.display_rects(), display);
}
#[test]
fn loose_selection_keeps_lines_columns_partial_ranges_and_empty_selections() {
    let document = Document::from_bytes(fixture()).unwrap();
    let text = document.page(0).unwrap().text().unwrap();
    let all = text.select(0..text.as_str().len()).unwrap().display_rects();
    assert_eq!(all.len(), 3, "{:?}: {all:?}", text.as_str());
    assert!(all.iter().all(|r| r.width() < 170.0));
    let second = text.as_str().find("Second").unwrap();
    let partial = text.select(6..second + 6).unwrap().display_rects();
    assert_eq!(partial.len(), 2);
    assert!(partial[0].left > all[0].left);
    let second_line = all.iter().find(|r| r.top < 150.0).unwrap();
    assert!(partial[1].right < second_line.right);
    assert!(text.select(0..0).unwrap().display_rects().is_empty());
}
