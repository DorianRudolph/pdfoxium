// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Region crop gate: at most 2/255 per channel for geometric-clipping AA,
//! with exact cache-history checks and no whole-page fallback for regions.
use pdfoxium::{
    Color, DeviceRect, Document, Matrix, PathSpec, Point, RenderConfig, Rotation, Size,
    StandardFont, Stroke, TextSpec,
};
use std::{path::PathBuf, sync::Arc};

fn fixture() -> Arc<[u8]> {
    let doc = Document::create().unwrap();
    let page = doc
        .insert_blank_page(
            0,
            Size {
                width: 173.,
                height: 127.,
            },
        )
        .unwrap();
    page.add_path(&PathSpec {
        commands: vec![
            pdfoxium::PathCommand::MoveTo(Point {
                x: 7.123,
                y: 20.031,
            }),
            pdfoxium::PathCommand::LineTo(Point {
                x: 150.983,
                y: 90.123,
            }),
            pdfoxium::PathCommand::LineTo(Point {
                x: 2.333,
                y: 110.717,
            }),
            pdfoxium::PathCommand::Close,
        ],
        fill: Some(Color::rgb(240, 240, 255)),
        stroke: Some(Stroke {
            color: Color::rgb(120, 80, 40),
            width: 0.37,
        }),
        ..Default::default()
    })
    .unwrap();
    let font = doc.standard_font(StandardFont::Helvetica).unwrap();
    for y in [17., 39.25, 63.5, 95.] {
        page.add_text(&TextSpec {
            text: "Region edges: AVWj 0123456789",
            font: &font,
            size: 11.3,
            matrix: Matrix::translate(-2.3, y),
            color: Color::BLACK,
        })
        .unwrap();
    }
    for x in [30.8, 61.9, 93.1, 123.9] {
        page.add_path(&PathSpec {
            commands: vec![
                pdfoxium::PathCommand::MoveTo(Point { x, y: 0. }),
                pdfoxium::PathCommand::LineTo(Point { x, y: 127. }),
            ],
            fill: None,
            stroke: Some(Stroke {
                color: Color::rgb(80, 120, 210),
                width: 0.,
            }),
            ..Default::default()
        })
        .unwrap();
    }
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    bytes.into()
}

fn rects(w: u32, h: u32, tile: u32) -> Vec<(u32, u32, u32, u32)> {
    let mut out = Vec::new();
    for y in (0..h).step_by(tile as usize) {
        for x in (0..w).step_by(tile as usize) {
            out.push((x, y, tile.min(w - x), tile.min(h - y)));
        }
    }
    // Merged bands and overlapping irregular regions exercise different clip edges.
    out.push((0, h / 3, w, (h - h / 3).min(tile + 13)));
    out.push((w / 4, 0, (w - w / 4).min(tile * 2 + 7), h));
    out.push((
        w / 3,
        h / 4,
        (w - w / 3).min(tile + 19),
        (h - h / 4).min(tile + 11),
    ));
    out
}
// Geometric clipping can round edge intersections differently from a full-page
// render. Bound every channel difference; never average away a displaced stroke.
const MAX_AA_DIFFERENCE: u8 = 2;

fn pixel_difference(a: &[u8], b: &[u8]) -> (usize, u8) {
    assert_eq!(a.len(), b.len());
    let mut changed = 0;
    let mut maximum = 0;
    for (a, b) in a.chunks_exact(4).zip(b.chunks_exact(4)) {
        let difference = a.iter().zip(b).map(|(&a, &b)| a.abs_diff(b)).max().unwrap();
        changed += usize::from(difference != 0);
        maximum = maximum.max(difference);
    }
    (changed, maximum)
}

fn mismatch(whole: &pdfoxium::Bitmap, part: &pdfoxium::Bitmap, x: u32, y: u32) -> (usize, u8) {
    let mut changed = 0;
    let mut maximum = 0;
    for row in 0..part.height() as usize {
        let a = (row + y as usize) * whole.stride() + x as usize * 4;
        let b = row * part.stride();
        let (count, difference) = pixel_difference(
            &whole.pixels()[a..a + part.width() as usize * 4],
            &part.pixels()[b..b + part.width() as usize * 4],
        );
        changed += count;
        maximum = maximum.max(difference);
    }
    (changed, maximum)
}

#[test]
fn aa_limit_rejects_local_errors_without_averaging() {
    let baseline = [255; 16];
    let mut actual = baseline;
    actual[0] = 253;
    assert_eq!(pixel_difference(&baseline, &actual), (1, MAX_AA_DIFFERENCE));
    actual[15] = 252;
    assert_eq!(
        pixel_difference(&baseline, &actual),
        (2, MAX_AA_DIFFERENCE + 1)
    );
    actual[0] = 0;
    assert_eq!(pixel_difference(&baseline, &actual), (2, 255));
}

fn gate(
    bytes: Arc<[u8]>,
    index: usize,
    scale: f32,
    rotation: Rotation,
    tile: u32,
    label: &str,
    fresh_tiles: bool,
) -> (usize, usize) {
    // Screen blending in this fixture is invisible on white in native too.
    // A gray backdrop makes its mask contribution observable without changing PDF data.
    let backdrop = if label.ends_with("/smask_blend.pdf") {
        [192, 192, 192, 255]
    } else {
        [255; 4]
    };
    let background = Color::rgb(backdrop[0], backdrop[1], backdrop[2]);
    let cfg = RenderConfig::scale(scale)
        .rotation(rotation)
        .background(Some(background));
    let baseline = Document::from_bytes(bytes.clone()).unwrap();
    let whole = baseline.page(index).unwrap().render(&cfg).unwrap();
    assert!(
        whole.pixels().chunks_exact(4).any(|p| p != backdrop),
        "{label} baseline must contain visible content"
    );
    let regions = rects(whole.width(), whole.height(), tile);
    let mut failed = 0;
    let mut checked = 0;
    let mut changed_pixels = 0;
    let mut max_channel_difference = 0;
    for (history, warm_scale, reverse) in [
        ("cold", None, false),
        ("warm", Some(scale), true),
        ("thumbnail-first", Some(scale * 0.2), false),
        ("larger-first", Some(scale * 1.5), true),
    ] {
        let doc = Document::from_bytes(bytes.clone()).unwrap();
        let page = doc.page(index).unwrap();
        if let Some(s) = warm_scale {
            page.render(
                &RenderConfig::scale(s)
                    .rotation(rotation)
                    .background(Some(background)),
            )
            .unwrap();
        }
        let mut order = regions.clone();
        if reverse {
            order.reverse();
        }
        for (i, &(x, y, w, h)) in order.iter().enumerate() {
            let region = cfg.clone().device_clip(DeviceRect::new(x, y, w, h));
            let part = page.render(&region).unwrap();
            let (n, maximum) = mismatch(&whole, &part, x, y);
            checked += 1;
            changed_pixels += n;
            max_channel_difference = max_channel_difference.max(maximum);
            if maximum > MAX_AA_DIFFERENCE {
                failed += 1;
                eprintln!("REGION_DIFF {label} page={index} scale={scale} rotation={rotation:?} history={history} rect={x},{y},{w},{h} pixels={n} max_channel_difference={maximum}");
            }
            // Separate decoded/parsed state per request for small synthetic tests,
            // and representative edge/interior regions for large corpus pages.
            if history == "cold"
                && (fresh_tiles || i == 0 || i == order.len() / 2 || i + 1 == order.len())
            {
                let fresh = Document::from_bytes(bytes.clone()).unwrap();
                let part = fresh.page(index).unwrap().render(&region).unwrap();
                let (n, maximum) = mismatch(&whole, &part, x, y);
                checked += 1;
                changed_pixels += n;
                max_channel_difference = max_channel_difference.max(maximum);
                if maximum > MAX_AA_DIFFERENCE {
                    failed += 1;
                    eprintln!("REGION_DIFF {label} page={index} scale={scale} rotation={rotation:?} history=fresh rect={x},{y},{w},{h} pixels={n} max_channel_difference={maximum}");
                }
            }
        }
        // Check history also cannot change the full-page result itself.
        let after = page.render(&cfg).unwrap();
        checked += 1;
        if whole.pixels() != after.pixels() {
            failed += 1;
            eprintln!("REGION_DIFF {label} page={index} scale={scale} rotation={rotation:?} history={history} full-page");
        }
    }
    eprintln!("REGION_CASE {label} page={index} scale={scale} rotation={rotation:?} comparisons={checked} failed={failed} changed_pixels={changed_pixels} max_channel_difference={max_channel_difference}");
    (checked, failed)
}

#[test]
fn synthetic_tiles_and_merged_regions_match_whole_page_crops_with_small_aa_differences() {
    let bytes = fixture();
    let mut failed = 0;
    let mut checked = 0;
    for scale in [0.25, 1., 1.25, 2.] {
        for rotation in [
            Rotation::None,
            Rotation::Cw90,
            Rotation::Cw180,
            Rotation::Cw270,
        ] {
            let (n, f) = gate(bytes.clone(), 0, scale, rotation, 31, "synthetic", true);
            checked += n;
            failed += f;
        }
    }
    assert_eq!(failed, 0, "{failed}/{checked} region comparisons failed");
}

/// Run explicitly in release mode; textbook inputs are kept outside ordinary CI.
#[test]
#[ignore = "region correctness corpus gate; requires local textbook PDFs"]
fn corpus_tiles_and_merged_regions_match_whole_page_crops_with_small_aa_differences() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let cases = [
        ("tests/render_corpus/books/openstax-biology-2e.pdf", 863),
        ("tests/render_corpus/books/openstax-biology-2e.pdf", 170),
        ("tests/render_corpus/books/hefferon-toc.pdf", 20),
        ("tests/render_corpus/books/hefferon-toc.pdf", 125),
        ("tests/render_corpus/books/obp.0526.pdf", 332),
        ("tests/render_corpus/pdfs/cropped_text.pdf", 0),
        ("tests/render_corpus/pdfs/image_clipped.pdf", 0),
        ("tests/render_corpus/pdfs/alpha_composite.pdf", 0),
        ("tests/render_corpus/external/hayro/pdfjs/issue12841.pdf", 0),
        ("tests/render_corpus/pdfs/axial_shading.pdf", 0),
        ("tests/render_corpus/pdfs/radial_shading.pdf", 0),
        ("tests/render_corpus/pdfs/smask_blend.pdf", 0),
        ("tests/render_corpus/pdfs/type3.pdf", 0),
        ("tests/render_corpus/pdfs/pattern_fill.pdf", 0),
        ("tests/render_corpus/pdfs/pattern_fill_uncolored.pdf", 0),
        ("tests/render_corpus/pdfs/pattern_stroke.pdf", 0),
        ("tests/render_corpus/pdfs/pattern_stroke_uncolored.pdf", 0),
        ("tests/render_corpus/pdfs/rotated_image.pdf", 0),
        ("tests/render_corpus/pdfs/clip_path.pdf", 0),
        ("tests/render_corpus/pdfs/stamp_appearance.pdf", 0),
        (
            "tests/render_corpus/pdfs/annotation_markup_multiline_no_ap.pdf",
            0,
        ),
        ("tests/render_corpus/external/hayro/pdfjs/issue11004.pdf", 4),
    ];
    let filter = std::env::var("PDFOXIUM_REGION_CASE").ok();
    let scales = std::env::var("PDFOXIUM_REGION_SCALE")
        .ok()
        .map(|s| vec![s.parse::<f32>().unwrap()])
        .unwrap_or(vec![0.5, 1., 2., 300. / 72.]);
    let mut checked = 0;
    let mut failed = 0;
    for (name, index) in cases {
        if filter.as_ref().is_some_and(|f| !name.contains(f)) {
            continue;
        }
        let path = root.join(name);
        let bytes: Arc<[u8]> = std::fs::read(&path)
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
            .into();
        for &scale in &scales {
            let (n, f) = gate(
                bytes.clone(),
                index,
                scale,
                Rotation::None,
                512,
                name,
                false,
            );
            checked += n;
            failed += f;
        }
    }
    assert!(checked > 0, "filter selected no cases");
    assert_eq!(
        failed, 0,
        "{failed}/{checked} corpus region comparisons failed"
    );
}

fn image_fixture(encoded: &[u8], filter: &str) -> Arc<[u8]> {
    let content = b"q 128 0 0 128 11 7 cm /I Do Q";
    let mut image=format!("<< /Type /XObject /Subtype /Image /Width 2048 /Height 2048 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /{filter} /Length {} >>\nstream\n",encoded.len()).into_bytes();
    image.extend_from_slice(encoded);
    image.extend_from_slice(b"\nendstream");
    let objects=vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),b"<< /Type /Pages /Count 1 /Kids [3 0 R] >>".to_vec(),b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 151 143] /Resources << /XObject << /I 5 0 R >> >> /Contents 4 0 R >>".to_vec(),format!("<< /Length {} >>\nstream\n{}\nendstream",content.len(),std::str::from_utf8(content).unwrap()).into_bytes(),image];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out.into()
}

#[test]
fn jpeg_and_jpx_tiles_are_independent_of_zoom_history() {
    let jpeg = include_bytes!("../../../tests/capi/fixtures/image-drawn-size/checker-2048-420.jpg");
    let jpx =
        include_bytes!("../../../tests/capi/fixtures/image-drawn-size/checker-2048-lossless.jp2");
    let mut checked = 0;
    let mut failed = 0;
    for (bytes, filter) in [
        (jpeg.as_slice(), "DCTDecode"),
        (jpx.as_slice(), "JPXDecode"),
    ] {
        let bytes = image_fixture(bytes, filter);
        for scale in [0.5, 1., 2.] {
            let (n, f) = gate(bytes.clone(), 0, scale, Rotation::None, 63, filter, false);
            checked += n;
            failed += f;
        }
    }
    assert_eq!(
        failed, 0,
        "{failed}/{checked} image region comparisons failed"
    );
}
