// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Coarse demo edits run on an isolated snapshot, then replace reader/render state.
use crate::view::{self, Loaded};
use pdfoxium::{Bitmap, Document, Matrix, PixelFormat, Point, RenderConfig, Rotation};
use std::{
    path::PathBuf,
    sync::{mpsc, Arc},
};
pub enum Input {
    Path(PathBuf),
    #[cfg(target_arch = "wasm32")]
    Bytes(Arc<[u8]>),
}
impl Input {
    fn document(self) -> pdfoxium::Result<Document> {
        match self {
            Self::Path(path) => Document::open(path),
            #[cfg(target_arch = "wasm32")]
            Self::Bytes(bytes) => Document::from_bytes(bytes),
        }
    }
}
pub enum Operation {
    Rotate,
    Delete,
    Import {
        source: Input,
        pages: Option<Vec<usize>>,
    },
    Image {
        source: Input,
        rect: eframe::egui::Rect,
    },
}
fn png_bitmap(source: Input) -> Result<Bitmap, String> {
    match source {
        Input::Path(path) => decode_png(std::io::BufReader::new(
            std::fs::File::open(path).map_err(|e| e.to_string())?,
        )),
        #[cfg(target_arch = "wasm32")]
        Input::Bytes(bytes) => decode_png(std::io::Cursor::new(bytes)),
    }
}
fn decode_png(reader: impl std::io::BufRead + std::io::Seek) -> Result<Bitmap, String> {
    let mut decoder = png::Decoder::new(reader);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    if u64::from(reader.info().width) * u64::from(reader.info().height) > 128 * 1024 * 1024 / 4 {
        return Err("image exceeds demo size limit".into());
    }
    let size = reader
        .output_buffer_size()
        .filter(|n| *n <= 128 * 1024 * 1024)
        .ok_or("image exceeds demo size limit")?;
    let mut data = vec![0; size];
    let info = reader.next_frame(&mut data).map_err(|e| e.to_string())?;
    let mut bitmap =
        Bitmap::new(info.width, info.height, PixelFormat::Rgba8).map_err(|e| e.to_string())?;
    let channels = info.color_type.samples();
    for (source, target) in data[..info.buffer_size()]
        .chunks_exact(channels)
        .zip(bitmap.pixels_mut().as_chunks_mut::<4>().0.iter_mut())
    {
        target.copy_from_slice(&match info.color_type {
            png::ColorType::Rgba => [source[0], source[1], source[2], source[3]],
            png::ColorType::Rgb => [source[0], source[1], source[2], 255],
            png::ColorType::Grayscale => [source[0], source[0], source[0], 255],
            png::ColorType::GrayscaleAlpha => [source[0], source[0], source[0], source[1]],
            _ => return Err("unsupported PNG color type".into()),
        });
    }
    Ok(bitmap)
}
fn apply(doc: &Document, page: usize, operation: Operation) -> Result<usize, String> {
    let fail = |e: pdfoxium::Error| e.to_string();
    let handle = doc.page(page).map_err(fail)?;
    match operation {
        Operation::Rotate => {
            let old = doc.page_layout(page).map_err(fail)?.rotation;
            let rotation = match old {
                Rotation::None => Rotation::Cw90,
                Rotation::Cw90 => Rotation::Cw180,
                Rotation::Cw180 => Rotation::Cw270,
                Rotation::Cw270 => Rotation::None,
            };
            handle.set_rotation(rotation).map_err(fail)?;
            Ok(page)
        }
        Operation::Delete => {
            if doc.page_count().map_err(fail)? <= 1 {
                return Err("cannot delete the last page".into());
            }
            doc.delete_page(handle.id()).map_err(fail)?;
            Ok(page.min(doc.page_count().map_err(fail)? - 1))
        }
        Operation::Import { source, pages } => {
            let source = source.document().map_err(fail)?;
            if matches!(
                source.form_type().map_err(fail)?,
                pdfoxium::FormType::XfaFull | pdfoxium::FormType::XfaForeground
            ) {
                return Err("importing XFA pages is not supported".into());
            }
            let pages = pages.unwrap_or((0..source.page_count().map_err(fail)?).collect());
            if pages.is_empty() {
                return Err("select at least one page to import".into());
            }
            for &index in &pages {
                if source
                    .page(index)
                    .map_err(fail)?
                    .annotations()
                    .map_err(fail)?
                    .iter()
                    .any(|a| a.subtype == b"Widget")
                {
                    return Err(
                        "page import currently supports pages without interactive widgets".into(),
                    );
                }
            }
            doc.import_pages(&source, &pages, page + 1).map_err(fail)?;
            Ok(page + 1)
        }
        Operation::Image { source, rect } => {
            if !rect.is_finite()
                || !rect.is_positive()
                || rect.min.x < 0.
                || rect.min.y < 0.
                || rect.max.x > 1.
                || rect.max.y > 1.
            {
                return Err("image bounds must be finite and inside the page".into());
            }
            let inverse = crate::text::inverse(
                handle
                    .render_matrix(&RenderConfig::fit(1, 1))
                    .map_err(fail)?,
            )?;
            let point = |p: eframe::egui::Pos2| inverse.transform(Point { x: p.x, y: p.y });
            let a = point(rect.left_bottom());
            let b = point(rect.right_bottom());
            let c = point(rect.left_top());
            let bitmap = png_bitmap(source)?;
            let ux = b.x - a.x;
            let uy = b.y - a.y;
            let vx = c.x - a.x;
            let vy = c.y - a.y;
            let width = ux.hypot(uy);
            let height = vx.hypot(vy);
            let scale = (width / bitmap.width() as f32).min(height / bitmap.height() as f32);
            let xscale = bitmap.width() as f32 * scale / width;
            let yscale = bitmap.height() as f32 * scale / height;
            let matrix = Matrix {
                a: ux * xscale,
                b: uy * xscale,
                c: vx * yscale,
                d: vy * yscale,
                e: a.x + ux * (1. - xscale) / 2. + vx * (1. - yscale) / 2.,
                f: a.y + uy * (1. - xscale) / 2. + vy * (1. - yscale) / 2.,
            };
            handle.add_image(bitmap, matrix).map_err(fail)?;
            Ok(page)
        }
    }
}
pub fn start(
    bytes: Arc<[u8]>,
    snapshot: Option<pdfoxium::thread::Ticket<Vec<u8>>>,
    path: PathBuf,
    page: usize,
    operation: Operation,
    ctx: eframe::egui::Context,
) -> mpsc::Receiver<Result<(Loaded, usize), String>> {
    let (tx, rx) = mpsc::channel();
    let ctx = crate::platform::repaint(&ctx);
    crate::platform::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let bytes = if let Some(snapshot) = snapshot {
                Arc::<[u8]>::from(snapshot.wait().map_err(|e| e.to_string())?)
            } else {
                bytes
            };
            let doc = Document::from_bytes(bytes).map_err(|e| e.to_string())?;
            if matches!(
                doc.form_type().map_err(|e| e.to_string())?,
                pdfoxium::FormType::XfaFull | pdfoxium::FormType::XfaForeground
            ) {
                return Err("PDF page/content editing is not supported for XFA documents".into());
            }
            let page = apply(&doc, page, operation)?;
            let mut result = Vec::new();
            doc.save_to(&mut result).map_err(|e| e.to_string())?;
            let loaded = view::loaded_from_bytes(path, result.into(), ctx.clone(), true)?;
            Ok((loaded, page))
        }))
        .unwrap_or_else(|_| {
            Err("isolated PDF editing failed; the current document is unchanged".into())
        });
        let _ = tx.send(result);
        ctx.request_repaint();
    });
    rx
}
