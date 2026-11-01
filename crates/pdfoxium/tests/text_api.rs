// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use pdfoxium::{Document, Error, Point, RenderConfig, SearchOptions, Size};

fn fixture() -> Vec<u8> {
    let plain = "BT /F1 12 Tf 20 100 Td (banana BANANA) Tj ET";
    let unicode = "/Span << /ActualText <FEFF004100E9D83DDE420042> >> BDC BT /F1 12 Tf 20 100 Td (ABCD) Tj ET EMC";
    let objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 6 0 R >>".to_owned(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Resources << /Font << /F1 5 0 R >> >> /Contents 7 0 R >>".to_owned(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        format!("<< /Length {} >>\nstream\n{plain}\nendstream", plain.len()),
        format!("<< /Length {} >>\nstream\n{unicode}\nendstream", unicode.len()),
    ];
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (index, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", index + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!(
        "xref\n0 {}\n0000000000 65535 f \n",
        objects.len() + 1
    ));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
        objects.len() + 1
    ));
    pdf.into_bytes()
}

#[test]
fn utf8_search_and_rect_search_preserve_exact_ranges_after_unload() {
    let document = Document::from_bytes(fixture()).unwrap();
    let page = document.page(1).unwrap();
    let text = page.text().unwrap();
    assert_eq!(text.as_str(), "Aé🙂B");
    let unicode_selection = text.select(1..7).unwrap();
    assert_eq!(unicode_selection.text(), "é🙂");
    let display = unicode_selection.display_rects();
    assert!(!display.is_empty());
    assert_eq!(unicode_selection.byte_range(), 1..7);
    let id = page.id();
    page.unload().unwrap();
    drop((page, document));
    assert_eq!(unicode_selection.display_rects(), display);
    let hit = text
        .search("é🙂", SearchOptions::default())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(hit.text(), "é🙂");
    assert_eq!(hit.byte_range(), 1..7);
    assert_eq!(hit.page_id(), id);
    assert_eq!(&text.as_str()[hit.byte_range()], hit.text());
    let highlighted = text
        .search_with_rects("é🙂", SearchOptions::default())
        .unwrap()
        .next()
        .unwrap()
        .unwrap();
    assert_eq!(highlighted.byte_range(), hit.byte_range());
    assert_eq!(highlighted.text(), hit.text());
    assert!(!highlighted.rects.is_empty());
    assert!(highlighted
        .rects
        .iter()
        .all(|r| r.right > r.left && r.top > r.bottom));
    assert!(matches!(text.select(2..3), Err(Error::InvalidArgument(_))));
    assert!(matches!(
        text.select(0..100),
        Err(Error::InvalidArgument(_))
    ));
    let selected = text.select(hit.byte_range()).unwrap();
    assert_eq!(selected.text(), hit.text());
    assert_eq!(selected.rects(), highlighted.rects);
    assert!(text.select(0..0).unwrap().rects().is_empty());
}

#[test]
fn search_options_selection_hit_testing_and_snapshot_identity() {
    let document = Document::from_bytes(fixture()).unwrap();
    let page = document.page(0).unwrap();
    let text = page.text().unwrap();
    assert_eq!(
        text.search("ana", SearchOptions::default())
            .unwrap()
            .count(),
        2
    );
    assert_eq!(
        text.search(
            "ana",
            SearchOptions {
                overlapping: true,
                ..Default::default()
            }
        )
        .unwrap()
        .count(),
        4
    );
    assert_eq!(
        text.search(
            "BANANA",
            SearchOptions {
                match_case: true,
                ..Default::default()
            }
        )
        .unwrap()
        .count(),
        1
    );
    assert_eq!(
        text.search(
            "ana",
            SearchOptions {
                match_whole_word: true,
                ..Default::default()
            }
        )
        .unwrap()
        .count(),
        0
    );
    assert!(text.search("", SearchOptions::default()).is_err());
    let rect = text.select(0..1).unwrap().rects()[0];
    let point = Point {
        x: (rect.left + rect.right) / 2.0,
        y: (rect.bottom + rect.top) / 2.0,
    };
    let tolerance = Size {
        width: 0.0,
        height: 0.0,
    };
    let position = text.hit_test(point, tolerance).unwrap().unwrap();
    assert_eq!(
        text.select_between(&position, &position).unwrap().text(),
        "b"
    );
    let other = page.text().unwrap();
    assert!(matches!(
        other.select_between(&position, &position),
        Err(Error::InvalidArgument(_))
    ));
    assert!(text
        .hit_test(
            Point {
                x: f32::NAN,
                y: 0.0
            },
            tolerance
        )
        .is_err());
    assert!(text
        .hit_test(
            Point {
                x: -100.0,
                y: -100.0
            },
            tolerance
        )
        .unwrap()
        .is_none());
}

#[test]
fn document_search_is_lazy_detached_and_respects_page_leases() {
    let document = Document::from_bytes(fixture()).unwrap();
    let hits = document
        .search("banana", SearchOptions::default())
        .unwrap()
        .collect::<pdfoxium::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].page_index, 0);
    assert_eq!(hits[0].text, "banana");
    assert_eq!(hits[1].text, "BANANA");
    let with_rects = document
        .search_with_rects("banana", SearchOptions::default())
        .unwrap()
        .collect::<pdfoxium::Result<Vec<_>>>()
        .unwrap();
    assert_eq!(with_rects[0].hit.byte_range, hits[0].byte_range);
    assert!(!with_rects[0].rects.is_empty());
    let page = document.page(0).unwrap();
    let job = page.render_job(&RenderConfig::fit(50, 50)).unwrap();
    let mut search = document.search("banana", SearchOptions::default()).unwrap();
    assert!(matches!(search.next(), Some(Err(Error::Busy))));
    assert!(search.next().is_none());
    drop(job);
    drop((page, document));
    assert_eq!(hits[0].text, "banana");
}

#[test]
fn document_search_rejects_changed_page_order() {
    let document = Document::from_bytes(fixture()).unwrap();
    let source = Document::from_bytes(fixture()).unwrap();
    let mut search = document.search("banana", SearchOptions::default()).unwrap();
    let hit = search.next().unwrap().unwrap();
    document.import_pages(&source, &[1], 0).unwrap();
    assert!(matches!(search.next(), Some(Err(Error::Stale))));
    assert!(search.next().is_none());
    assert_eq!(hit.page_index, 0); // Index at extraction, paired with stable identity.
    assert_eq!(document.page_by_id(hit.page_id).unwrap().index(), 1);
}
