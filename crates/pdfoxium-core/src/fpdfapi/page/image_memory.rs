// Copyright 2026 The PDFium Authors
// Use of this source code is governed by a BSD-style license that can be
// found in upstream/pdfium/LICENSE.

//! Calling-thread policy for realizing newly decoded page image color planes.
//! This is not an aggregate memory budget. Masks and decoder storage are
//! excluded; existing realized images remain cached until their page is unloaded.

use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageMemoryLimits {
    /// Realize a color plane only when its pitch times height is strictly below
    /// this limit. Zero keeps it lazy; `usize::MAX` always realizes it.
    /// The default preserves PDFium's 60,000,000-byte threshold.
    pub cached_image_bytes: usize,
}

impl Default for ImageMemoryLimits {
    fn default() -> Self {
        Self {
            cached_image_bytes: 60_000_000,
        }
    }
}

impl ImageMemoryLimits {
    pub const UNLIMITED: Self = Self {
        cached_image_bytes: usize::MAX,
    };

    pub(crate) fn realizes(self, pitch: u32, height: i32) -> bool {
        let Ok(height) = u64::try_from(height) else {
            return false;
        };
        self.cached_image_bytes == usize::MAX
            || u64::from(pitch) * height < self.cached_image_bytes as u64
    }
}

thread_local! {
    static LIMITS: Cell<ImageMemoryLimits> = Cell::new(ImageMemoryLimits::default());
}

pub fn image_memory_limits() -> ImageMemoryLimits {
    LIMITS.with(Cell::get)
}

/// Changes subsequent decode decisions on this thread; does not evict existing
/// entries or alter rendering's independent resampling thresholds.
pub fn set_image_memory_limits(limits: ImageMemoryLimits) {
    LIMITS.with(|current| current.set(limits));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strict_threshold_zero_unlimited_and_nonwrapping_size() {
        let limits = ImageMemoryLimits::default();
        assert!(limits.realizes(299_999, 200));
        assert!(!limits.realizes(300_000, 200));
        assert!(!limits.realizes(u32::MAX, 2));
        assert!(!ImageMemoryLimits {
            cached_image_bytes: 0
        }
        .realizes(4, 1));
        assert!(ImageMemoryLimits::UNLIMITED.realizes(u32::MAX, i32::MAX));
        assert!(!ImageMemoryLimits::UNLIMITED.realizes(4, -1));
    }
}
