// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

use std::path::PathBuf;

use crate::{Error, Result};

pub use pdfoxium_core::fpdfapi::page::image_memory::{
    image_memory_limits, set_image_memory_limits, ImageMemoryLimits,
};
pub use pdfoxium_core::fxge::font_memory::{
    font_memory_limits, font_memory_usage, set_font_memory_limits, trim_font_memory,
    FontMemoryLimits, FontMemoryUsage,
};

/// Configuration for one thread's library instance.
///
/// Installation is optional: opening a document initializes default settings.
/// A configuration can be cloned and sent to worker threads; install it on each
/// worker before using the library. This does not make documents thread-safe.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Config {
    /// Directories to scan for system fonts. `None` uses platform defaults;
    /// `Some(vec![])` deliberately disables system font directory scanning.
    /// Paths are compared literally and can only be chosen before initialization.
    pub font_paths: Option<Vec<PathBuf>>,
    /// Retained glyph cache and scratch budgets, shared across this thread.
    pub font_memory: FontMemoryLimits,
    /// Threshold for realizing new page image-cache color planes.
    /// This is not a total image-memory budget; existing entries remain cached.
    pub image_memory: ImageMemoryLimits,
}

impl Config {
    /// Installs this configuration on the current thread.
    ///
    /// Call before opening a document or using font-memory controls to choose
    /// font paths. Once initialized (including through the C API), installing
    /// different paths returns [`Error::InvalidArgument`] without changing
    /// either memory setting. Reinstalling the same paths updates memory limits.
    /// Use [`set_font_memory_limits`] and [`set_image_memory_limits`] to adjust
    /// memory independently of startup settings.
    ///
    /// ```
    /// use pdfoxium::{Config, ImageMemoryLimits};
    /// Config {
    ///     image_memory: ImageMemoryLimits { cached_image_bytes: 128 * 1024 * 1024 },
    ///     ..Config::default()
    /// }.install()?;
    /// # Ok::<(), pdfoxium::Error>(())
    /// ```
    pub fn install(&self) -> Result<()> {
        if !pdfoxium_core::fxge::cfx_gemodule::initialize_ge_module_with_paths(
            self.font_paths.clone(),
        ) {
            return Err(Error::InvalidArgument(
                "font paths cannot change after this thread's library is initialized",
            ));
        }
        set_font_memory_limits(self.font_memory);
        set_image_memory_limits(self.image_memory);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn configuration_is_send_and_thread_local() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Config>();
        // Fresh threads avoid depending on the test runner's reused TLS state.
        std::thread::spawn(|| {
            let config = Config {
                font_paths: Some(Vec::new()),
                font_memory: FontMemoryLimits {
                    glyph_cache_bytes: 123,
                    glyph_scratch_bytes: 456,
                },
                image_memory: ImageMemoryLimits {
                    cached_image_bytes: 789,
                },
            };
            config.install().unwrap();
            config.install().unwrap();
            assert_eq!(font_memory_limits(), config.font_memory);
            assert_eq!(image_memory_limits(), config.image_memory);

            let worker_config = config.clone();
            std::thread::spawn(move || {
                worker_config.install().unwrap();
                assert_eq!(font_memory_limits(), worker_config.font_memory);
                assert_eq!(image_memory_limits(), worker_config.image_memory);
            })
            .join()
            .unwrap();

            let sent = config.clone();
            std::thread::spawn(move || {
                assert_eq!(font_memory_limits(), FontMemoryLimits::default());
                assert_eq!(image_memory_limits(), ImageMemoryLimits::default());
                // Reading font settings already initialized this child with
                // default paths, so an explicit empty path list is different.
                assert!(matches!(sent.install(), Err(Error::InvalidArgument(_))));
                assert_eq!(font_memory_limits(), FontMemoryLimits::default());
                assert_eq!(image_memory_limits(), ImageMemoryLimits::default());
                set_font_memory_limits(FontMemoryLimits::UNLIMITED);
                set_image_memory_limits(ImageMemoryLimits::UNLIMITED);
            })
            .join()
            .unwrap();
            assert_eq!(font_memory_limits(), config.font_memory);
            assert_eq!(image_memory_limits(), config.image_memory);

            let changed_paths = Config {
                font_paths: None,
                ..Config::default()
            };
            assert!(matches!(
                changed_paths.install(),
                Err(Error::InvalidArgument(_))
            ));
            assert_eq!(font_memory_limits(), config.font_memory);
            assert_eq!(image_memory_limits(), config.image_memory);

            Config {
                font_memory: FontMemoryLimits::UNLIMITED,
                image_memory: ImageMemoryLimits::UNLIMITED,
                ..config
            }
            .install()
            .unwrap();
            assert_eq!(font_memory_limits(), FontMemoryLimits::UNLIMITED);
            assert_eq!(image_memory_limits(), ImageMemoryLimits::UNLIMITED);
            trim_font_memory();
            assert_eq!(font_memory_usage().glyph_cache_bytes, 0);
            assert_eq!(font_memory_usage().glyph_scratch_bytes, 0);
        })
        .join()
        .unwrap();
    }

    #[test]
    fn existing_core_initialization_and_lifecycle_are_respected() {
        std::thread::spawn(|| {
            use pdfoxium_core::fxge::cfx_gemodule::{destroy_ge_module, initialize_ge_module};
            let paths = Some(vec![PathBuf::from("/explicit/font/path")]);
            initialize_ge_module(paths.clone());
            assert!(matches!(
                Config::default().install(),
                Err(Error::InvalidArgument(_))
            ));
            Config {
                font_paths: paths,
                ..Config::default()
            }
            .install()
            .unwrap();
            set_font_memory_limits(FontMemoryLimits::UNLIMITED);
            destroy_ge_module();
            Config::default().install().unwrap();
            assert_eq!(font_memory_limits(), FontMemoryLimits::default());
        })
        .join()
        .unwrap();
    }
}
