// Contains code adapted from PDFium for pdfoxium.
// Adapted code is subject to the original license terms below.
//
// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.
use eframe::egui;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Whole,
    Tiled,
}
impl Mode {
    /// Strict enter/exit thresholds. GPU limits always override visibility and
    /// hysteresis; scrolling alone cannot exit tiled mode above the lower bound.
    pub fn update(
        self,
        page: (u32, u32),
        visible_area: u64,
        screen_area: u64,
        max_texture_side: u32,
    ) -> Self {
        if page.0 > max_texture_side || page.1 > max_texture_side {
            return Self::Tiled;
        }
        if screen_area == 0 {
            return self;
        }
        let area = u128::from(page.0) * u128::from(page.1);
        let screen = u128::from(screen_area);
        match self {
            Self::Whole if area > 4 * screen && u128::from(visible_area) * 4 < area * 3 => {
                Self::Tiled
            }
            Self::Tiled if area < 3 * screen => Self::Whole,
            mode => mode,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Whole => "whole",
            Self::Tiled => "tiled",
        }
    }
}
#[derive(Clone, Copy)]
pub struct Screen {
    pub pixels: (u32, u32),
    pub monitor: bool,
    pub max_texture_side: u32,
}
impl Screen {
    pub fn read(ctx: &egui::Context) -> Self {
        let (monitor, window, ppp, limit) = ctx.input(|i| {
            (
                i.viewport().monitor_size,
                i.content_rect().size(),
                i.pixels_per_point,
                i.max_texture_side,
            )
        });
        // egui-winit divides the physical monitor size by effective pixels per
        // point. Multiplying it back also handles native DPI and UI scale.
        let valid = |size: egui::Vec2| size.is_finite() && size.x > 0.0 && size.y > 0.0;
        let monitor = monitor.filter(|s| valid(*s));
        let size = monitor.unwrap_or(window);
        let physical = |v: f32| {
            (f64::from(v) * f64::from(ppp))
                .round()
                .clamp(1.0, f64::from(u32::MAX)) as u32
        };
        Self {
            pixels: (physical(size.x), physical(size.y)),
            monitor: monitor.is_some(),
            max_texture_side: limit.min(u32::MAX as usize) as u32,
        }
    }
    pub fn refresh(self, ctx: &egui::Context) -> Self {
        let mut screen = Self::read(ctx);
        // Monitor metadata may briefly be unavailable while moving a window.
        // Keep the last monitor area instead of changing to a window threshold.
        if !screen.monitor && self.monitor {
            screen.pixels = self.pixels;
            screen.monitor = true;
        }
        screen
    }
    pub fn area(self) -> u64 {
        u64::from(self.pixels.0) * u64::from(self.pixels.1)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_thresholds_visibility_and_hysteresis() {
        let update = |mode, page, visible| Mode::update(mode, (page, 1), visible, 1000, 10000);
        assert_eq!(update(Mode::Whole, 4000, 0), Mode::Whole);
        assert_eq!(update(Mode::Whole, 4001, 3000), Mode::Tiled);
        assert_eq!(update(Mode::Whole, 4004, 3003), Mode::Whole); // exactly 75%
        assert_eq!(update(Mode::Whole, 4004, 3002), Mode::Tiled);
        assert_eq!(update(Mode::Whole, 5000, 5000), Mode::Whole);
        assert_eq!(update(Mode::Tiled, 5000, 5000), Mode::Tiled);
        assert_eq!(update(Mode::Tiled, 3000, 0), Mode::Tiled);
        assert_eq!(update(Mode::Tiled, 2999, 0), Mode::Whole);
        for mode in [Mode::Whole, Mode::Tiled] {
            for area in [3000, 3500, 4000] {
                assert_eq!(update(mode, area, u64::from(area)), mode);
            }
        }
    }
    #[test]
    fn gpu_limit_overrides_area_visibility_and_exit() {
        assert_eq!(
            Mode::Whole.update((2049, 1), 2049, 10_000_000, 2048),
            Mode::Tiled
        );
        assert_eq!(
            Mode::Tiled.update((2049, 1), 2049, 10_000_000, 2048),
            Mode::Tiled
        );
        assert_eq!(
            Mode::Tiled.update((2048, 1), 2048, 10_000_000, 2048),
            Mode::Whole
        );
        assert_eq!(
            Mode::Whole.update((1, 2049), 2049, 10_000_000, 2048),
            Mode::Tiled
        );
        // No threshold multiplication overflows at the public u32/u64 bounds.
        assert_eq!(
            Mode::Whole.update((u32::MAX, u32::MAX), 0, u64::MAX, u32::MAX),
            Mode::Whole
        );
    }
    #[test]
    fn monitor_physical_pixels_use_dpi_not_window_area() {
        let ctx = egui::Context::default();
        let mut input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(800.0, 600.0),
            )),
            ..Default::default()
        };
        let viewport = input.viewports.get_mut(&egui::ViewportId::ROOT).unwrap();
        viewport.native_pixels_per_point = Some(2.0);
        viewport.monitor_size = Some(egui::vec2(1920.0, 1080.0));
        let _ = crate::test_frame(&ctx, input, |ctx| {
            let screen = Screen::read(ctx);
            assert_eq!(screen.pixels, (3840, 2160));
            assert!(screen.monitor);
        });
        let _ = crate::test_frame(
            &ctx,
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            },
            |ctx| {
                let screen = Screen::read(ctx);
                assert!(!screen.monitor);
                assert!(screen.area() > 0);
                let previous = Screen {
                    pixels: (3840, 2160),
                    monitor: true,
                    max_texture_side: screen.max_texture_side,
                };
                assert_eq!(previous.refresh(ctx).pixels, (3840, 2160));
            },
        );
    }
}
