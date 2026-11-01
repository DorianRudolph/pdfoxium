// Copyright 2015 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! All tables/assertions from cpdf_linkextract_unittest.cpp.
use super::*;
fn wide(s: &str) -> Vec<u32> {
    s.chars().map(u32::from).collect()
}
#[test]
fn check_mail_link() {
    let invalid = [
        "",
        "peter.pan",
        "abc@server",
        "abc.@gmail.com",
        "abc@xyz&q.org",
        "abc@.xyz.org",
        "fan@g..com",
    ];
    for input in invalid {
        assert!(
            super::check_mail_link(&mut wide(input)).is_none(),
            "{input}"
        );
    }
    let valid = [
        ("peter@abc.d", "peter@abc.d"),
        ("red.teddy.b@abc.com", "red.teddy.b@abc.com"),
        ("abc_@gmail.com", "abc_@gmail.com"),
        ("dummy-hi@gmail.com", "dummy-hi@gmail.com"),
        ("a..df@gmail.com", "df@gmail.com"),
        (".john@yahoo.com", "john@yahoo.com"),
        ("abc@xyz.org?/", "abc@xyz.org"),
        ("fan{abc@xyz.org", "abc@xyz.org"),
        ("fan@g.com..", "fan@g.com"),
        ("CAP.cap@Gmail.Com", "CAP.cap@Gmail.Com"),
    ];
    for (input, expected) in valid {
        let mut actual = wide(input);
        assert!(super::check_mail_link(&mut actual).is_some(), "{input}");
        assert_eq!(actual, wide(&format!("mailto:{expected}")), "{input}");
    }
}
#[test]
fn check_web_link() {
    let invalid = [
        "",
        "http",
        "www.",
        "https-and-www",
        "http:/abc.com",
        "http://((()),",
        "ftp://example.com",
        "http:example.com",
        "http//[example.com",
        "http//[00:00:00:00:00:00",
        "http//[]",
        "abc.example.com",
    ];
    for input in invalid {
        assert!(super::check_web_link(&wide(input)).is_none(), "{input}");
    }
    let valid = [
        ("http://www.example.com", "http://www.example.com", 0, 22),
        (
            "http://www.example.com:88",
            "http://www.example.com:88",
            0,
            25,
        ),
        (
            "http://test@www.example.com",
            "http://test@www.example.com",
            0,
            27,
        ),
        (
            "http://test:test@example.com",
            "http://test:test@example.com",
            0,
            28,
        ),
        ("http://example", "http://example", 0, 14),
        ("http////www.server", "http://www.server", 8, 10),
        ("http:/www.abc.com", "http://www.abc.com", 6, 11),
        ("www.a.b.c", "http://www.a.b.c", 0, 9),
        ("https://a.us", "https://a.us", 0, 12),
        ("https://www.t.us", "https://www.t.us", 0, 16),
        ("www.example-test.com", "http://www.example-test.com", 0, 20),
        ("www.example.com,", "http://www.example.com", 0, 15),
        ("www.example.com;(", "http://www.example.com", 0, 15),
        ("test:www.abc.com", "http://www.abc.com", 5, 11),
        ("(http://www.abc.com)", "http://www.abc.com", 1, 18),
        ("0(http://www.abc.com)0", "http://www.abc.com", 2, 18),
        ("0(www.abc.com)0", "http://www.abc.com", 2, 11),
        ("http://www.abc.com)0", "http://www.abc.com)0", 0, 20),
        ("{(<http://www.abc.com>)}", "http://www.abc.com", 3, 18),
        (
            "[http://www.abc.com/z(1)]",
            "http://www.abc.com/z(1)",
            1,
            23,
        ),
        (
            "(http://www.abc.com/z(1))",
            "http://www.abc.com/z(1)",
            1,
            23,
        ),
        ("\"http://www.abc.com\"", "http://www.abc.com", 1, 18),
        ("www.g.com..", "http://www.g.com..", 0, 11),
        ("http://192.168.0.1", "http://192.168.0.1", 0, 18),
        ("http://192.168.0.1:80", "http://192.168.0.1:80", 0, 21),
        (
            "http://[aa::00:bb::00:cc:00]",
            "http://[aa::00:bb::00:cc:00]",
            0,
            28,
        ),
        (
            "http://[aa::00:bb::00:cc:00]:12",
            "http://[aa::00:bb::00:cc:00]:12",
            0,
            31,
        ),
        ("http://[aa]:12", "http://[aa]:12", 0, 14),
        ("http://[aa]:12abc", "http://[aa]:12", 0, 14),
        ("http://[aa]:", "http://[aa]", 0, 11),
        ("www.abc.com/#%%^&&*(", "http://www.abc.com/#%%^&&*(", 0, 20),
        (
            "www.a.com/#a=@?q=rr&r=y",
            "http://www.a.com/#a=@?q=rr&r=y",
            0,
            23,
        ),
        (
            "http://a.com/1/2/3/4\u{5}\u{6}",
            "http://a.com/1/2/3/4\u{5}\u{6}",
            0,
            22,
        ),
        (
            "http://www.example.com/foo;bar",
            "http://www.example.com/foo;bar",
            0,
            30,
        ),
        ("http://ex[am]ple", "http://ex[am]ple", 0, 16),
        ("http://:example.com", "http://:example.com", 0, 19),
        ("http://((())/path?", "http://((())/path?", 0, 18),
        ("http:////abc.server", "http:////abc.server", 0, 19),
        ("www.测试.net", "http://www.测试.net", 0, 10),
        ("www.测试。net。", "http://www.测试。net。", 0, 11),
        ("www.测试.net；", "http://www.测试.net；", 0, 11),
    ];
    for (input, expected, start, count) in valid {
        let link = super::check_web_link(&wide(input)).unwrap_or_else(|| panic!("{input}"));
        assert_eq!(link.url, wide(expected), "{input}");
        assert_eq!(link.range, Range { start, count }, "{input}");
    }
}
#[test]
fn malformed_brackets_and_short_candidates_are_bounded() {
    for input in [
        "(",
        "()",
        "(http://)",
        "[http://[]]",
        "((www.))",
        "https://",
        "@",
        "a@",
        "a@.",
        "a@b..c",
    ] {
        let _ = super::check_web_link(&wide(input));
        let _ = super::check_mail_link(&mut wide(input));
    }
    assert_eq!(trim_external_brackets(&[40, 41], 1, 1), None);
    assert_eq!(find_web_link_ending(&[104], 1, 0), None);
    let mut address = wide("prefix{user@host.org?trailer");
    assert_eq!(
        super::check_mail_link(&mut address),
        Some(Range {
            start: 7,
            count: 13
        })
    );
    assert_eq!(address, wide("mailto:user@host.org"));
}

fn text_page(lines: &[(&[u8], f32)]) -> Rc<CpdfTextPage> {
    use crate::{
        fpdfapi::{
            font::cpdf_font::CpdfFont,
            page::{
                cpdf_page::CpdfPage, cpdf_pageobject::PageObject, cpdf_textobject::CpdfTextObject,
            },
            parser::cpdf_document::CpdfDocument,
        },
        fxcrt::fx_coordinates::CfxMatrix,
    };
    let mut document = CpdfDocument::new();
    document.create_new_doc();
    document.create_new_page(0).unwrap();
    let mut page = CpdfPage::from_document(&mut document, 0).unwrap();
    let font = CpdfFont::get_stock_font(&document, b"Helvetica").unwrap();
    for &(contents, y) in lines {
        let mut text = CpdfTextObject::new(-1);
        text.base_mut()
            .mutable_text_state()
            .set_font(Some(font.clone()), font.font_dict());
        text.base_mut().mutable_text_state().set_font_size(12.);
        text.set_text(contents);
        text.set_text_matrix(CfxMatrix::new(1., 0., 0., 1., 20., y));
        page.mutable_holder().append_page_object(Box::new(text));
    }
    Rc::new(CpdfTextPage::new(&page, false))
}
#[test]
fn real_text_page_links_have_exact_character_ranges_and_retained_rectangles() {
    let page = text_page(&[(b"Visit http://example.com and user@host.org.", 100.)]);
    assert_eq!(
        page.get_all_page_text(),
        wide("Visit http://example.com and user@host.org.")
    );
    let mut links = CpdfLinkExtract::new(Rc::clone(&page));
    links.extract_links();
    assert_eq!(links.count_links(), 2);
    assert_eq!(links.get_url(0), wide("http://example.com"));
    assert_eq!(
        links.get_text_range(0),
        Some(Range {
            start: 6,
            count: 18
        })
    );
    assert_eq!(links.get_url(1), wide("mailto:user@host.org"));
    assert_eq!(
        links.get_text_range(1),
        Some(Range {
            start: 29,
            count: 13
        })
    );
    for index in 0..2 {
        let range = links.get_text_range(index).unwrap();
        let rects = links.get_rects(index);
        assert_eq!(rects.len(), 1);
        assert!(rects[0].width() > 50. && rects[0].height() > 5.);
        assert_eq!(
            rects,
            page.get_rect_array(range.start as i32, range.count as i32)
        );
    }
    let rects = links.get_rects(0);
    drop(page);
    assert_eq!(links.get_rects(0), rects);
    assert_eq!(links.get_url(usize::MAX), Vec::<u32>::new());
    assert_eq!(links.get_text_range(usize::MAX), None);
    assert!(links.get_rects(usize::MAX).is_empty());
    links.extract_links();
    assert_eq!(links.count_links(), 2);
}
#[test]
fn hyphenated_line_join_link_spans_both_physical_lines() {
    let page = text_page(&[(b"http://example-", 100.), (b"site.org", 80.)]);
    let mut links = CpdfLinkExtract::new(Rc::clone(&page));
    links.extract_links();
    assert_eq!(links.count_links(), 1);
    assert_eq!(links.get_url(0), wide("http://example-site.org"));
    let range = links.get_text_range(0).unwrap();
    let raw = page.get_page_text(range.start as i32, range.count as i32);
    assert!(raw.contains(&0xfffe));
    assert_eq!(range.start, 0);
    assert_eq!(range.count, page.size());
    assert_eq!(links.get_rects(0).len(), 2);
}

#[test]
fn hyphenated_url_path_removes_crlf_but_keeps_full_character_range() {
    let page = text_page(&[(b"http://example.com/path-", 100.), (b"/second", 80.)]);
    let mut links = CpdfLinkExtract::new(Rc::clone(&page));
    links.extract_links();
    assert_eq!(links.count_links(), 1);
    assert_eq!(links.get_url(0), wide("http://example.com/path-/second"));
    let raw = page.get_all_page_text();
    assert!(raw.contains(&13) && raw.contains(&10));
    assert_eq!(
        links.get_text_range(0),
        Some(Range {
            start: 0,
            count: page.size()
        })
    );
    assert_eq!(links.get_rects(0).len(), 2);
}
