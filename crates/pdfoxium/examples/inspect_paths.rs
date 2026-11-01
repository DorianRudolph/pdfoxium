// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Walk page objects, including nested forms, and recover exact path segments.
use pdfoxium::{Document, ObjectKind};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let input = std::env::args_os()
        .nth(1)
        .ok_or("usage: inspect_paths input.pdf")?;
    let document = Document::open(input)?;
    for page in document.pages()? {
        let page = page?;
        let mut pending = page.object_handles()?;
        while let Some(object) = pending.pop() {
            match object.kind()? {
                ObjectKind::Path => {
                    let path = object.path()?;
                    println!(
                        "page {}: {:?}, matrix {:?}",
                        page.index(),
                        path.segments,
                        path.matrix
                    );
                }
                ObjectKind::Form => pending.extend(object.children()?),
                _ => {}
            }
        }
        page.unload()?;
    }
    Ok(())
}
