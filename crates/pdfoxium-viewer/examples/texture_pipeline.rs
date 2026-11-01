// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
//! Matched old/new CPU conversion plus standard egui-wgpu texture upload.
#![forbid(unsafe_code)]
#[path = "../src/texture.rs"]
mod texture;
use eframe::{
    egui::{self, ColorImage},
    egui_wgpu,
};
use pdfoxium::{Bitmap, DeviceRect, Document, PixelFormat, RenderConfig};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn old_images(bitmap: Bitmap, sources: &[DeviceRect]) -> Vec<ColorImage> {
    sources
        .iter()
        .map(|r| {
            let mut bytes = Vec::with_capacity(r.width as usize * r.height as usize * 4);
            for y in r.y..r.y + r.height {
                let start = y as usize * bitmap.stride() + r.x as usize * 4;
                bytes.extend_from_slice(&bitmap.pixels()[start..start + r.width as usize * 4]);
            }
            ColorImage::from_rgba_unmultiplied([r.width as usize, r.height as usize], &bytes)
        })
        .collect()
}
fn new_images(bitmap: Bitmap, sources: &[DeviceRect]) -> Vec<ColorImage> {
    let converter = texture::Converter::new(&bitmap);
    sources
        .iter()
        .map(|source| converter.image(*source))
        .collect()
}
fn copy_input(bitmap: &Bitmap) -> Bitmap {
    let mut copy = Bitmap::new(bitmap.width(), bitmap.height(), PixelFormat::Rgba8).unwrap();
    copy.pixels_mut().copy_from_slice(bitmap.pixels());
    copy
}
fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}
fn median(values: &[f64]) -> f64 {
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    (v[(v.len() - 1) / 2] + v[v.len() / 2]) * 0.5
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    pollster::block_on(run())
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(std::env::args().nth(1).ok_or(
        "usage: cargo run --release -p pdfoxium-viewer --example texture_pipeline -- REPORT.json",
    )?);
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await?;
    let mut renderer = egui_wgpu::Renderer::new(
        &device,
        wgpu::TextureFormat::Bgra8Unorm,
        egui_wgpu::RendererOptions::default(),
    );
    let mut records = Vec::new();
    let mut next_id = 100;
    let cases = [
        (
            "text",
            "tests/render_corpus/books/hamiltonian.pdf",
            0,
            false,
        ),
        (
            "large_text",
            "tests/render_corpus/books/hamiltonian.pdf",
            0,
            false,
        ),
        (
            "image",
            "tests/render_corpus/books/openstax-biology-2e.pdf",
            170,
            false,
        ),
        (
            "pattern",
            "tests/render_corpus/pdfs/pattern_fill.pdf",
            0,
            false,
        ),
        (
            "transparent_text",
            "tests/render_corpus/books/hamiltonian.pdf",
            0,
            true,
        ),
    ];
    for (name, file, page_index, transparent) in cases {
        if !root.join(file).exists() {
            return Err(format!("missing fixture: {file}").into());
        }
        let document = Document::open(root.join(file))?;
        let page = document.page(page_index)?;
        let mut base = if name == "large_text" {
            RenderConfig::fit(4800, 6400)
        } else {
            RenderConfig::fit(2400, 3200)
        }
        .format(PixelFormat::Rgba8);
        if transparent {
            base = base.background(None);
        }
        let full = page.render_size(&base)?;
        for tiled in [false, true] {
            let config = if tiled {
                let width = full.0.min(1536);
                let height = full.1.min(1536);
                base.clone().device_clip(DeviceRect::new(
                    (full.0 - width).min(512),
                    (full.1 - height).min(512),
                    width,
                    height,
                ))
            } else {
                base.clone()
            };
            let bitmap = page.render(&config)?;
            drop(page.render(&config)?);
            let mut render = Vec::new();
            for _ in 0..7 {
                let start = Instant::now();
                let image = page.render(&config)?;
                render.push(ms(start));
                std::hint::black_box(image);
            }
            let mut sources = Vec::new();
            if tiled {
                for y in (0..bitmap.height()).step_by(512) {
                    for x in (0..bitmap.width()).step_by(512) {
                        sources.push(DeviceRect::new(
                            x,
                            y,
                            512.min(bitmap.width() - x),
                            512.min(bitmap.height() - y),
                        ));
                    }
                }
            } else {
                sources.push(DeviceRect::new(0, 0, bitmap.width(), bitmap.height()));
            }
            assert!(sources
                .iter()
                .all(|r| r.width <= device.limits().max_texture_dimension_2d
                    && r.height <= device.limits().max_texture_dimension_2d));
            assert_eq!(
                old_images(copy_input(&bitmap), &sources),
                new_images(copy_input(&bitmap), &sources)
            );
            let mut old = Vec::new();
            let mut new = Vec::new();
            for iteration in 0..18 {
                // Alternate ordering; two initial iterations warm allocator/driver paths.
                let order = if iteration % 2 == 0 {
                    [false, true]
                } else {
                    [true, false]
                };
                for optimized in order {
                    // Input copy simulates ownership of the finished render, and is
                    // excluded from BOTH conversion timings. Rendering is above.
                    let input = copy_input(&bitmap);
                    let start = Instant::now();
                    let images = if optimized {
                        new_images(input, &sources)
                    } else {
                        old_images(input, &sources)
                    };
                    let conversion = ms(start);
                    let start = Instant::now();
                    let mut ids = Vec::new();
                    for image in images {
                        let id = egui::TextureId::Managed(next_id);
                        next_id += 1;
                        let delta =
                            egui::epaint::ImageDelta::full(image, egui::TextureOptions::LINEAR);
                        renderer.update_texture(&device, &queue, id, &delta);
                        ids.push(id);
                    }
                    let enqueue = ms(start);
                    let start = Instant::now();
                    let submission = queue.submit([]);
                    device.poll(wgpu::PollType::Wait {
                        submission_index: Some(submission),
                        timeout: Some(Duration::from_secs(10)),
                    })?;
                    let submit_wait = ms(start);
                    for id in ids {
                        renderer.free_texture(&id);
                    }
                    if iteration >= 2 {
                        let record = json!({"conversion_ms":conversion,"backend_enqueue_ms":enqueue,"submit_wait_ms":submit_wait,"upload_wall_ms":enqueue+submit_wait});
                        if optimized {
                            new.push(record);
                        } else {
                            old.push(record);
                        }
                    }
                }
            }
            let summary = |samples: &[Value]| {
                let collect = |key: &str| {
                    samples
                        .iter()
                        .map(|v| v[key].as_f64().unwrap())
                        .collect::<Vec<_>>()
                };
                json!({"conversion_ms":median(&collect("conversion_ms")),"backend_enqueue_ms":median(&collect("backend_enqueue_ms")),"submit_wait_ms":median(&collect("submit_wait_ms")),"upload_wall_ms":median(&collect("upload_wall_ms"))})
            };
            let record = json!({"name":name,"source":file,"page_index":page_index,"mode":if tiled {"merged_tiles"} else {"whole"},"pixels":[bitmap.width(),bitmap.height()],"textures":sources.len(),"large_output_allocations":{"old":2*sources.len(),"new":sources.len()},"pixel_materialization_bytes":{"old":u64::from(bitmap.width())*u64::from(bitmap.height())*8,"new":u64::from(bitmap.width())*u64::from(bitmap.height())*4},"render_ms":median(&render),"render_samples_ms":render,"old":summary(&old),"new":summary(&new),"old_samples":old,"new_samples":new});
            println!(
                "{name} {}: render {:.3}ms, CPU {:.3} -> {:.3}ms, upload {:.3} -> {:.3}ms",
                if tiled { "tiles" } else { "whole" },
                record["render_ms"].as_f64().unwrap(),
                record["old"]["conversion_ms"].as_f64().unwrap(),
                record["new"]["conversion_ms"].as_f64().unwrap(),
                record["old"]["upload_wall_ms"].as_f64().unwrap(),
                record["new"]["upload_wall_ms"].as_f64().unwrap()
            );
            records.push(record);
        }
    }
    let report = json!({"adapter":format!("{:?}",adapter.get_info()),"max_texture_side":device.limits().max_texture_dimension_2d,"os":std::env::consts::OS,"arch":std::env::consts::ARCH,"samples_per_variant":16,"notes":"Matched old/new conversion of identical RGBA inputs, input cloning excluded; warm render measured independently. Upload uses unmodified egui-wgpu Renderer::update_texture. Backend enqueue is CPU texture allocation/staging; submit_wait is CPU+GPU completion wall time, not a GPU timestamp or presentation/frame timing.","cases":records});
    std::fs::write(output, serde_json::to_vec_pretty(&report)?)?;
    Ok(())
}
