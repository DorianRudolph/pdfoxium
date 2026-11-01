// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use super::*;
use std::time::{Duration, Instant};

pub fn fixture(count: usize) -> Arc<[u8]> {
    fixture_with_text(count, "Hello banana BANANA")
}
fn fixture_with_text(count: usize, text: &str) -> Arc<[u8]> {
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        String::new(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
    ];
    let mut kids = String::new();
    for index in 0..count {
        let page = objects.len() + 1;
        kids.push_str(&format!("{page} 0 R "));
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 240 260] /CropBox [10 20 210 220] /Rotate {} /Resources << /Font << /F1 3 0 R >> >> /Contents {} 0 R >>",index%4*90,page+1));
        let content = if index == 4 {
            "/Span << /ActualText <FEFF004100E9D83DDE420042> >> BDC BT /F1 12 Tf 30 130 Td (ABCD) Tj ET EMC"
        } else {
            &format!("BT /F1 12 Tf 30 130 Td ({text}) Tj ET")
        };
        objects.push(format!(
            "<< /Length {} >>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    objects[1] = format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>");
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj\n{object}\nendobj\n", i + 1));
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
    pdf.into_bytes().into()
}
fn engine() -> Engine {
    Engine {
        document: Document::from_bytes(fixture(12)).unwrap(),
        cache: VecDeque::new(),
        search_page: None,
    }
}
fn endpoint(engine: &mut Engine, page: usize, range: Range<usize>) -> Location {
    let p = engine.page(page).unwrap();
    let rect = p.text.select(range).unwrap().rects()[0];
    Location {
        page,
        point: normalized(p.matrix, rect).center(),
        tolerance: Vec2::splat(0.01),
    }
}
#[test]
fn cropped_rotated_page_search_geometry_and_reverse_drag_match_ranges() {
    let mut engine = engine();
    let shared = Shared::default();
    shared.selection_id.store(7, Ordering::Relaxed);
    for page in 0..4 {
        let first = endpoint(&mut engine, page, 6..7);
        let last = endpoint(&mut engine, page, 11..12);
        for (a, f) in [(first, last), (last, first)] {
            let selection = engine
                .selection(
                    &SelectionRequest {
                        id: 7,
                        select: Select::Points(a, f),
                    },
                    &shared,
                )
                .unwrap();
            assert_eq!(selection.text, "banana");
            assert_eq!(selection.pages[0].range, 6..12);
            let matrix = engine.page(page).unwrap().matrix;
            let raw = Point { x: 60.0, y: 130.0 };
            let back = inverse(matrix).unwrap().transform(matrix.transform(raw));
            assert!((back.x - raw.x).abs() < 0.001 && (back.y - raw.y).abs() < 0.001);
            let bitmap = engine
                .document
                .page(page)
                .unwrap()
                .render(&RenderConfig::fit(400, 400))
                .unwrap();
            let mut ink = 0;
            for rect in &selection.pages[0].rects {
                let x0 = (rect.min.x * bitmap.width() as f32).floor().max(0.0) as u32;
                let x1 = (rect.max.x * bitmap.width() as f32)
                    .ceil()
                    .min(bitmap.width() as f32) as u32;
                let y0 = (rect.min.y * bitmap.height() as f32).floor().max(0.0) as u32;
                let y1 = (rect.max.y * bitmap.height() as f32)
                    .ceil()
                    .min(bitmap.height() as f32) as u32;
                for y in y0..y1 {
                    for x in x0..x1 {
                        let offset = y as usize * bitmap.stride() + x as usize * 4;
                        if bitmap.pixels()[offset] < 128 {
                            ink += 1;
                        }
                    }
                }
            }
            assert!(
                ink > 50,
                "normalized highlight must cover rendered text, page {page}"
            );
        }
    }
}
#[test]
fn cross_page_selection_unicode_ranges_and_cache_residency() {
    let mut engine = engine();
    let shared = Shared::default();
    shared.selection_id.store(1, Ordering::Relaxed);
    let start = endpoint(&mut engine, 0, 6..7);
    let end = endpoint(&mut engine, 1, 4..5);
    let selected = engine
        .selection(
            &SelectionRequest {
                id: 1,
                select: Select::Points(start, end),
            },
            &shared,
        )
        .unwrap();
    assert_eq!(selected.text, "banana BANANA\nHello");
    assert_eq!(selected.pages.len(), 2);
    let selected = engine
        .selection(
            &SelectionRequest {
                id: 1,
                select: Select::Range(4, 1..7),
            },
            &shared,
        )
        .unwrap();
    assert_eq!(selected.text, "é🙂");
    assert!(!selected.pages[0].rects.is_empty());
    assert!(engine
        .selection(
            &SelectionRequest {
                id: 1,
                select: Select::Range(4, 2..3)
            },
            &shared
        )
        .is_err());
    assert!(engine
        .selection(
            &SelectionRequest {
                id: 2,
                select: Select::Range(0, 0..5)
            },
            &shared
        )
        .is_err());
    for page in 0..12 {
        engine.page(page).unwrap();
    }
    assert_eq!(engine.cache.len(), 8);
}
fn until(state: &mut State, condition: impl Fn(&State) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !condition(state) {
        state.poll();
        assert!(Instant::now() < deadline, "text worker timed out");
        std::thread::sleep(Duration::from_millis(1));
    }
}
#[test]
fn replacement_search_and_selection_discard_stale_work_and_wrap() {
    let mut state = State::new(fixture(12), egui::Context::default());
    state.search("banana".into(), false, 0);
    state.search("Hello".into(), true, 0);
    until(&mut state, |s| !s.searching);
    assert_eq!(state.hits.len(), 11);
    assert!(state.hits.iter().all(|h| h.text == "Hello"));
    assert!(state.active.is_none());
    assert_eq!(state.next(false).unwrap().page, 0);
    assert_eq!(state.next(true).unwrap().page, 11);
    assert_eq!(state.next(false).unwrap().page, 0);
    state.select(0, 0..5);
    state.clear_selection();
    state.select(4, 1..7);
    until(&mut state, |s| !s.selecting);
    assert_eq!(state.selection.text, "é🙂");
    state.search(String::new(), false, 0);
    assert!(state.hits.is_empty());
    assert!(!state.searching);
}
#[test]
fn failed_worker_reports_future_requests_instead_of_hanging() {
    let mut state = State::new(Arc::from(&b"not a PDF"[..]), egui::Context::default());
    state.search("test".into(), false, 0);
    until(&mut state, |s| !s.searching);
    assert!(state.error.is_some());
    state.select(0, 0..1);
    until(&mut state, |s| !s.selecting);
    assert!(state.error.is_some());
    state.request_links(0);
    state.navigate(
        pdfoxium::Destination {
            page_index: 0,
            view: pdfoxium::DestinationView::Fit,
        },
        0,
    );
    until(&mut state, |s| !s.navigation_busy());
    assert!(state.navigation_error.is_some());
}

#[test]
fn reader_navigation_is_lazy_and_cancels_stale_destinations() {
    let mut state = State::new(fixture(4), egui::Context::default());
    state.request_links(1);
    until(&mut state, |s| s.links.contains_key(&1));
    assert!(state.links[&1].is_empty());
    assert!(state.hits.is_empty());
    state.navigate(
        pdfoxium::Destination {
            page_index: 1,
            view: pdfoxium::DestinationView::Fit,
        },
        0,
    );
    state.navigate(
        pdfoxium::Destination {
            page_index: 2,
            view: pdfoxium::DestinationView::Fit,
        },
        0,
    );
    until(&mut state, |s| !s.navigating);
    assert_eq!(state.destination.take().unwrap().destination.page_index, 2);
    state.navigate(
        pdfoxium::Destination {
            page_index: 0,
            view: pdfoxium::DestinationView::Fit,
        },
        0,
    );
    assert!(state.next(false).is_none());
    state.cancel_navigation();
    let deadline = Instant::now() + Duration::from_millis(30);
    while Instant::now() < deadline {
        state.poll();
        std::thread::yield_now();
    }
    assert!(state.destination.is_none());
    state.navigate(
        pdfoxium::Destination {
            page_index: 99,
            view: pdfoxium::DestinationView::Fit,
        },
        0,
    );
    until(&mut state, |s| !s.navigating);
    assert!(state.navigation_error.is_some());
}

#[test]
fn explicit_navigation_cancels_a_pending_search_step() {
    let mut state = State::new(fixture(10), egui::Context::default());
    state.search("banana".into(), false, 0);
    assert!(state.next(false).is_none());
    state.cancel_navigation();
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.searching {
        assert!(state.poll().is_none());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(!state.hits.is_empty());
    assert!(state.next(false).is_some());
}

#[test]
fn search_starts_at_current_page_without_navigation_and_wraps() {
    let mut state = State::new(fixture(12), egui::Context::default());
    state.search("Hello".into(), true, 8);
    let deadline = Instant::now() + Duration::from_secs(5);
    while state.searching {
        assert!(state.poll().is_none());
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert!(state.active.is_none());
    assert_eq!(
        state.hits.iter().map(|h| h.page).collect::<Vec<_>>(),
        vec![8, 9, 10, 11, 0, 1, 2, 3, 5, 6, 7]
    );
    assert_eq!(state.next(false).unwrap().page, 8);
    assert_eq!(state.next(true).unwrap().page, 7);
    state.search("Hello".into(), true, 8);
    assert!(state.next(false).is_none());
    let mut revealed = None;
    while state.searching {
        revealed = state.poll().or(revealed);
        assert!(Instant::now() < deadline);
        std::thread::yield_now();
    }
    assert_eq!(revealed.unwrap().page, 8);
}

#[test]
fn common_search_visits_every_match_across_document_wrap() {
    // More than the former 10,000-match cutoff before reaching page zero.
    let mut state = State::new(
        fixture_with_text(12, &"a ".repeat(1500)),
        egui::Context::default(),
    );
    state.search("a".into(), true, 2);
    until(&mut state, |s| !s.searching);
    assert_eq!(state.scanned, 12);
    assert!(!state.truncated);
    assert_eq!(state.hits.len(), 11 * 1500);
    let expected = state
        .hits
        .iter()
        .map(|h| (h.page, h.range.clone()))
        .collect::<Vec<_>>();
    assert_eq!(expected.first().unwrap().0, 2);
    assert_eq!(expected.last().unwrap().0, 1);
    for (page, range) in &expected {
        let hit = state.next(false).unwrap();
        assert_eq!((hit.page, &hit.range), (*page, range));
    }
    assert_eq!(state.next(false).unwrap().range, expected[0].1);
    assert_eq!(state.active, Some(0));
    assert_eq!(state.next(true).unwrap().page, 1);
}

#[test]
fn annotation_hit_requests_are_lazy_coalesced_and_report_invalid_pages() {
    let mut state = State::new(
        crate::annotations::picking_tests::annotated_bytes(),
        egui::Context::default(),
    );
    assert!(state.annotations.is_empty());
    state.request_annotations(0);
    state.request_annotations(0);
    until(&mut state, |s| !s.annotations_busy());
    assert_eq!(state.annotations[&0].len(), 2);
    assert!(!state.selecting && !state.searching);
    state.request_annotations(999);
    until(&mut state, |s| !s.annotations_busy());
    assert!(state.error.is_some());
    assert!(state.annotations[&999].is_empty());
    assert!(!state.stopped());
}
