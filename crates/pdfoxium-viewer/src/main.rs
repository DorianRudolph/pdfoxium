// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
#![forbid(unsafe_code)]
mod annotation_geometry;
mod annotation_style;
mod annotations;
mod app;
mod chrome;
mod control;
mod demos;
mod editing;
mod form_view;
mod layout;
mod navigation;
mod outline;
mod platform;
mod policy;
mod text;
mod texture;
mod thumbnails;
mod typing_trace;
mod view;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
#[cfg(not(target_arch = "wasm32"))]
fn run() -> Result<(), String> {
    let mut args = std::env::args().skip(1).peekable();
    if args.peek().is_some_and(|s| s == "--help" || s == "-h") {
        println!("pdfoxium-viewer [PDF] [--control FILE] [--control-only-input]\npdfoxium-viewer ui --control FILE METHOD [JSON_PARAMS]\n\nExamples:\n  pdfoxium-viewer input.pdf --control /private/path/viewer.json\n  pdfoxium-viewer ui --control /private/path/viewer.json ui.state\n  pdfoxium-viewer ui --control /private/path/viewer.json ui.command '{{\"id\":\"view.zoom\",\"zoom\":2}}'\n  pdfoxium-viewer ui --control /private/path/viewer.json ui.screenshot '{{\"path\":\"/tmp/viewer.png\"}}'");
        return Ok(());
    }
    if args.peek().is_some_and(|s| s == "ui") {
        args.next();
        if args.next().as_deref() != Some("--control") {
            return Err("expected ui --control FILE METHOD [JSON_PARAMS]".into());
        }
        let path = args.next().ok_or("missing control file")?;
        let method = args.next().ok_or("missing method")?;
        let params = args
            .next()
            .map(|p| serde_json::from_str(&p))
            .transpose()
            .map_err(|e| e.to_string())?
            .unwrap_or(serde_json::json!({}));
        if args.next().is_some() {
            return Err("unexpected extra arguments".into());
        }
        let response = control::call(std::path::Path::new(&path), method, params)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&response).map_err(|e| e.to_string())?
        );
        if response.get("error").is_some() {
            return Err("control request failed".into());
        }
        return Ok(());
    }
    let mut path = None;
    let mut control = None;
    let mut control_only_input = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--control-only-input" => control_only_input = true,
            "--control" => {
                if control.is_some() {
                    return Err("duplicate --control".into());
                }
                control = Some(args.next().ok_or("missing control file")?.into());
            }
            value if value.starts_with('-') => return Err(format!("unknown option {value}")),
            _ if path.is_none() => path = Some(arg.into()),
            _ => return Err("only one PDF can be opened at a time".into()),
        }
    }
    if control_only_input && control.is_none() {
        return Err("--control-only-input requires --control".into());
    }
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        wgpu_options: eframe::egui_wgpu::WgpuConfiguration {
            // Prefer input responsiveness over queuing extra GPU frames.
            surface: eframe::egui_wgpu::SurfaceConfig {
                desired_maximum_frame_latency: Some(1),
                ..eframe::egui_wgpu::SurfaceConfig::LOW_LATENCY
            },
            ..Default::default()
        },
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 850.0])
            .with_min_inner_size([480.0, 320.0]),
        ..Default::default()
    };
    eframe::run_native(
        "pdfoxium",
        options,
        Box::new(move |cc| {
            let mut app =
                app::App::new(&cc.egui_ctx, path, control).map_err(std::io::Error::other)?;
            app.set_control_only_input(control_only_input);
            Ok(Box::new(app))
        }),
    )
    .map_err(|e| e.to_string())
}

#[cfg(target_arch = "wasm32")]
fn main() {}

#[cfg(test)]
fn test_frame(
    ctx: &eframe::egui::Context,
    input: eframe::egui::RawInput,
    ui: impl FnMut(&mut eframe::egui::Ui),
) -> eframe::egui::FullOutput {
    let mut output = ctx.run_ui(input, ui);
    // These headless tests inspect layout/input/output without a GPU. Egui 0.37
    // requires explicitly discarding texture uploads when they are not rendered.
    output.textures_delta.clear();
    output
}
