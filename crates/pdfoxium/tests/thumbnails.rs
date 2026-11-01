// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use pdfoxium::{Document, Error, PixelFormat, RenderConfig};
fn fixture(kind: u8) -> Vec<u8> {
    let thumb = if kind == 0 { "" } else { "/Thumb 5 0 R" };
    let samples = if kind == 2 {
        ""
    } else {
        "ff000000ff000000ffffffff>"
    };
    let objects=[
        "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Resources <<>> /Contents 4 0 R {thumb} >>"),
        "<< /Length 0 >>\nstream\n\nendstream".into(),
        format!("<< /Type /XObject /Subtype /Image /Width 2 /Height 2 /BitsPerComponent 8 /ColorSpace /DeviceRGB /Filter /ASCIIHexDecode /Length {} >>\nstream\n{samples}\nendstream",samples.len()),
    ];
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (index, obj) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n{obj}\nendobj\n", index + 1).as_bytes());
    }
    let xref = pdf.len();
    pdf.extend_from_slice(
        format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes(),
    );
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    pdf
}
#[test]
fn owned_embedded_thumbnail_pixels_formats_and_read_only_save_round_trip() {
    for kind in [0, 1, 2] {
        let doc = Document::from_bytes(fixture(kind)).unwrap();
        let page = doc.page(0).unwrap();
        for format in [PixelFormat::Bgra8, PixelFormat::Rgba8] {
            let bitmap = page.embedded_thumbnail(format).unwrap();
            if kind != 1 {
                assert!(bitmap.is_none());
                continue;
            }
            let bitmap = bitmap.unwrap();
            assert_eq!((bitmap.width(), bitmap.height()), (2, 2));
            let expected = if format == PixelFormat::Rgba8 {
                vec![
                    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
                ]
            } else {
                vec![
                    0, 0, 255, 255, 0, 255, 0, 255, 255, 0, 0, 255, 255, 255, 255, 255,
                ]
            };
            assert_eq!(bitmap.pixels(), expected);
            page.unload().unwrap();
            assert_eq!(bitmap.pixels(), expected); // detached owned pixels survive unloading
        }
        let job = page.render_job(&RenderConfig::default()).unwrap();
        assert!(matches!(
            page.embedded_thumbnail(PixelFormat::Rgba8),
            Err(Error::Busy)
        ));
        drop(job);
        let mut saved = Vec::new();
        doc.save_to(&mut saved).unwrap();
        let reopened = Document::from_bytes(saved).unwrap();
        assert_eq!(
            reopened
                .page(0)
                .unwrap()
                .embedded_thumbnail(PixelFormat::Rgba8)
                .unwrap()
                .is_some(),
            kind == 1
        );
    }
}
