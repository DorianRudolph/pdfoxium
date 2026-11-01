// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Whole-page worker isolation: only input bytes and rendered results cross
//! threads; each worker opens and retains its own document and font state.

use pdfoxium::{Config, Document, FontMemoryLimits, ImageMemoryLimits, RenderConfig};
use std::sync::{Arc, Barrier};

#[test]
fn shared_input_with_independent_worker_documents_matches_serial_pixels() {
    let sources: Vec<Arc<[u8]>> = [
        "rectangles_multi_pages.pdf",
        "embedded_truetype.pdf",
        "hello_world.pdf",
        "jpeg_rgb.pdf",
        "image_clipped.pdf",
    ]
    .into_iter()
    .map(|name| {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/render_corpus/pdfs")
                .join(name),
        )
        .unwrap()
        .into()
    })
    .collect();
    let render = RenderConfig::fit(192, 192);
    let expected: Vec<Vec<_>> = sources
        .iter()
        .map(|source| {
            let document = Document::from_bytes(source.clone()).unwrap();
            document
                .pages()
                .unwrap()
                .map(|page| {
                    let bitmap = page.unwrap().render(&render).unwrap();
                    assert!(bitmap
                        .pixels()
                        .chunks_exact(4)
                        .any(|pixel| pixel != [255; 4]));
                    bitmap
                })
                .collect()
        })
        .collect();

    // The barrier makes workers enter document opening/rendering together.
    // There are no later barriers, so a failed assertion cannot strand peers.
    let start = Barrier::new(4);
    std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4)
            .map(|worker| {
                let (sources, expected, render, start) = (&sources, &expected, &render, &start);
                scope.spawn(move || {
                    Config {
                        // Different worker policies also exercise TLS isolation.
                        font_memory: if worker % 2 == 0 {
                            FontMemoryLimits {
                                glyph_cache_bytes: 0,
                                glyph_scratch_bytes: 0,
                            }
                        } else {
                            FontMemoryLimits::default()
                        },
                        image_memory: if worker % 2 == 0 {
                            ImageMemoryLimits {
                                cached_image_bytes: 0,
                            }
                        } else {
                            ImageMemoryLimits::UNLIMITED
                        },
                        ..Config::default()
                    }
                    .install()
                    .unwrap();
                    start.wait();
                    for (source, expected) in sources.iter().zip(expected) {
                        let document = Document::from_bytes(source.clone()).unwrap();
                        assert_eq!(document.page_count().unwrap(), expected.len());
                        for pass in 0..3 {
                            for (index, expected) in expected.iter().enumerate() {
                                let page = document.page(index).unwrap();
                                let actual = page.render(render).unwrap();
                                assert_eq!(
                                    (actual.width(), actual.height(), actual.stride()),
                                    (expected.width(), expected.height(), expected.stride()),
                                );
                                assert_eq!(
                                    actual.pixels(),
                                    expected.pixels(),
                                    "worker {worker}, pass {pass}, page {index}"
                                );
                                if pass != 0 {
                                    // First repeat exercises warm caches, final
                                    // repeat reloads pages after explicit unload.
                                    page.unload().unwrap();
                                }
                            }
                        }
                    }
                })
            })
            .collect();
        for worker in workers {
            worker.join().unwrap();
        }
    });
    // Worker documents have released every input owner at thread teardown.
    assert!(sources.iter().all(|source| Arc::strong_count(source) == 1));
}

#[test]
fn render_pools_match_serial_whole_regions_rotations_and_warm_caches() {
    use pdfoxium::thread::{PoolOptions, Priority, RenderPool, Source};
    use pdfoxium::{DeviceRect, PixelFormat, Rotation};
    let root =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/render_corpus/pdfs");
    let mut compared = 0;
    for name in [
        "rectangles_multi_pages.pdf",
        "embedded_truetype.pdf",
        "hello_world.pdf",
        "jpeg_rgb.pdf",
        "image_clipped.pdf",
        "pattern_fill.pdf",
        "type3.pdf",
        "stamp_appearance.pdf",
    ] {
        let bytes: Arc<[u8]> = std::fs::read(root.join(name)).unwrap().into();
        {
            let serial = Document::from_bytes(bytes.clone()).unwrap();
            // Separate pools render the same pages concurrently; each pool also
            // exercises page affinity across two independent worker documents.
            let pools: Vec<_> = (0..4)
                .map(|worker| {
                    RenderPool::with_options(
                        Source::Bytes(bytes.clone()),
                        Config::default(),
                        PoolOptions {
                            threads: 2,
                            max_paused_jobs: if worker % 2 == 0 { 0 } else { 2 },
                            max_cached_pages: if worker % 2 == 0 { 1 } else { 8 },
                            ..PoolOptions::default()
                        },
                    )
                    .unwrap()
                })
                .collect();
            for pass in 0..2 {
                for variant in 0..4 {
                    let mut expected = Vec::new();
                    let mut tickets = Vec::new();
                    for index in 0..serial.page_count().unwrap() {
                        let page = serial.page(index).unwrap();
                        let mut config = RenderConfig::fit(192, 192);
                        match variant {
                            1 => config = config.rotation(Rotation::Cw90),
                            2 => {
                                let full = page.render(&config).unwrap();
                                let (w, h) = (full.width(), full.height());
                                let ink = full
                                    .pixels()
                                    .chunks_exact(4)
                                    .position(|p| p[..3].iter().any(|&channel| channel < 240))
                                    .expect("full-page fixture must have visible content");
                                let (rw, rh) = ((w / 2).max(1), (h / 2).max(1));
                                let x = (ink as u32 % w).saturating_sub(rw / 2).min(w - rw);
                                let y = (ink as u32 / w).saturating_sub(rh / 2).min(h - rh);
                                config = config.device_clip(DeviceRect::new(x, y, rw, rh));
                            }
                            3 => config = config.format(PixelFormat::Rgba8).background(None),
                            _ => {}
                        }
                        let reference = page.render(&config).unwrap();
                        assert!(
                            reference
                                .pixels()
                                .chunks_exact(4)
                                .any(|p| p != [255; 4] && p != [0; 4]),
                            "blank {name}"
                        );
                        expected.push(reference);
                        for pool in &pools {
                            tickets.push((
                                index,
                                pool.render(index, config.clone(), Priority::Visible),
                            ));
                        }
                    }
                    for (index, ticket) in tickets {
                        let actual = ticket.wait().unwrap();
                        assert_eq!(actual.source.page_index, index);
                        assert_eq!(actual.source.generation, 0);
                        let expected = &expected[index];
                        assert_eq!(
                            (
                                actual.bitmap.width(),
                                actual.bitmap.height(),
                                actual.bitmap.format()
                            ),
                            (expected.width(), expected.height(), expected.format())
                        );
                        assert_eq!(
                            actual.bitmap.pixels(),
                            expected.pixels(),
                            "{name}, page={index}, pass={pass}, variant={variant}"
                        );
                        compared += 1;
                    }
                }
            }
        }
        assert_eq!(
            Arc::strong_count(&bytes),
            1,
            "workers retained {name} after shutdown"
        );
    }
    eprintln!("POOL_PIXEL_COMPARISONS={compared}");
}
