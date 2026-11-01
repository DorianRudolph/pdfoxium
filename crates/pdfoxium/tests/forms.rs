// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#[path = "support/forms.rs"]
#[allow(dead_code)]
mod support;
use pdfoxium::*;
fn click(form: &FormSession, x: f32, y: f32) {
    form.input(0, FormInput::MouseMove(Point { x, y }, Default::default()))
        .unwrap();
    form.input(0, FormInput::MouseDown(Point { x, y }, Default::default()))
        .unwrap();
    form.input(0, FormInput::MouseUp(Point { x, y }, Default::default()))
        .unwrap();
}
#[test]
fn fields_focus_typing_checkbox_tab_drawing_save_and_ownership() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let page = doc.page(0).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    assert!(matches!(
        doc.forms(FormOptions::default()),
        Err(Error::Busy)
    ));
    assert_eq!(form.fields(0).unwrap().len(), 5);
    let other = Document::from_bytes(support::fixture()).unwrap();
    assert!(matches!(
        doc.import_pages(&other, &[0], 0),
        Err(Error::Busy)
    ));
    assert!(matches!(
        other.import_pages(&doc, &[0], 0),
        Err(Error::Busy)
    ));
    assert!(matches!(
        page.set_rotation(Rotation::Cw90),
        Err(Error::Busy)
    ));
    assert!(matches!(
        form.input(
            0,
            FormInput::MouseMove(
                Point {
                    x: f32::NAN,
                    y: 0.0
                },
                Default::default()
            )
        ),
        Err(Error::InvalidArgument(_))
    ));
    let before = form
        .render(0, &RenderConfig::scale(2.0).format(PixelFormat::Rgba8))
        .unwrap();
    click(&form, 50.0, 265.0);
    assert_eq!(form.state().unwrap().focus.unwrap().name, "name");
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(0, FormInput::Text("Edited".into(), Default::default()))
        .unwrap();
    assert_eq!(form.state().unwrap().text, "Edited");
    page.unload().unwrap();
    assert_eq!(form.state().unwrap().text, "Edited");
    assert!(form.state().unwrap().dirty);
    let during = form
        .render(0, &RenderConfig::scale(2.0).format(PixelFormat::Rgba8))
        .unwrap();
    assert_ne!(before.pixels(), during.pixels());
    form.input(0, FormInput::KeyDown(FormKey::Tab, Default::default()))
        .unwrap();
    assert_eq!(form.state().unwrap().focus.unwrap().name, "second");
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Edited"
    );
    click(&form, 35.0, 165.0);
    assert!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "check")
            .unwrap()
            .checked
    );
    let checked = form
        .render(0, &RenderConfig::scale(2.0).format(PixelFormat::Rgba8))
        .unwrap();
    let at = (270 * checked.stride()) + 70 * 4;
    assert!(checked.pixels()[at] < 50);
    let job = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(form.input(0, FormInput::Blur), Err(Error::Busy)));
    drop(job);
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let newform = reopened.forms(FormOptions::default()).unwrap();
    assert_eq!(
        newform
            .fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Edited"
    );
    assert!(
        newform
            .fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "check")
            .unwrap()
            .checked
    );
    drop(form);
    let form = doc.forms(FormOptions::default()).unwrap();
    drop(doc);
    assert!(form.state().is_ok());
}
fn take<T>(mut ticket: pdfoxium::thread::Ticket<T>) -> pdfoxium::Result<T> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if let Some(result) = ticket.try_take() {
            return result;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "document thread stalled"
        );
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
}
#[test]
fn document_thread_orders_input_restarts_rendering_and_retains_accepted_save_on_drop() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(support::fixture().into()),
        Config::default(),
        DocOptions::default(),
    )
    .unwrap();
    let canceled = thread.render(0, RenderConfig::scale(4.0), Priority::Thumbnail);
    canceled.cancel();
    assert!(matches!(take(canceled), Err(Error::Cancelled)));
    for input in [
        FormInput::MouseMove(Point { x: 50.0, y: 265.0 }, Default::default()),
        FormInput::MouseDown(Point { x: 50.0, y: 265.0 }, Default::default()),
        FormInput::MouseUp(Point { x: 50.0, y: 265.0 }, Default::default()),
        FormInput::SelectAll,
    ] {
        take(thread.input(0, input)).unwrap();
    }
    drop(thread.input(
        0,
        FormInput::Text("Thread value".into(), Default::default()),
    ));
    assert_eq!(
        take(thread.call(|_, form| Ok(form.state()?.text))).unwrap(),
        "Thread value"
    );
    let before = thread.preview_generation();
    let output = take(thread.render(0, RenderConfig::scale(2.0), Priority::Visible)).unwrap();
    assert_eq!((output.bitmap.width(), output.bitmap.height()), (400, 600));
    assert!(thread.generation() >= before);
    let path = std::env::temp_dir().join(format!(
        "pdfoxium-form-thread-{}-{}.pdf",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    drop(thread.save(path.clone()));
    drop(thread);
    let reopened = Document::open(&path).unwrap();
    let form = reopened.forms(FormOptions::default()).unwrap();
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Thread value"
    );
    drop(form);
    drop(reopened);
    std::fs::remove_file(path).unwrap();
}
#[test]
fn document_thread_reports_poison_after_callback_panics() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(support::fixture().into()),
        Config::default(),
        DocOptions::default(),
    )
    .unwrap();
    let result = take(thread.call::<()>(|_, _| panic!("intentional callback panic")));
    assert!(matches!(result, Err(Error::Poisoned)));
    assert!(matches!(
        take(thread.call(|doc, _| doc.page_count())),
        Err(Error::Poisoned)
    ));
    assert!(matches!(
        take(thread.render(0, RenderConfig::default(), Priority::Visible)),
        Err(Error::Poisoned)
    ));
    assert!(thread
        .take_events()
        .iter()
        .any(|e| matches!(e, DocEvent::Failed(_))));
}

#[test]
fn caret_timers_and_rotated_region_overlay_match_full_render() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    click(&form, 50.0, 265.0);
    let deadline = form.next_timer_deadline().expect("focused caret timer");
    form.take_events();
    let events = form.run_due_timers(deadline).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e, FormEvent::Invalidate { .. })));
    assert!(form.next_timer_deadline().unwrap() > deadline);
    form.input(0, FormInput::Blur).unwrap();
    for rotation in [
        Rotation::None,
        Rotation::Cw90,
        Rotation::Cw180,
        Rotation::Cw270,
    ] {
        let config = RenderConfig::scale(2.0)
            .rotation(rotation)
            .format(PixelFormat::Rgba8);
        let full = form.render(0, &config).unwrap();
        let region = DeviceRect::new(12, 18, full.width() - 24, full.height() - 36);
        let tile = form.render(0, &config.device_clip(region)).unwrap();
        let mut max_difference = 0;
        for y in 0..region.height as usize {
            let start = (y + region.y as usize) * full.stride() + region.x as usize * 4;
            let expected = &full.pixels()[start..start + region.width as usize * 4];
            let actual =
                &tile.pixels()[y * tile.stride()..y * tile.stride() + region.width as usize * 4];
            for (a, b) in expected.iter().zip(actual) {
                max_difference = max_difference.max(a.abs_diff(*b));
            }
        }
        assert!(max_difference <= 1, "{rotation:?}: {max_difference}");
    }
}

#[test]
fn document_thread_save_does_not_overwrite_existing_file() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(support::fixture().into()),
        Config::default(),
        DocOptions::default(),
    )
    .unwrap();
    let path = std::env::temp_dir().join(format!(
        "pdfoxium-form-no-overwrite-{}.pdf",
        std::process::id()
    ));
    std::fs::write(&path, b"existing content").unwrap();
    assert!(matches!(take(thread.save(path.clone())), Err(Error::Io(_))));
    assert_eq!(std::fs::read(&path).unwrap(), b"existing content");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn dropping_session_during_render_defers_focus_commit_until_lease_release() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    click(&form, 50.0, 265.0);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(0, FormInput::Text("Retained".into(), Default::default()))
        .unwrap();
    let revision = doc.revision();
    let mut job = doc
        .page(0)
        .unwrap()
        .render_job(&RenderConfig::scale(2.0))
        .unwrap();
    drop(form);
    assert!(matches!(
        doc.forms(FormOptions::default()),
        Err(Error::Busy)
    ));
    while job.resume(&mut || false).unwrap() != Progress::Done {}
    drop(job.finish().unwrap());
    assert!(doc.revision() > revision);
    assert!(doc.is_dirty());
    let form = doc.forms(FormOptions::default()).unwrap();
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Retained"
    );
}

#[test]
fn highlight_color_and_alpha_preserve_owned_pixel_formats() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let form = doc
        .forms(FormOptions {
            highlight: Some(Color::rgba(0, 0, 255, 64)),
            ..Default::default()
        })
        .unwrap();
    let rgba = form
        .render(0, &RenderConfig::scale(1.0).format(PixelFormat::Rgba8))
        .unwrap();
    let bgra = form
        .render(0, &RenderConfig::scale(1.0).format(PixelFormat::Bgra8))
        .unwrap();
    let at = 135 * rgba.stride() + 35 * 4;
    let color = &rgba.pixels()[at..at + 4];
    assert!(color[0] >= 190 && color[0] <= 192, "{color:?}");
    assert_eq!(color[0], color[1]);
    assert_eq!(&color[2..], &[255, 255]);
    assert_eq!(
        &bgra.pixels()[at..at + 4],
        &[color[2], color[1], color[0], color[3]]
    );
}

/// Focused release measurement, opt in with --ignored --nocapture. Import a
/// real corpus page behind the fields; no core or production fixture changes.
#[test]
#[ignore = "release-only form input latency benchmark"]
fn form_input_latency_while_rendering_heavy_pages() {
    use pdfoxium::thread::*;
    use std::time::{Duration, Instant};
    fn wait<T>(ticket: Ticket<T>) -> pdfoxium::Result<T> {
        ticket.wait()
    }
    let directory = std::env::var_os("PDFOXIUM_FORM_BENCH_FIXTURES").expect("generate fixtures with scripts/bench-form-input-fixtures.py and set PDFOXIUM_FORM_BENCH_FIXTURES");
    for (book, source_page) in [
        ("obp.0526", 332),
        ("hefferon-toc", 100),
        ("openstax-biology-2e", 170),
    ] {
        let bytes: std::sync::Arc<[u8]> = std::fs::read(
            std::path::Path::new(&directory).join(format!("{book}-{source_page}.pdf")),
        )
        .unwrap()
        .into();
        for region in ["whole", "field", "caret"] {
            let dt = DocumentThread::spawn(
                Source::Bytes(bytes.clone()),
                Config::default(),
                DocOptions::default(),
            )
            .unwrap();
            let ink = wait(dt.call(|doc, _| {
                Ok(doc
                    .page(0)?
                    .render(&RenderConfig::fit_width(400))?
                    .pixels()
                    .chunks_exact(4)
                    .filter(|p| p[0] < 200 || p[1] < 200 || p[2] < 200)
                    .count())
            }))
            .unwrap();
            assert!(ink > 500, "blank benchmark page {book} {source_page}");
            let full_config = RenderConfig::fit_width(3200).format(PixelFormat::Rgba8);
            let matrix =
                wait(dt.call(move |doc, _| doc.page(0)?.render_matrix(&full_config))).unwrap();
            let caret = matrix.transform(Point { x: 50., y: 345. });
            let config = match region {
                "caret" => RenderConfig::fit_width(3200)
                    .format(PixelFormat::Rgba8)
                    .device_clip(DeviceRect::new(caret.x as u32, caret.y as u32, 8, 100)),
                "field" => {
                    let points = [(20., 330.), (180., 330.), (20., 360.), (180., 360.)]
                        .map(|(x, y)| matrix.transform(Point { x, y }));
                    let x = points
                        .iter()
                        .map(|p| p.x)
                        .fold(f32::INFINITY, f32::min)
                        .floor() as u32;
                    let y = points
                        .iter()
                        .map(|p| p.y)
                        .fold(f32::INFINITY, f32::min)
                        .floor() as u32;
                    let right = points
                        .iter()
                        .map(|p| p.x)
                        .fold(f32::NEG_INFINITY, f32::max)
                        .ceil() as u32;
                    let bottom = points
                        .iter()
                        .map(|p| p.y)
                        .fold(f32::NEG_INFINITY, f32::max)
                        .ceil() as u32;
                    RenderConfig::fit_width(3200)
                        .format(PixelFormat::Rgba8)
                        .device_clip(DeviceRect::new(x, y, right - x, bottom - y))
                }
                _ => RenderConfig::fit_width(3200).format(PixelFormat::Rgba8),
            };
            // Warm fonts and image decoders before measuring.
            wait(dt.render(0, config.clone(), Priority::Visible)).unwrap();
            let mut render_ms = Vec::new();
            let mut base_ms = Vec::new();
            for _ in 0..7 {
                let full_config = RenderConfig::fit_width(3200).format(PixelFormat::Rgba8);
                base_ms.push(
                    wait(dt.call(move |_, form| {
                        let start = Instant::now();
                        let bitmap = form.render(0, &full_config)?;
                        let elapsed = start.elapsed().as_secs_f64() * 1000.;
                        std::hint::black_box(bitmap);
                        Ok(elapsed)
                    }))
                    .unwrap(),
                );
                let start = Instant::now();
                wait(dt.render(0, config.clone(), Priority::Visible)).unwrap();
                render_ms.push(start.elapsed().as_secs_f64() * 1000.);
            }
            for input in [
                FormInput::MouseMove(Point { x: 50., y: 345. }, Default::default()),
                FormInput::MouseDown(Point { x: 50., y: 345. }, Default::default()),
                FormInput::MouseUp(Point { x: 50., y: 345. }, Default::default()),
                FormInput::SelectAll,
            ] {
                wait(dt.input(0, input)).unwrap();
            }
            let mut idle_ms = Vec::new();
            let mut busy_ms = Vec::new();
            for busy in [false, true] {
                for _ in 0..15 {
                    let render = busy.then(|| dt.render(0, config.clone(), Priority::Visible));
                    if busy {
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    let start = Instant::now();
                    wait(dt.input(0, FormInput::Text("x".into(), Default::default()))).unwrap();
                    let elapsed = start.elapsed().as_secs_f64() * 1000.;
                    if busy {
                        busy_ms.push(elapsed);
                    } else {
                        idle_ms.push(elapsed);
                    }
                    if let Some(render) = render {
                        render.cancel();
                    }
                    wait(dt.call(|_, f| f.state())).unwrap();
                }
            }
            for v in [&mut base_ms, &mut render_ms, &mut idle_ms, &mut busy_ms] {
                v.sort_by(f64::total_cmp);
            }
            println!("{{\"book\":\"{book}\",\"page\":{},\"width\":3200,\"region\":\"{region}\",\"full_render_median_ms\":{},\"warm_request_median_ms\":{},\"idle_input_median_ms\":{},\"busy_input_median_ms\":{},\"busy_input_p95_ms\":{}}}",source_page,base_ms[3],render_ms[3],idle_ms[7],busy_ms[7],busy_ms[14]);
        }
    }
}

#[test]
fn word_selection_multiline_wheel_radio_and_crop_rotation_persist() {
    let doc = Document::from_bytes(support::advanced()).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    assert!(!form.input(0, FormInput::SelectWord).unwrap().handled);
    assert!(matches!(
        form.input(
            0,
            FormInput::MouseWheel(
                Point { x: 230., y: 294. },
                Point { x: 0., y: f32::NAN },
                Default::default()
            )
        ),
        Err(Error::InvalidArgument(_))
    ));
    click(&form, 50., 265.);
    form.input(0, FormInput::SelectWord).unwrap();
    assert_eq!(form.state().unwrap().selected_text, "Initial");
    click(&form, 230., 294.);
    form.input(
        0,
        FormInput::MouseWheel(
            Point { x: 230., y: 294. },
            Point { x: 0., y: -40. },
            Default::default(),
        ),
    )
    .unwrap();
    form.input(0, FormInput::Blur).unwrap();
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "list")
            .unwrap()
            .value,
        "Two"
    );
    let before = form.render(0, &RenderConfig::scale(2.)).unwrap();
    click(&form, 300., 155.);
    let radios: Vec<_> = form
        .fields(0)
        .unwrap()
        .into_iter()
        .filter(|f| f.name == "radio")
        .collect();
    assert!(!radios[0].checked && radios[1].checked);
    let after = form.render(0, &RenderConfig::scale(2.)).unwrap();
    assert_ne!(before.pixels(), after.pixels());
    click(&form, 240., 90.);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(
        0,
        FormInput::Text("First line\rSecond line".into(), Default::default()),
    )
    .unwrap();
    assert_eq!(
        form.state().unwrap().text.lines().collect::<Vec<_>>(),
        vec!["First line", "Second line"]
    );
    form.input(0, FormInput::SelectWord).unwrap();
    assert_eq!(form.state().unwrap().selected_text, "line");
    let mut bytes = Vec::new();
    doc.save_to(&mut bytes).unwrap();
    let reopened = Document::from_bytes(bytes).unwrap();
    let form = reopened.forms(FormOptions::default()).unwrap();
    let fields = form.fields(0).unwrap();
    assert_eq!(
        fields
            .iter()
            .find(|f| f.name == "multiline")
            .unwrap()
            .value
            .lines()
            .collect::<Vec<_>>(),
        vec!["First line", "Second line"]
    );
    assert_eq!(
        fields.iter().find(|f| f.name == "radio").unwrap().value,
        "B"
    );
    assert_eq!(
        fields.iter().find(|f| f.name == "list").unwrap().value,
        "Two"
    );
    for rotation in [
        Rotation::None,
        Rotation::Cw90,
        Rotation::Cw180,
        Rotation::Cw270,
    ] {
        assert_eq!(
            form.render(0, &RenderConfig::scale(2.).rotation(rotation))
                .unwrap()
                .width(),
            760
        );
    }
}

#[test]
fn actual_form_damage_patches_reproduce_full_frame_at_crop_and_rotations() {
    for rotation in [
        Rotation::None,
        Rotation::Cw90,
        Rotation::Cw180,
        Rotation::Cw270,
    ] {
        let doc = Document::from_bytes(support::advanced()).unwrap();
        let form = doc.forms(FormOptions::default()).unwrap();
        let config = RenderConfig::scale(1.7)
            .format(PixelFormat::Rgba8)
            .rotation(rotation);
        let matrix = doc.page(0).unwrap().render_matrix(&config).unwrap();
        let mut frame = form.render(0, &config).unwrap();
        let mut inputs = vec![
            FormInput::MouseMove(Point { x: 50., y: 265. }, Default::default()),
            FormInput::MouseDown(Point { x: 50., y: 265. }, Default::default()),
            FormInput::MouseUp(Point { x: 50., y: 265. }, Default::default()),
            FormInput::SelectAll,
            FormInput::Text("Changed word".into(), Default::default()),
            FormInput::KeyDown(FormKey::Left, Default::default()),
            FormInput::SelectWord,
            FormInput::Blur,
        ];
        // Group updates must repaint both the old and new radio. An expanded
        // choice must survive clipping outside its original widget rectangle.
        for point in [
            Point { x: 300., y: 155. },
            Point { x: 240., y: 155. },
            Point { x: 175., y: 110. },
        ] {
            inputs.extend([
                FormInput::MouseMove(point, Default::default()),
                FormInput::MouseDown(point, Default::default()),
                FormInput::MouseUp(point, Default::default()),
            ]);
        }
        inputs.push(FormInput::KeyDown(FormKey::Escape, Default::default()));
        inputs.push(FormInput::Blur);
        for input in inputs {
            let result = form.input(0, input).unwrap();
            for event in result.events {
                let FormEvent::Invalidate { page: 0, rect } = event else {
                    continue;
                };
                assert!(rect.bottom <= rect.top);
                let points = [
                    (rect.left, rect.bottom),
                    (rect.right, rect.bottom),
                    (rect.left, rect.top),
                    (rect.right, rect.top),
                ]
                .map(|(x, y)| matrix.transform(Point { x, y }));
                let x = (points
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    - 1.)
                    .max(0.) as u32;
                let y = (points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::INFINITY, f32::min)
                    .floor()
                    - 1.)
                    .max(0.) as u32;
                let right = (points
                    .iter()
                    .map(|p| p.x)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    + 1.)
                    .max(0.)
                    .min(frame.width() as f32) as u32;
                let bottom = (points
                    .iter()
                    .map(|p| p.y)
                    .fold(f32::NEG_INFINITY, f32::max)
                    .ceil()
                    + 1.)
                    .max(0.)
                    .min(frame.height() as f32) as u32;
                if right <= x || bottom <= y {
                    continue;
                }
                let clip = DeviceRect::new(x, y, right - x, bottom - y);
                let patch = form.render(0, &config.clone().device_clip(clip)).unwrap();
                let stride = frame.stride();
                for row in 0..clip.height as usize {
                    let start = (row + clip.y as usize) * stride + clip.x as usize * 4;
                    frame.pixels_mut()[start..start + clip.width as usize * 4].copy_from_slice(
                        &patch.pixels()
                            [row * patch.stride()..row * patch.stride() + clip.width as usize * 4],
                    );
                }
            }
            let fresh = form.render(0, &config).unwrap();
            let maximum = frame
                .pixels()
                .iter()
                .zip(fresh.pixels())
                .map(|(a, b)| a.abs_diff(*b))
                .max()
                .unwrap();
            assert!(
                maximum <= 1,
                "{rotation:?}: dirty patch max difference {maximum}"
            );
        }
    }
}

#[test]
fn input_paint_callback_sees_new_state_and_generation_without_a_second_request() {
    use pdfoxium::thread::*;
    let thread = DocumentThread::spawn(
        Source::Bytes(support::fixture().into()),
        Config::default(),
        DocOptions::default(),
    )
    .unwrap();
    for input in [
        FormInput::MouseMove(Point { x: 50., y: 265. }, Default::default()),
        FormInput::MouseDown(Point { x: 50., y: 265. }, Default::default()),
        FormInput::MouseUp(Point { x: 50., y: 265. }, Default::default()),
        FormInput::SelectAll,
    ] {
        take(thread.input(0, input)).unwrap();
    }
    let (input, (text, bitmap, epoch)) = take(thread.input_with_paint(
        0,
        FormInput::Text("Combined paint".into(), Default::default()),
        |_, form, input, generation| {
            assert_eq!(input.state.text, "Combined paint");
            Ok((
                form.state()?.text,
                form.render(
                    0,
                    &RenderConfig::scale(1.).device_clip(DeviceRect::new(20, 20, 160, 30)),
                )?,
                generation,
            ))
        },
    ))
    .unwrap();
    assert_eq!(input.state.text, text);
    assert_eq!(bitmap.width(), 160);
    assert_eq!(bitmap.height(), 30);
    assert_eq!(thread.generation(), epoch);
    assert!(thread.take_events().iter().any(|event|matches!(event,DocEvent::Form{generation,damage_handled:true,events,..} if *generation==epoch && events.iter().any(|event|matches!(event,FormEvent::Invalidate{..})))));
    assert!(matches!(
        take(thread.input_with_paint(
            0,
            FormInput::Text(" retained".into(), Default::default()),
            |_, _, _, _| Err::<(), _>(Error::Unsupported("paint failed"))
        )),
        Err(Error::Unsupported("paint failed"))
    ));
    assert_eq!(
        take(thread.call(|_, form| Ok(form.state()?.text))).unwrap(),
        "Combined paint retained"
    );
    take(thread.input(0, FormInput::Blur)).unwrap();
    // Calls complete before their state publication. A barrier makes the
    // preceding publication observable without relying on scheduling sleeps.
    take(thread.call(|_, _| Ok(()))).unwrap();
    thread.take_events();
    let generation = thread.generation();
    let preview = thread.preview_generation();
    take(thread.call(|doc, _| doc.save_to(&mut Vec::new()))).unwrap();
    take(thread.call(|_, _| Ok(()))).unwrap();
    assert_eq!(thread.generation(), generation);
    assert_eq!(thread.preview_generation(), preview);
    assert!(thread.take_events().iter().any(|event| matches!(
        event,
        DocEvent::Form { visual: false, state, .. } if !state.dirty
    )));
}

#[test]
fn live_annotation_edits_commit_fields_refresh_views_and_preserve_dirty_state() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let page = doc.page(0).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    let rect = Rect {
        left: 20.0,
        bottom: 70.0,
        right: 100.0,
        top: 90.0,
    };
    assert!(matches!(
        page.add_annotation(AnnotationSpec::highlight(vec![rect])),
        Err(Error::Busy)
    ));
    click(&form, 50.0, 265.0);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(
        0,
        FormInput::Text("Before annotation".into(), Default::default()),
    )
    .unwrap();
    let id = form
        .add_annotation(0, AnnotationSpec::highlight(vec![rect]))
        .unwrap();
    assert!(form.state().unwrap().focus.is_none());
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Before annotation"
    );
    form.set_annotation_contents(0, id, "A comment").unwrap();
    assert!(form.state().unwrap().dirty && doc.is_dirty());
    let widget = page
        .annotation_ids()
        .unwrap()
        .into_iter()
        .find(|id| page.annotation(*id).unwrap().kind == AnnotationKind::Widget)
        .unwrap();
    assert!(matches!(
        form.remove_annotation(0, widget),
        Err(Error::Unsupported(_))
    ));
    let lease = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(form.remove_annotation(0, id), Err(Error::Busy)));
    drop(lease);
    form.remove_annotation(0, id).unwrap();
    assert!(matches!(page.annotation(id), Err(Error::Stale)));
    click(&form, 35.0, 165.0);
    assert!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "check")
            .unwrap()
            .checked
    );
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert!(!form.state().unwrap().dirty);
    drop(form);
    assert!(!doc.is_dirty());
    // An empty form change mark must not erase static edits after saving.
    let id = page
        .add_annotation(AnnotationSpec::text_note(rect, "Static edit"))
        .unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    assert!(form.state().unwrap().dirty);
    drop(form);
    assert!(doc.is_dirty());
    assert_eq!(
        page.annotation_string(id, b"Contents").unwrap(),
        "Static edit"
    );
}

#[test]
fn moving_notes_preserves_custom_appearance_size_and_commits_focus() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let page = doc.page(0).unwrap();
    let rect = Rect {
        left: 20.,
        bottom: 20.,
        right: 60.,
        top: 50.,
    };
    let id = page
        .add_annotation(AnnotationSpec::text_note(rect, "Move me"))
        .unwrap();
    // Custom dimensions/AP must survive; regenerating Text AP resets it to 20x20.
    page.set_annotation_bounds(id, rect).unwrap();
    page.set_annotation_appearance(id, AppearanceMode::Normal, Some("1 0 0 rg 0 0 20 20 re f"))
        .unwrap();
    let appearance = page
        .annotation_appearance(id, AppearanceMode::Normal)
        .unwrap();
    let highlight = page
        .add_annotation(AnnotationSpec::highlight(vec![rect]))
        .unwrap();
    let mut initial = Vec::new();
    doc.save_to(&mut initial).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    let revision = doc.revision();
    form.move_note(0, id, Point { x: 0., y: 0. }).unwrap();
    assert_eq!(doc.revision(), revision);
    assert!(!doc.is_dirty());
    for offset in [
        Point { x: f32::NAN, y: 0. },
        Point {
            x: 0.,
            y: f32::INFINITY,
        },
    ] {
        assert!(matches!(
            form.move_note(0, id, offset),
            Err(Error::InvalidArgument(_))
        ));
    }
    assert!(matches!(
        form.move_note(0, highlight, Point { x: 1., y: 1. }),
        Err(Error::Unsupported(_))
    ));
    let lease = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(
        form.move_note(0, id, Point { x: 1., y: 1. }),
        Err(Error::Busy)
    ));
    drop(lease);
    click(&form, 50., 265.);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(
        0,
        FormInput::Text("Committed by moving".into(), Default::default()),
    )
    .unwrap();
    form.move_note(0, id, Point { x: 70., y: 35. }).unwrap();
    assert!(form.state().unwrap().focus.is_none());
    assert!(doc.is_dirty());
    let expected = Rect {
        left: 90.,
        bottom: 55.,
        right: 130.,
        top: 85.,
    };
    assert_eq!(page.annotation(id).unwrap().rect, expected);
    assert_eq!(
        page.annotation_appearance(id, AppearanceMode::Normal)
            .unwrap(),
        appearance
    );
    assert_eq!(page.annotation_string(id, b"Contents").unwrap(), "Move me");
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Committed by moving"
    );
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert!(!doc.is_dirty());
    let reopened = Document::from_bytes(saved).unwrap();
    let page2 = reopened.page(0).unwrap();
    let moved = page2
        .annotation_ids()
        .unwrap()
        .into_iter()
        .find(|id| page2.annotation(*id).unwrap().kind == AnnotationKind::Text)
        .unwrap();
    assert_eq!(page2.annotation(moved).unwrap().rect, expected);
    assert_eq!(
        page2
            .annotation_appearance(moved, AppearanceMode::Normal)
            .unwrap(),
        appearance
    );
    form.remove_annotation(0, id).unwrap();
    assert!(matches!(
        form.move_note(0, id, Point { x: 1., y: 1. }),
        Err(Error::Stale)
    ));
}

#[test]
fn resizing_live_annotations_rebuilds_appearance_and_preserves_identity_and_style() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let page = doc.page(0).unwrap();
    let rect = Rect {
        left: 20.,
        bottom: 20.,
        right: 100.,
        top: 55.,
    };
    let specs = [
        AnnotationSpec::square(rect)
            .color(Color::rgb(200, 20, 40))
            .fill(Color::rgb(240, 220, 180)),
        AnnotationSpec::circle(rect).color(Color::rgb(20, 100, 200)),
        AnnotationSpec::free_text(rect, "Resizable text", 12.),
    ];
    let ids: Vec<_> = specs
        .into_iter()
        .map(|s| page.add_annotation(s).unwrap())
        .collect();
    let note = page
        .add_annotation(AnnotationSpec::text_note(rect, "Fixed icon"))
        .unwrap();
    let markup = page
        .add_annotation(AnnotationSpec::highlight(vec![rect]))
        .unwrap();
    let before: Vec<_> = ids
        .iter()
        .map(|id| {
            page.annotation_appearance(*id, AppearanceMode::Normal)
                .unwrap()
        })
        .collect();
    let mut initial = Vec::new();
    doc.save_to(&mut initial).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    let revision = doc.revision();
    form.resize_annotation(0, ids[0], rect).unwrap();
    assert_eq!(doc.revision(), revision);
    assert!(!doc.is_dirty());
    for id in [note, markup] {
        assert!(matches!(
            form.resize_annotation(0, id, rect),
            Err(Error::Unsupported(_))
        ));
    }
    for invalid in [
        Rect {
            right: f32::INFINITY,
            ..rect
        },
        Rect {
            right: rect.left,
            ..rect
        },
        Rect {
            left: rect.right + 1.,
            ..rect
        },
    ] {
        assert!(matches!(
            form.resize_annotation(0, ids[0], invalid),
            Err(Error::InvalidArgument(_))
        ));
    }
    let new = Rect {
        left: 20.,
        bottom: 20.,
        right: 160.,
        top: 90.,
    };
    let lease = page.render_job(&RenderConfig::default()).unwrap();
    assert!(matches!(
        form.resize_annotation(0, ids[0], new),
        Err(Error::Busy)
    ));
    drop(lease);
    click(&form, 50., 265.);
    form.input(0, FormInput::SelectAll).unwrap();
    form.input(
        0,
        FormInput::Text("Committed by resize".into(), Default::default()),
    )
    .unwrap();
    for (i, id) in ids.iter().enumerate() {
        let details = page.annotation(*id).unwrap();
        form.resize_annotation(0, *id, new).unwrap();
        let resized = page.annotation(*id).unwrap();
        assert_eq!(resized.rect, new);
        assert_eq!(resized.kind, details.kind);
        assert_eq!(resized.border, details.border);
        assert_ne!(
            page.annotation_appearance(*id, AppearanceMode::Normal)
                .unwrap(),
            before[i]
        );
    }
    assert!(form.state().unwrap().focus.is_none() && doc.is_dirty());
    assert_eq!(
        form.fields(0)
            .unwrap()
            .iter()
            .find(|f| f.name == "name")
            .unwrap()
            .value,
        "Committed by resize"
    );
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    assert!(!doc.is_dirty());
    let reopened = Document::from_bytes(saved).unwrap();
    let reopened = reopened.page(0).unwrap();
    for kind in [
        AnnotationKind::Square,
        AnnotationKind::Circle,
        AnnotationKind::FreeText,
    ] {
        let id = reopened
            .annotation_ids()
            .unwrap()
            .into_iter()
            .find(|id| reopened.annotation(*id).unwrap().kind == kind)
            .unwrap();
        assert_eq!(reopened.annotation(id).unwrap().rect, new);
    }
    form.remove_annotation(0, ids[0]).unwrap();
    assert!(matches!(
        form.resize_annotation(0, ids[0], new),
        Err(Error::Stale)
    ));
}

#[test]
fn moving_boxes_preserves_appearance_dimensions_and_live_identity() {
    let doc = Document::from_bytes(support::fixture()).unwrap();
    let page = doc.page(0).unwrap();
    let rect = Rect {
        left: 20.,
        bottom: 20.,
        right: 100.,
        top: 80.,
    };
    let specs = [
        AnnotationSpec::free_text(rect, "First\nSecond", 12.),
        AnnotationSpec::square(rect),
        AnnotationSpec::circle(rect),
    ];
    let ids: Vec<_> = specs
        .into_iter()
        .map(|s| page.add_annotation(s).unwrap())
        .collect();
    page.set_annotation_appearance(
        ids[1],
        AppearanceMode::Normal,
        Some("0 0 1 rg 20 20 80 60 re f"),
    )
    .unwrap();
    let before: Vec<_> = ids
        .iter()
        .map(|id| {
            page.annotation_appearance(*id, AppearanceMode::Normal)
                .unwrap()
        })
        .collect();
    let mut initial = Vec::new();
    doc.save_to(&mut initial).unwrap();
    let form = doc.forms(FormOptions::default()).unwrap();
    let revision = doc.revision();
    form.move_annotation(0, ids[0], Point::default()).unwrap();
    assert_eq!(doc.revision(), revision);
    assert!(!doc.is_dirty());
    assert!(matches!(
        form.move_note(0, ids[0], Point { x: 1., y: 1. }),
        Err(Error::Unsupported(_))
    ));
    for (i, id) in ids.iter().enumerate() {
        form.move_annotation(0, *id, Point { x: 35., y: 20. })
            .unwrap();
        assert_eq!(
            page.annotation(*id).unwrap().rect,
            Rect {
                left: 55.,
                bottom: 40.,
                right: 135.,
                top: 100.
            }
        );
        assert_eq!(
            page.annotation_appearance(*id, AppearanceMode::Normal)
                .unwrap(),
            before[i]
        );
    }
    let mut saved = Vec::new();
    doc.save_to(&mut saved).unwrap();
    let reopened = Document::from_bytes(saved).unwrap();
    let reopened = reopened.page(0).unwrap();
    for (i, id) in reopened
        .annotation_ids()
        .unwrap()
        .into_iter()
        .filter(|id| {
            matches!(
                reopened.annotation(*id).unwrap().kind,
                AnnotationKind::FreeText | AnnotationKind::Square | AnnotationKind::Circle
            )
        })
        .enumerate()
    {
        assert_eq!(
            reopened.annotation(id).unwrap().rect,
            Rect {
                left: 55.,
                bottom: 40.,
                right: 135.,
                top: 100.
            }
        );
        assert_eq!(
            reopened
                .annotation_appearance(id, AppearanceMode::Normal)
                .unwrap(),
            before[i]
        );
    }
}
