// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Seed the generated viewer demo with real editable PDF annotations and appearances.
use pdfoxium::{AnnotationSpec, Color, Document, Rect, SearchOptions};
fn rect(left: f32, bottom: f32, right: f32, top: f32) -> Rect {
    Rect {
        left,
        bottom,
        right,
        top,
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("expected generated editing-demo.pdf path")?;
    // Keep the source in memory so writing the finished fixture cannot truncate
    // a stream still being read by its document.
    let document = Document::from_bytes(std::fs::read(&path)?)?;
    let page = document.page(0)?;
    if !page.annotations()?.is_empty() {
        return Err("regenerate the unannotated base before seeding it".into());
    }
    for (index, query) in [
        "This sentence already has a yellow highlight.",
        "Underline marks important text.",
        "Strike out text that should be removed.",
    ]
    .into_iter()
    .enumerate()
    {
        let bounds = {
            let text = page.text()?;
            let hit = text
                .search_with_rects(query, SearchOptions::default())?
                .next()
                .ok_or("demo text was not found")??;
            // Each example occupies one line; keep a continuous markup band.
            hit.rects
                .into_iter()
                .reduce(|a, b| {
                    rect(
                        a.left.min(b.left),
                        a.bottom.min(b.bottom),
                        a.right.max(b.right),
                        a.top.max(b.top),
                    )
                })
                .ok_or("demo text has no geometry")?
        };
        let spec = match index {
            0 => AnnotationSpec::highlight(vec![bounds])
                .color(Color::rgba(255, 220, 40, 100))
                .contents("An existing highlight. Select it to edit its comment."),
            1 => AnnotationSpec::underline(vec![bounds])
                .color(Color::rgb(20, 90, 180))
                .border_width(1.2)
                .contents("A continuous underline annotation."),
            _ => AnnotationSpec::strikeout(vec![bounds])
                .color(Color::rgb(190, 55, 45))
                .border_width(1.2)
                .contents("A strikeout annotation marking text for removal."),
        };
        page.add_annotation(spec)?;
    }
    for spec in [
        AnnotationSpec::text_note(
            rect(520., 520., 540., 540.),
            "A review note. You can edit this comment or move the note.",
        ).color(Color::rgb(255, 195, 40)),
        AnnotationSpec::free_text(
            rect(65., 260., 315., 390.),
            "This is an editable text annotation.\n\nChange this text, resize its box, or move it using the grip.",
            13.,
        ).color(Color::rgb(32, 49, 62)),
        AnnotationSpec::square(rect(345., 330., 535., 395.))
            .color(Color::rgb(20, 108, 148)).border_width(2.)
            .contents("Drag or resize this rectangle."),
        AnnotationSpec::circle(rect(380., 220., 500., 300.))
            .color(Color::rgb(210, 95, 25)).border_width(2.)
            .contents("Drag or resize this ellipse."),
    ] {
        page.add_annotation(spec)?;
    }
    let mut bytes = Vec::new();
    document.save_to(&mut bytes)?;
    std::fs::write(path, bytes)?;
    Ok(())
}
