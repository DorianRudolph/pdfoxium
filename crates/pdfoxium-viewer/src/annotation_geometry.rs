// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Viewer-created markup uses continuous line bands, not text-object ink boxes.
use pdfoxium::{Rect, TextPage};
use std::ops::Range;

pub fn markup_rects(text: &TextPage, range: Range<usize>) -> pdfoxium::Result<Vec<Rect>> {
    let selection = text.select(range.clone())?;
    let mut start = range.start;
    let mut result = Vec::new();
    // PDFium groups its selection boxes by text object, which may contain
    // multiple lines. Split the selected UTF-8 range at extracted line breaks
    // before grouping, so even one multiline text object cannot bridge lines.
    for part in selection.text().split_inclusive(['\r', '\n']) {
        let end = start + part.trim_end_matches(['\r', '\n']).len();
        if end > start {
            result.extend(group_line(text.select(start..end)?.rects()));
        }
        start += part.len();
    }
    Ok(result)
}

fn group_line(mut rects: Vec<Rect>) -> Vec<Rect> {
    rects.sort_by(|a, b| a.left.total_cmp(&b.left));
    let mut groups: Vec<Rect> = Vec::new();
    let mut reference: Option<Rect> = None;
    for rect in rects {
        if let (Some(previous), Some(band)) = (groups.last_mut(), reference) {
            let height = band.height().max(rect.height());
            let overlap = band.top.min(rect.top) - band.bottom.max(rect.bottom);
            let gap = (rect.left - previous.right).max(0.0);
            // Require a substantial shared vertical band and a text-sized gap.
            // Compare against the first run, not the growing union, to prevent
            // transitive merging down a staircase into the next line.
            if overlap >= height * 0.5 && gap <= height * 0.75 {
                previous.left = previous.left.min(rect.left);
                previous.right = previous.right.max(rect.right);
                previous.bottom = previous.bottom.min(rect.bottom);
                previous.top = previous.top.max(rect.top);
                continue;
            }
        }
        reference = Some(rect);
        groups.push(rect);
    }
    groups
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(left: f32, bottom: f32, right: f32, top: f32) -> Rect {
        Rect {
            left,
            bottom,
            right,
            top,
        }
    }
    #[test]
    fn adjacent_runs_share_a_band_but_columns_and_other_lines_do_not() {
        assert_eq!(
            group_line(vec![
                r(10., 100., 25., 110.),
                r(28., 101., 40., 109.),
                r(42., 99., 55., 111.)
            ]),
            vec![r(10., 99., 55., 111.)]
        );
        assert_eq!(
            group_line(vec![r(10., 100., 25., 110.), r(100., 100., 125., 110.)]).len(),
            2
        );
        assert_eq!(
            group_line(vec![r(10., 100., 25., 110.), r(26., 80., 40., 90.)]).len(),
            2
        );
        assert_eq!(
            group_line(vec![
                r(10., 100., 25., 110.),
                r(26., 105., 40., 115.),
                r(41., 110., 55., 120.)
            ])
            .len(),
            2
        );
        // Reversed object order / overlapping italic bounds still form one band.
        assert_eq!(
            group_line(vec![r(24., 101., 40., 109.), r(10., 100., 25., 110.)]),
            vec![r(10., 100., 40., 110.)]
        );
    }
}

#[cfg(test)]
mod extraction_tests {
    use super::*;
    use pdfoxium::Document;
    fn fixture() -> Document {
        let content="BT /F1 12 Tf 20 150 Td (Hello be) Tj /F2 12 Tf (go) Tj /F1 12 Tf (ne BANANA) Tj 0 -25 Td (Second line) Tj ET";
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
        Document::from_bytes(pdf.into_bytes()).unwrap()
    }
    #[test]
    fn mixed_font_word_becomes_one_quad_and_multiline_object_stays_separate() {
        let doc = fixture();
        let page = doc.page(0).unwrap();
        let text = page.text().unwrap();
        assert_eq!(&text.as_str()[6..12], "begone");
        let raw = text.select(6..12).unwrap().rects();
        assert_eq!(raw.len(), 3);
        assert!((raw[0].top + raw[0].bottom - raw[1].top - raw[1].bottom).abs() > 1.0);
        let grouped = markup_rects(&text, 6..12).unwrap();
        assert_eq!(grouped.len(), 1);
        assert_eq!(grouped[0].left, raw[0].left);
        assert_eq!(grouped[0].right, raw[2].right);
        let lines = markup_rects(&text, 0..text.as_str().len()).unwrap();
        assert_eq!(lines.len(), 2, "{:?}: {lines:?}", text.as_str());
        assert!(lines[0].bottom > lines[1].top);
        let second = text.as_str().find("Second").unwrap();
        let partial = markup_rects(&text, 6..second + 6).unwrap();
        assert_eq!(partial.len(), 2);
        assert_eq!(partial[0].left, raw[0].left);
        assert!(partial[1].right < lines[1].right);
        assert!(markup_rects(&text, 0..0).unwrap().is_empty());
        assert!(markup_rects(&text, 0..usize::MAX).is_err());
    }
}
