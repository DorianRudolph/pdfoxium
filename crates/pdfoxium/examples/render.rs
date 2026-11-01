// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by the BSD-style license in LICENSE.
//! Minimal synchronous rendering. The full CLI also supports parallel jobs.
//! cargo run -p pdfoxium --example render -- input.pdf output-directory
use pdfoxium::{Document, RenderConfig};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let input = args
        .next()
        .ok_or("usage: render INPUT.pdf [OUTPUT_DIRECTORY]")?;
    let output = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(|| "rendered".into());
    if args.next().is_some() {
        return Err("usage: render INPUT.pdf [OUTPUT_DIRECTORY]".into());
    }
    std::fs::create_dir_all(&output)?;
    let document = Document::open(input)?;
    let config = RenderConfig::dpi(144.0);
    for index in 0..document.page_count()? {
        let page = document.page(index)?;
        page.render(&config)?
            .write_png(output.join(format!("page-{:04}.png", index + 1)))?;
        page.unload()?;
    }
    Ok(())
}
